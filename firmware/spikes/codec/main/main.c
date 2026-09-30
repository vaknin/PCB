// Phase D.1 codec spike (docs/plan.md, D-025): encodes the embedded 16 kHz mono WAV to OGG/Opus
// 32 kbps (Capture SPEC §3) with esp_audio_codec + esp_muxer, at two encoder complexities, and
// writes each result into the `storage` partition: slot n at n * SLOT, a 16-byte header
// ("OGGSPIKE", length, complexity) and the file right after it. run.sh reads the slots back out
// of QEMU's flash image. Timings in QEMU are not those of real silicon (Phase D.5 measures them).
#include <stdio.h>
#include <string.h>
#include "esp_audio_enc.h"
#include "esp_audio_enc_default.h"
#include "esp_heap_caps.h"
#include "esp_muxer.h"
#include "esp_muxer_default.h"
#include "esp_partition.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define RATE 16000
#define BITRATE 32000
#define SLOT (1024 * 1024)
#define HDR 16
#define OUT_MAX (SLOT - HDR)
#define PAGE_CACHE 4096
#define STACK (48 * 1024)

extern const uint8_t wav_start[] asm("_binary_speech_wav_start");
extern const uint8_t wav_end[] asm("_binary_speech_wav_end");

// The muxer's file writer: the whole file is kept in PSRAM (so a seek back is harmless and is
// counted), then written to its flash slot on close.
typedef struct {
    uint8_t *buf;
    uint32_t pos, len;
    int writes, seeks;
} sink_t;

static sink_t sink;

static void *sink_open(char *path)
{
    sink.pos = sink.len = 0;
    return &sink;
}

static int sink_write(void *w, void *data, int len)
{
    if (sink.pos + len > OUT_MAX) {
        return -1;
    }
    memcpy(sink.buf + sink.pos, data, len);
    sink.pos += len;
    if (sink.pos > sink.len) {
        sink.len = sink.pos;
    }
    sink.writes++;
    return len;
}

static int sink_seek(void *w, uint64_t pos)
{
    sink.seeks++;
    sink.pos = (uint32_t)pos;
    return 0;
}

static int sink_close(void *w)
{
    return 0;
}

static int url(char *path, int len, int slice)
{
    snprintf(path, len, "slot");
    return 0;
}

// PCM samples of the embedded WAV (its "data" chunk).
static const uint8_t *wav_pcm(uint32_t *size)
{
    const uint8_t *p = wav_start + 12;
    while (p + 8 <= wav_end) {
        uint32_t n = p[4] | p[5] << 8 | p[6] << 16 | (uint32_t)p[7] << 24;
        if (memcmp(p, "data", 4) == 0) {
            *size = n;
            return p + 8;
        }
        p += 8 + n + (n & 1);
    }
    return NULL;
}

