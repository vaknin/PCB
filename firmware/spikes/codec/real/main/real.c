// Phase D.5 (docs/plan.md, D-025): the codec spike on a real ESP32-S3 (N16R8). It waits for one
// command per line on the USB Serial/JTAG console and answers with `R {json}` lines and a final
// `DONE`, so the laptop (bench.py) drives every measurement and nothing is lost while the USB
// port is away (it disappears in deep sleep):
//   status                         how this boot went (reset reason, wake cause, wake-to-app_main)
//   psram                          pattern test over the largest free PSRAM block
//   enc <cx> <mhz> <src> <mem> <save> [voip|audio]   Opus 16 kHz mono 32 kbps CBR 20 ms at complexity cx,
//                                  CPU at mhz; src speech|synth; mem psram|int (where the
//                                  encoder's state goes); save 1 writes the OGG to storage slot 0
//   scan                           passive Wi-Fi scan: counts and the strongest RSSI only
//   sleep <ms> <none|ext0|ext1>    deep sleep, timer wake after ms, GPIO0-low wake as named
//   restart                        software reset; the next `status` gives reset-to-app_main
//   dump                           the OGG saved in storage slot 0, as `B <base64>` lines
//   erase                          wipe storage slot 0 (no recording stays on the board)
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "driver/rtc_io.h"
#include "driver/usb_serial_jtag.h"
#include "driver/usb_serial_jtag_vfs.h"
#include "esp_audio_enc.h"
#include "esp_audio_enc_default.h"
#include "esp_event.h"
#include "esp_flash.h"
#include "esp_heap_caps.h"
#include "esp_muxer.h"
#include "esp_muxer_default.h"
#include "esp_netif.h"
#include "esp_partition.h"
#include "esp_pm.h"
#include "esp_private/esp_clk.h"
#include "esp_psram.h"
#include "esp_random.h"
#include "esp_sleep.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "freertos/task.h"
#include "nvs_flash.h"
#include "sdkconfig.h"

#define RATE 16000
#define BITRATE 32000
#define SLOT (1024 * 1024)
#define HDR 16
#define OUT_MAX (SLOT - HDR)
#define PAGE_CACHE 4096
#define STACK (48 * 1024)
#define SYNTH_S 20
#define WAKE_PIN 0
#define SLEEP_MAGIC 0x51EE9D05u

#if CONFIG_SPIKE_EMBED_SPEECH
extern const uint8_t wav_start[] asm("_binary_speech_wav_start");
extern const uint8_t wav_end[] asm("_binary_speech_wav_end");
#endif

// Survives deep sleep: when the sleep was asked for and for how long.
static RTC_DATA_ATTR struct {
    uint32_t magic;
    uint32_t ms;
    uint64_t rtc_us;
    char mode[8];
} asleep;

// Survives a software reset (not reloaded by the bootloader): when the restart was asked for.
static RTC_NOINIT_ATTR struct {
    uint32_t magic;
    uint64_t rtc_us;
} restarted;

// Taken at the top of app_main.
static struct {
    uint64_t rtc_us;
    int64_t timer_us;
    uint32_t causes;
    int reset;
    int64_t wake_to_main_us; // -1 when this boot was not a timed wake
    int64_t reset_to_main_us; // -1 when this boot did not follow `restart`
    uint32_t slept_ms;
    char mode[8];
} boot = {.wake_to_main_us = -1, .reset_to_main_us = -1};

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

#if CONFIG_SPIKE_EMBED_SPEECH
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
#endif

// A stand-in for speech, made here so no recording has to be on the board: a voiced source
// (pitch gliding 90-170 Hz, harmonics shaped by three formant bumps that move), hiss in every
// third syllable, a ~3.5 Hz syllable envelope and a pause every few seconds.
static int16_t *synth(uint32_t *size)
{
    static int16_t *pcm; // made once and kept: it takes longer to make than to encode
    uint32_t n = RATE * SYNTH_S;
    *size = n * 2;
    if (pcm != NULL) {
        return pcm;
    }
    pcm = heap_caps_malloc(n * 2, MALLOC_CAP_SPIRAM);
    if (pcm == NULL) {
        return NULL;
    }
    float phase = 0;
    uint32_t seed = 12345;
    for (uint32_t i = 0; i < n; i++) {
        float t = (float)i / RATE;
        float f0 = 130 + 40 * sinf(2 * M_PI * 0.7f * t);
        phase += 2 * M_PI * f0 / RATE;
        if (phase > 2 * M_PI) {
            phase -= 2 * M_PI;
        }
        float syl = sinf(2 * M_PI * 3.5f * t);
        float env = syl > 0 ? sqrtf(syl) : 0;
        if (fmodf(t, 4.0f) > 3.3f) {
            env = 0;
        }
        float f1 = 500 + 250 * sinf(2 * M_PI * 1.3f * t), f2 = 1500 + 500 * sinf(2 * M_PI * 0.9f * t);
        float s = 0;
        for (int k = 1; k * f0 < 3800; k++) {
            float f = k * f0;
            float a = expf(-(f - f1) * (f - f1) / 60000) + 0.6f * expf(-(f - f2) * (f - f2) / 120000)
                      + 0.3f * expf(-(f - 2600) * (f - 2600) / 200000) + 0.03f;
            s += a * sinf(k * phase);
        }
        seed = seed * 1664525 + 1013904223;
        float noise = (int32_t)seed / 2147483648.0f;
        bool hiss = ((int)(t * 3.5f)) % 3 == 2;
        float v = env * (hiss ? 0.25f * noise : 0.22f * s) + 0.002f * noise;
        pcm[i] = (int16_t)(v * 20000);
    }
    return pcm;
}

static const esp_partition_t *storage(void);

typedef struct {
    int complexity, mhz, save;
    char src[8], mem[8], app[8];
    SemaphoreHandle_t done;
} job_t;

static int cmp_u32(const void *a, const void *b)
{
    uint32_t x = *(const uint32_t *)a, y = *(const uint32_t *)b;
    return x < y ? -1 : x > y;
}