static int encode(int slot, int complexity, const uint8_t *pcm, uint32_t pcm_size)
{
    esp_opus_enc_config_t cfg = ESP_OPUS_ENC_CONFIG_DEFAULT();
    cfg.sample_rate = RATE;
    cfg.channel = 1;
    cfg.bitrate = BITRATE;
    cfg.frame_duration = ESP_OPUS_ENC_FRAME_DURATION_20_MS;
    cfg.application_mode = ESP_OPUS_ENC_APPLICATION_VOIP;
    cfg.complexity = complexity;
    esp_audio_enc_config_t enc_cfg = {.type = ESP_AUDIO_TYPE_OPUS, .cfg = &cfg, .cfg_sz = sizeof(cfg)};

    size_t int_before = heap_caps_get_free_size(MALLOC_CAP_INTERNAL);
    size_t all_before = heap_caps_get_free_size(MALLOC_CAP_8BIT);
    esp_audio_enc_handle_t enc = NULL;
    if (esp_audio_enc_open(&enc_cfg, &enc) != ESP_AUDIO_ERR_OK) {
        printf("SPIKE {\"error\":\"encoder open failed\"}\n");
        return -1;
    }
    size_t enc_internal = int_before - heap_caps_get_free_size(MALLOC_CAP_INTERNAL);
    size_t enc_heap = all_before - heap_caps_get_free_size(MALLOC_CAP_8BIT);
    int in_size = 0, out_size = 0;
    esp_audio_enc_get_frame_size(enc, &in_size, &out_size);
    uint8_t *out = malloc(out_size);

    ogg_muxer_config_t mcfg = {
        .base_config = {
            .muxer_type = ESP_MUXER_TYPE_OGG,
            .slice_duration = ESP_MUXER_MAX_SLICE_DURATION,
            .url_pattern = url,
        },
        // ~1 s of 32 kbps packets per page: ~1 % page overhead, and one page per planned fsync
        .page_cache_size = PAGE_CACHE,
    };
    esp_muxer_handle_t mux = esp_muxer_open(&mcfg.base_config, sizeof(mcfg));
    esp_muxer_file_writer_t writer = {sink_open, sink_write, sink_seek, sink_close};
    int stream = -1;
    esp_muxer_audio_stream_info_t info = {
        .codec = ESP_MUXER_ADEC_OPUS,
        .channel = 1,
        .bits_per_sample = 16,
        .sample_rate = RATE,
        .min_packet_duration = 20,
    };
    if (mux == NULL || esp_muxer_set_file_writer(mux, &writer) != ESP_MUXER_ERR_OK
        || esp_muxer_add_audio_stream(mux, &info, &stream) != ESP_MUXER_ERR_OK) {
        printf("SPIKE {\"error\":\"muxer setup failed\"}\n");
        return -1;
    }
    sink.writes = sink.seeks = 0;

    int64_t enc_us = 0, t0 = esp_timer_get_time();
    uint32_t frames = 0, packet_bytes = 0;
    for (uint32_t at = 0; at + in_size <= pcm_size; at += in_size) {
        esp_audio_enc_in_frame_t in = {.buffer = (uint8_t *)pcm + at, .len = in_size};
        esp_audio_enc_out_frame_t o = {.buffer = out, .len = out_size};
        int64_t e0 = esp_timer_get_time();
        if (esp_audio_enc_process(enc, &in, &o) != ESP_AUDIO_ERR_OK) {
            printf("SPIKE {\"error\":\"encode failed at frame %u\"}\n", (unsigned)frames);
            return -1;
        }
        enc_us += esp_timer_get_time() - e0;
        esp_muxer_audio_packet_t pkt = {.data = out, .len = o.encoded_bytes, .pts = (uint32_t)o.pts};
        if (esp_muxer_add_audio_packet(mux, stream, &pkt) != ESP_MUXER_ERR_OK) {
            printf("SPIKE {\"error\":\"mux failed at frame %u\"}\n", (unsigned)frames);
            return -1;
        }
        frames++;
        packet_bytes += o.encoded_bytes;
    }
    esp_muxer_close(mux);
    int64_t total_us = esp_timer_get_time() - t0;
    esp_audio_enc_close(enc);
    free(out);

    const esp_partition_t *part = esp_partition_find_first(ESP_PARTITION_TYPE_DATA, ESP_PARTITION_SUBTYPE_ANY, "storage");
    uint8_t hdr[HDR] = "OGGSPIKE";
    memcpy(hdr + 8, &sink.len, 4);
    memcpy(hdr + 12, &complexity, 4);
    esp_partition_erase_range(part, slot * SLOT, SLOT);
    esp_partition_write(part, slot * SLOT, hdr, HDR);
    esp_partition_write(part, slot * SLOT + HDR, sink.buf, sink.len);

    double audio_s = frames * 0.02;
    printf("SPIKE {\"slot\":%d,\"complexity\":%d,\"frames\":%u,\"audio_s\":%.2f,\"packet_bytes\":%u,"
           "\"ogg_bytes\":%u,\"kbps\":%.1f,\"encode_ms\":%lld,\"total_ms\":%lld,\"rtf\":%.3f,"
           "\"encoder_heap\":%u,\"encoder_internal\":%u,\"writes\":%d,\"seeks\":%d,\"in_frame\":%d,\"out_frame\":%d}\n",
           slot, complexity, (unsigned)frames, audio_s, (unsigned)packet_bytes, (unsigned)sink.len,
           sink.len * 8 / audio_s / 1000, enc_us / 1000, total_us / 1000, enc_us / 1e6 / audio_s,
           (unsigned)enc_heap, (unsigned)enc_internal, sink.writes, sink.seeks, in_size, out_size);
    return 0;
}

static void spike(void *arg)
{
    uint32_t pcm_size = 0;
    const uint8_t *pcm = wav_pcm(&pcm_size);
    sink.buf = heap_caps_malloc(OUT_MAX, MALLOC_CAP_SPIRAM);
    if (pcm == NULL || sink.buf == NULL) {
        printf("SPIKE {\"error\":\"no WAV data or no PSRAM\"}\n");
    } else {
        esp_audio_enc_register_default();
        esp_muxer_register_default();
        printf("SPIKE {\"pcm_bytes\":%u,\"psram_free\":%u}\n", (unsigned)pcm_size,
               (unsigned)heap_caps_get_free_size(MALLOC_CAP_SPIRAM));
        int complexities[] = {0, 5};
        for (int i = 0; i < 2; i++) {
            encode(i, complexities[i], pcm, pcm_size);
        }
    }
    printf("SPIKE {\"stack_used\":%u,\"stack_size\":%u}\n", (unsigned)(STACK - uxTaskGetStackHighWaterMark(NULL)), STACK);
    printf("SPIKE DONE\n");
    vTaskDelete(NULL);
}

void app_main(void)
{
    // Opus needs a large stack (esp_muxer's own example uses 40 KB).
    xTaskCreate(spike, "spike", STACK, NULL, 5, NULL);
}