static void encode(job_t *job)
{
    const uint8_t *pcm = NULL;
    uint32_t pcm_size = 0;
    if (strcmp(job->src, "speech") == 0) {
#if CONFIG_SPIKE_EMBED_SPEECH
        pcm = wav_pcm(&pcm_size);
#endif
    } else {
        pcm = (const uint8_t *)synth(&pcm_size);
    }
    if (pcm == NULL) {
        printf("R {\"error\":\"no %s audio in this build\"}\n", job->src);
        return;
    }

    esp_pm_config_t pm = {.max_freq_mhz = job->mhz, .min_freq_mhz = job->mhz, .light_sleep_enable = false};
    if (esp_pm_configure(&pm) != ESP_OK) {
        printf("R {\"error\":\"cannot set %d MHz\"}\n", job->mhz);
        return;
    }
    vTaskDelay(pdMS_TO_TICKS(50)); // the switch happens as the idle task next runs
    // "int": every allocation stays in internal RAM; "psram": IDF's default (over 16 KB goes out)
    bool internal = strcmp(job->mem, "int") == 0;
    heap_caps_malloc_extmem_enable(internal ? 4 * 1024 * 1024 : CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL);

    esp_opus_enc_config_t cfg = ESP_OPUS_ENC_CONFIG_DEFAULT();
    cfg.sample_rate = RATE;
    cfg.channel = 1;
    cfg.bitrate = BITRATE;
    cfg.frame_duration = ESP_OPUS_ENC_FRAME_DURATION_20_MS;
    bool audio = strcmp(job->app, "audio") == 0;
    cfg.application_mode = audio ? ESP_OPUS_ENC_APPLICATION_AUDIO : ESP_OPUS_ENC_APPLICATION_VOIP;
    cfg.complexity = job->complexity;
    esp_audio_enc_config_t enc_cfg = {.type = ESP_AUDIO_TYPE_OPUS, .cfg = &cfg, .cfg_sz = sizeof(cfg)};

    size_t int_before = heap_caps_get_free_size(MALLOC_CAP_INTERNAL);
    size_t ext_before = heap_caps_get_free_size(MALLOC_CAP_SPIRAM);
    esp_audio_enc_handle_t enc = NULL;
    if (esp_audio_enc_open(&enc_cfg, &enc) != ESP_AUDIO_ERR_OK) {
        printf("R {\"error\":\"encoder open failed\"}\n");
        return;
    }
    size_t enc_internal = int_before - heap_caps_get_free_size(MALLOC_CAP_INTERNAL);
    size_t enc_psram = ext_before - heap_caps_get_free_size(MALLOC_CAP_SPIRAM);
    int in_size = 0, out_size = 0;
    esp_audio_enc_get_frame_size(enc, &in_size, &out_size);
    // The frame in and the packet out sit in internal RAM, as an I2S buffer would.
    uint8_t *in_buf = heap_caps_malloc(in_size, MALLOC_CAP_INTERNAL);
    uint8_t *out = heap_caps_malloc(out_size, MALLOC_CAP_INTERNAL);
    uint32_t max_frames = pcm_size / in_size;
    uint32_t *frame_us = heap_caps_malloc(max_frames * 4, MALLOC_CAP_SPIRAM); // the bench's own
    size_t int_mux = heap_caps_get_free_size(MALLOC_CAP_INTERNAL);
    size_t ext_mux = heap_caps_get_free_size(MALLOC_CAP_SPIRAM);

    ogg_muxer_config_t mcfg = {
        .base_config = {
            .muxer_type = ESP_MUXER_TYPE_OGG,
            .slice_duration = ESP_MUXER_MAX_SLICE_DURATION,
            .url_pattern = url,
        },
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
        printf("R {\"error\":\"muxer setup failed\"}\n");
        return;
    }
    sink.writes = sink.seeks = 0;
    size_t mux_internal = int_mux - heap_caps_get_free_size(MALLOC_CAP_INTERNAL);
    size_t mux_psram = ext_mux - heap_caps_get_free_size(MALLOC_CAP_SPIRAM);
    size_t largest_internal = heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL);
    size_t largest_psram = heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM);
    size_t free_internal = heap_caps_get_free_size(MALLOC_CAP_INTERNAL);

    int cpu_mhz = esp_clk_cpu_freq() / 1000000;
    int64_t enc_us = 0, mux_us = 0, t0 = esp_timer_get_time();
    uint32_t frames = 0, packet_bytes = 0, worst = 0, worst_at = 0, over = 0;
    for (uint32_t at = 0; at + in_size <= pcm_size; at += in_size) {
        memcpy(in_buf, pcm + at, in_size);
        esp_audio_enc_in_frame_t in = {.buffer = in_buf, .len = in_size};
        esp_audio_enc_out_frame_t o = {.buffer = out, .len = out_size};
        int64_t e0 = esp_timer_get_time();
        if (esp_audio_enc_process(enc, &in, &o) != ESP_AUDIO_ERR_OK) {
            printf("R {\"error\":\"encode failed at frame %u\"}\n", (unsigned)frames);
            return;
        }
        int64_t e1 = esp_timer_get_time();
        uint32_t us = (uint32_t)(e1 - e0);
        enc_us += us;
        frame_us[frames] = us;
        if (us > worst) {
            worst = us;
            worst_at = frames;
        }
        over += us > 20000;
        esp_muxer_audio_packet_t pkt = {.data = out, .len = o.encoded_bytes, .pts = (uint32_t)o.pts};
        if (esp_muxer_add_audio_packet(mux, stream, &pkt) != ESP_MUXER_ERR_OK) {
            printf("R {\"error\":\"mux failed at frame %u\"}\n", (unsigned)frames);
            return;
        }
        mux_us += esp_timer_get_time() - e1;
        frames++;
        packet_bytes += o.encoded_bytes;
    }
    // the muxer allocates its page cache lazily: measure again with everything in use
    size_t mux_internal_end = int_mux - heap_caps_get_free_size(MALLOC_CAP_INTERNAL);
    size_t mux_psram_end = ext_mux - heap_caps_get_free_size(MALLOC_CAP_SPIRAM);
    esp_muxer_close(mux);
    int64_t total_us = esp_timer_get_time() - t0;
    esp_audio_enc_close(enc);
    qsort(frame_us, frames, 4, cmp_u32);
    uint32_t p50 = frame_us[frames / 2], p99 = frame_us[frames * 99 / 100];
    free(frame_us);
    free(in_buf);
    free(out);
    heap_caps_malloc_extmem_enable(CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL);

    if (job->save) {
        const esp_partition_t *part = storage();
        uint8_t hdr[HDR] = "OGGSPIKE";
        memcpy(hdr + 8, &sink.len, 4);
        memcpy(hdr + 12, &job->complexity, 4);
        esp_partition_erase_range(part, 0, SLOT);
        esp_partition_write(part, 0, hdr, HDR);
        esp_partition_write(part, HDR, sink.buf, sink.len);
    }

    double audio_s = frames * 0.02;
    printf("R {\"test\":\"enc\",\"complexity\":%d,\"mhz_asked\":%d,\"mhz\":%d,\"src\":\"%s\",\"mem\":\"%s\",\"app\":\"%s\","
           "\"frames\":%u,\"audio_s\":%.2f,\"packet_bytes\":%u,\"ogg_bytes\":%u,\"kbps\":%.1f,"
           "\"encode_ms\":%lld,\"mux_ms\":%lld,\"total_ms\":%lld,\"rtf\":%.4f,"
           "\"frame_p50_us\":%u,\"frame_p99_us\":%u,\"frame_worst_us\":%u,\"worst_at\":%u,\"frames_over_20ms\":%u,"
           "\"encoder_internal\":%u,\"encoder_psram\":%u,\"muxer_internal\":%u,\"muxer_psram\":%u,"
           "\"muxer_internal_end\":%u,\"muxer_psram_end\":%u,\"free_internal\":%u,"
           "\"largest_internal\":%u,\"largest_psram\":%u,"
           "\"stack_used\":%u,\"stack_size\":%u,\"min_free_internal\":%u,\"min_free_psram\":%u,"
           "\"writes\":%d,\"seeks\":%d,\"saved\":%d}\n",
           job->complexity, job->mhz, cpu_mhz, job->src, job->mem, job->app, (unsigned)frames, audio_s,
           (unsigned)packet_bytes, (unsigned)sink.len, sink.len * 8 / audio_s / 1000, enc_us / 1000,
           mux_us / 1000, total_us / 1000, enc_us / 1e6 / audio_s, (unsigned)p50, (unsigned)p99,
           (unsigned)worst, (unsigned)worst_at, (unsigned)over, (unsigned)enc_internal,
           (unsigned)enc_psram, (unsigned)mux_internal, (unsigned)mux_psram, (unsigned)mux_internal_end,
           (unsigned)mux_psram_end, (unsigned)free_internal, (unsigned)largest_internal,
           (unsigned)largest_psram,
           (unsigned)(STACK - uxTaskGetStackHighWaterMark(NULL)), STACK,
           (unsigned)heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL),
           (unsigned)heap_caps_get_minimum_free_size(MALLOC_CAP_SPIRAM), sink.writes, sink.seeks, job->save);
}

// A fresh task per run, so the stack high-water mark is that run's own.
static void encode_task(void *arg)
{
    job_t *job = arg;
    encode(job);
    xSemaphoreGive(job->done);
    vTaskDelete(NULL);
}

static void cmd_enc(const char *args)
{
    job_t job = {.done = xSemaphoreCreateBinary(), .app = "voip"};
    if (sscanf(args, "%d %d %7s %7s %d %7s", &job.complexity, &job.mhz, job.src, job.mem, &job.save, job.app) < 5) {
        printf("R {\"error\":\"enc <cx> <mhz> <speech|synth> <psram|int> <save> [voip|audio]\"}\n");
        return;
    }
    xTaskCreatePinnedToCore(encode_task, "enc", STACK, &job, 5, NULL, 1);
    xSemaphoreTake(job.done, portMAX_DELAY);
    vSemaphoreDelete(job.done);
    esp_pm_config_t pm = {.max_freq_mhz = 240, .min_freq_mhz = 240};
    esp_pm_configure(&pm);
}

static void cmd_status(void)
{
    uint32_t flash = 0;
    esp_flash_get_size(NULL, &flash);
    printf("R {\"test\":\"status\",\"reset_reason\":%d,\"wake_causes\":%u,\"wake_timer\":%d,\"wake_ext0\":%d,"
           "\"wake_ext1\":%d,\"slept_ms\":%u,\"sleep_mode\":\"%s\",\"wake_to_main_us\":%lld,"
           "\"reset_to_main_us\":%lld,\"largest_internal\":%u,\"largest_psram\":%u,\"skip_validate\":%d,"
           "\"timer_at_main_us\":%lld,\"rtc_at_main_us\":%llu,\"psram_ok\":%d,\"psram_bytes\":%u,"
           "\"psram_free\":%u,\"internal_free\":%u,\"flash_bytes\":%u,\"cpu_mhz\":%d,\"memtest\":%d,"
           "\"speech\":%d}\n",
           boot.reset, (unsigned)boot.causes, !!(boot.causes & BIT(ESP_SLEEP_WAKEUP_TIMER)),
           !!(boot.causes & BIT(ESP_SLEEP_WAKEUP_EXT0)), !!(boot.causes & BIT(ESP_SLEEP_WAKEUP_EXT1)),
           (unsigned)boot.slept_ms, boot.mode, boot.wake_to_main_us, boot.reset_to_main_us,
           (unsigned)heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL),
           (unsigned)heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM),
#if CONFIG_BOOTLOADER_SKIP_VALIDATE_IN_DEEP_SLEEP
           1,
#else
           0,
#endif
           boot.timer_us, boot.rtc_us,
           esp_psram_is_initialized(), (unsigned)esp_psram_get_size(),
           (unsigned)heap_caps_get_free_size(MALLOC_CAP_SPIRAM),
           (unsigned)heap_caps_get_free_size(MALLOC_CAP_INTERNAL), (unsigned)flash,
           (int)(esp_clk_cpu_freq() / 1000000),
#if CONFIG_SPIRAM_MEMTEST
           1,
#else
           0,
#endif
#if CONFIG_SPIKE_EMBED_SPEECH
           1
#else
           0
#endif
    );
}

// Our own check on top of IDF's boot test: every word of the largest free PSRAM block holds an
// address-dependent pattern and then its inverse.
static void cmd_psram(void)
{
    size_t n = heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM) & ~3u;
    uint32_t *p = heap_caps_malloc(n, MALLOC_CAP_SPIRAM);
    if (p == NULL) {
        printf("R {\"error\":\"no PSRAM block\"}\n");
        return;
    }
    size_t words = n / 4, bad = 0;
    int64_t t0 = esp_timer_get_time();
    for (int pass = 0; pass < 2; pass++) {
        for (size_t i = 0; i < words; i++) {
            uint32_t v = (uint32_t)i * 2654435761u ^ 0xA5A5A5A5u;
            p[i] = pass ? ~v : v;
        }
        for (size_t i = 0; i < words; i++) {
            uint32_t v = (uint32_t)i * 2654435761u ^ 0xA5A5A5A5u;
            bad += p[i] != (pass ? ~v : v);
        }
    }
    int64_t us = esp_timer_get_time() - t0;
    free(p);
    printf("R {\"test\":\"psram\",\"psram_bytes\":%u,\"tested_bytes\":%u,\"bad_words\":%u,\"ms\":%lld}\n",
           (unsigned)esp_psram_get_size(), (unsigned)n, (unsigned)bad, us / 1000);
}

// Passive scan. Only the count and signal strengths leave this function: no names, no addresses.
static void cmd_scan(void)
{
    int64_t t0 = esp_timer_get_time();
    esp_err_t e = nvs_flash_init();
    if (e == ESP_ERR_NVS_NO_FREE_PAGES || e == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        nvs_flash_erase();
        nvs_flash_init();
    }
    static bool netif_up;
    if (!netif_up) {
        esp_netif_init();
        esp_event_loop_create_default();
        esp_netif_create_default_wifi_sta();
        netif_up = true;
    }
    wifi_init_config_t init = WIFI_INIT_CONFIG_DEFAULT();
    if (esp_wifi_init(&init) != ESP_OK || esp_wifi_set_storage(WIFI_STORAGE_RAM) != ESP_OK
        || esp_wifi_set_mode(WIFI_MODE_STA) != ESP_OK || esp_wifi_start() != ESP_OK) {
        printf("R {\"error\":\"wifi start failed\"}\n");
        return;
    }
    int64_t t1 = esp_timer_get_time();
    wifi_scan_config_t sc = {.scan_type = WIFI_SCAN_TYPE_PASSIVE, .scan_time.passive = 120, .show_hidden = true};
    e = esp_wifi_scan_start(&sc, true);
    int64_t t2 = esp_timer_get_time();
    uint16_t found = 0;
    esp_wifi_scan_get_ap_num(&found);
    uint16_t n = found;
    wifi_ap_record_t *aps = calloc(n ? n : 1, sizeof(*aps));
    esp_wifi_scan_get_ap_records(&n, aps);
    int best = -127, over70 = 0, ch24[15] = {0};
    for (int i = 0; i < n; i++) {
        if (aps[i].rssi > best) {
            best = aps[i].rssi;
        }
        over70 += aps[i].rssi >= -70;
        if (aps[i].primary < 15) {
            ch24[aps[i].primary]++;
        }
    }
    int channels = 0;
    for (int i = 1; i < 15; i++) {
        channels += ch24[i] > 0;
    }
    memset(aps, 0, (n ? n : 1) * sizeof(*aps));
    free(aps);
    esp_wifi_stop();
    esp_wifi_deinit();
    printf("R {\"test\":\"scan\",\"err\":%d,\"networks\":%u,\"strongest_rssi\":%d,\"at_least_minus70\":%d,"
           "\"channels_with_networks\":%d,\"wifi_start_ms\":%lld,\"scan_ms\":%lld}\n",
           (int)e, (unsigned)found, best, over70, channels, (t1 - t0) / 1000, (t2 - t1) / 1000);
}

static void cmd_sleep(const char *args)
{
    unsigned ms = 0;
    char mode[8] = "";
    if (sscanf(args, "%u %7s", &ms, mode) != 2) {
        printf("R {\"error\":\"sleep <ms> <none|ext0|ext1>\"}\n");
        return;
    }
    esp_sleep_enable_timer_wakeup((uint64_t)ms * 1000);
    if (strcmp(mode, "ext0") == 0) {
        // ext0 keeps the RTC peripherals on, so the RTC pull-up holds the pin high
        rtc_gpio_init(WAKE_PIN);
        rtc_gpio_set_direction(WAKE_PIN, RTC_GPIO_MODE_INPUT_ONLY);
        rtc_gpio_pullup_en(WAKE_PIN);
        rtc_gpio_pulldown_dis(WAKE_PIN);
        esp_sleep_enable_ext0_wakeup(WAKE_PIN, 0);
    } else if (strcmp(mode, "ext1") == 0) {
        // ext1 lets the RTC peripherals power down; keep them on for the pull-up
        esp_sleep_pd_config(ESP_PD_DOMAIN_RTC_PERIPH, ESP_PD_OPTION_ON);
        rtc_gpio_init(WAKE_PIN);
        rtc_gpio_set_direction(WAKE_PIN, RTC_GPIO_MODE_INPUT_ONLY);
        rtc_gpio_pullup_en(WAKE_PIN);
        rtc_gpio_pulldown_dis(WAKE_PIN);
        esp_sleep_enable_ext1_wakeup_io(1ULL << WAKE_PIN, ESP_EXT1_WAKEUP_ANY_LOW);
    }
    printf("R {\"test\":\"sleep\",\"ms\":%u,\"mode\":\"%s\",\"pin_level\":%d}\nDONE\n", ms, mode,
           (int)rtc_gpio_get_level(WAKE_PIN));
    vTaskDelay(pdMS_TO_TICKS(200)); // let the answer reach the laptop before USB goes away
    asleep.magic = SLEEP_MAGIC;
    asleep.ms = ms;
    strlcpy(asleep.mode, mode, sizeof(asleep.mode));
    asleep.rtc_us = esp_clk_rtc_time();
    esp_deep_sleep_start();
}

static void cmd_restart(void)
{
    printf("R {\"test\":\"restart\"}\nDONE\n");
    vTaskDelay(pdMS_TO_TICKS(200));
    restarted.magic = SLEEP_MAGIC;
    restarted.rtc_us = esp_clk_rtc_time();
    esp_restart();
}

static const esp_partition_t *storage(void)
{
    return esp_partition_find_first(ESP_PARTITION_TYPE_DATA, ESP_PARTITION_SUBTYPE_ANY, "storage");
}

static void cmd_erase(void)
{
    esp_err_t e = esp_partition_erase_range(storage(), 0, SLOT);
    printf("R {\"test\":\"erase\",\"err\":%d}\n", (int)e);
}

// The saved OGG, read back from flash (not from RAM), 57 bytes per base64 line.
static void cmd_dump(void)
{
    static const char b64[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const esp_partition_t *part = storage();
    uint8_t hdr[HDR];
    uint32_t len = 0;
    int32_t cx = 0;
    esp_partition_read(part, 0, hdr, HDR);
    memcpy(&len, hdr + 8, 4);
    memcpy(&cx, hdr + 12, 4);
    if (memcmp(hdr, "OGGSPIKE", 8) != 0 || len > OUT_MAX) {
        printf("R {\"error\":\"slot 0 is empty\"}\n");
        return;
    }
    uint32_t sum = 0;
    for (uint32_t at = 0; at < len; at += 57) {
        uint8_t in[57] = {0};
        char line[80];
        uint32_t n = len - at < 57 ? len - at : 57;
        esp_partition_read(part, HDR + at, in, n);
        int o = 0;
        for (uint32_t i = 0; i < n; i += 3) {
            uint32_t v = in[i] << 16 | in[i + 1] << 8 | in[i + 2];
            line[o++] = b64[v >> 18];
            line[o++] = b64[v >> 12 & 63];
            line[o++] = i + 1 < n ? b64[v >> 6 & 63] : '=';
            line[o++] = i + 2 < n ? b64[v & 63] : '=';
        }
        line[o] = 0;
        for (uint32_t i = 0; i < n; i++) {
            sum = sum * 31 + in[i];
        }
        printf("B %s\n", line);
    }
    printf("R {\"test\":\"dump\",\"bytes\":%u,\"complexity\":%d,\"sum31\":%u}\n", (unsigned)len, (int)cx, (unsigned)sum);
}

static void read_line(char *line, int max)
{
    int n = 0;
    for (;;) {
        char c;
        if (usb_serial_jtag_read_bytes(&c, 1, portMAX_DELAY) != 1) {
            continue;
        }
        if (c == '\n' || c == '\r') {
            if (n > 0) {
                break;
            }
            continue;
        }
        if (n < max - 1) {
            line[n++] = c;
        }
    }
    line[n] = 0;
}

static void console(void *arg)
{
    static char line[96];
    for (;;) {
        read_line(line, sizeof(line));
        if (strcmp(line, "status") == 0) {
            cmd_status();
        } else if (strcmp(line, "psram") == 0) {
            cmd_psram();
        } else if (strcmp(line, "scan") == 0) {
            cmd_scan();
        } else if (strcmp(line, "dump") == 0) {
            cmd_dump();
        } else if (strcmp(line, "erase") == 0) {
            cmd_erase();
        } else if (strcmp(line, "restart") == 0) {
            cmd_restart();
            continue;
        } else if (strncmp(line, "enc ", 4) == 0) {
            cmd_enc(line + 4);
        } else if (strncmp(line, "sleep ", 6) == 0) {
            cmd_sleep(line + 6);
            continue;
        } else {
            printf("R {\"error\":\"unknown command\"}\n");
        }
        printf("DONE\n");
    }
}

void app_main(void)
{
    boot.rtc_us = esp_clk_rtc_time();
    boot.timer_us = esp_timer_get_time();
    boot.causes = esp_sleep_get_wakeup_causes();
    boot.reset = esp_reset_reason();
    if (asleep.magic == SLEEP_MAGIC && boot.reset == ESP_RST_DEEPSLEEP) {
        boot.slept_ms = asleep.ms;
        strlcpy(boot.mode, asleep.mode, sizeof(boot.mode));
        if (boot.causes & BIT(ESP_SLEEP_WAKEUP_TIMER)) {
            boot.wake_to_main_us = (int64_t)(boot.rtc_us - asleep.rtc_us) - (int64_t)asleep.ms * 1000;
        }
    }
    asleep.magic = 0;
    if (restarted.magic == SLEEP_MAGIC && boot.reset == ESP_RST_SW) {
        boot.reset_to_main_us = (int64_t)(boot.rtc_us - restarted.rtc_us);
    }
    restarted.magic = 0;

    usb_serial_jtag_driver_config_t usb = USB_SERIAL_JTAG_DRIVER_CONFIG_DEFAULT();
    usb.rx_buffer_size = 256;
    usb.tx_buffer_size = 2048;
    usb_serial_jtag_driver_install(&usb);
    usb_serial_jtag_vfs_use_driver();
    setvbuf(stdout, NULL, _IOLBF, 0);

    sink.buf = heap_caps_malloc(OUT_MAX, MALLOC_CAP_SPIRAM);
    esp_audio_enc_register_default();
    esp_muxer_register_default();
    xTaskCreate(console, "console", 8192, NULL, 4, NULL);
}
