// The recorder (Phase D.1's spike made into a task): microphone -> Opus 16 kHz mono 32 kbps CBR,
// 20 ms frames -> Ogg with about a second of packets per page -> <id>.ogg, fsync'd page by page.
// A power cut therefore loses at most the last second, and what is on flash decodes.
#include "app.h"

#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include "encoder/impl/esp_opus_enc.h"
#include "esp_audio_enc.h"
#include "esp_muxer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "freertos/task.h"
#include "hw.h"
#include "impl/ogg_muxer.h"

#define BITRATE 32000
#define FRAME_MS 20
#define PAGE_CACHE 4096 // about 1 s of 80-byte packets per page: 2 % overhead instead of 35 %
// The Opus encoder used 23 KB of stack in QEMU (D.1). On the chip's own RAM, not PSRAM: the
// task writes flash, and the PSRAM is unreachable while flash is written.
#define STACK (32 * 1024)

static struct {
    TaskHandle_t task;
    SemaphoreHandle_t go, done;
    volatile bool run, busy;
    char path[80];
    void (*failed)(void);
    int fd;
    bool write_error;
    int64_t frames;
} rec;

// ---- the muxer's file: ours, so every page is on flash before the next is made ----

static void *file_open(char *path)
{
    (void)path;
    rec.fd = open(rec.path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    return rec.fd < 0 ? NULL : &rec;
}

static int file_write(void *file, void *data, int len)
{
    (void)file;
    if (write(rec.fd, data, (size_t)len) != len || fsync(rec.fd) != 0) {
        rec.write_error = true;
        return -1;
    }
    return len;
}

static int file_seek(void *file, uint64_t position)
{
    (void)file;
    return lseek(rec.fd, (off_t)position, SEEK_SET) < 0 ? -1 : 0; // the Ogg muxer never does (D.1)
}

static int file_close(void *file)
{
    (void)file;
    int fd = rec.fd;
    rec.fd = -1;
    if (fd < 0) {
        return 0;
    }
    bool ok = fsync(fd) == 0;
    return close(fd) == 0 && ok ? 0 : -1;
}

static int file_name(char *path, int len, int slice)
{
    (void)slice;
    snprintf(path, (size_t)len, "%s", rec.path);
    return 0;
}

// One recording, until rec.run goes false or something fails. True when it ended by being stopped.
static bool record(void)
{
    bool ok = false, mic = false;
    esp_audio_enc_handle_t encoder = NULL;
    esp_muxer_handle_t muxer = NULL;
    uint8_t *packet = NULL;
    int16_t *pcm = NULL;
    rec.frames = 0;
    rec.write_error = false;

    esp_opus_enc_config_t opus = ESP_OPUS_ENC_CONFIG_DEFAULT();
    opus.sample_rate = HW_SAMPLE_RATE;
    opus.channel = 1;
    opus.bitrate = BITRATE;
    opus.frame_duration = ESP_OPUS_ENC_FRAME_DURATION_20_MS;
    opus.application_mode = ESP_OPUS_ENC_APPLICATION_VOIP;
    opus.complexity = 0; // until Phase D.5 has the encoder's cost on the chip
    esp_audio_enc_config_t enc_cfg = {.type = ESP_AUDIO_TYPE_OPUS, .cfg = &opus, .cfg_sz = sizeof opus};
    int in_size = 0, out_size = 0;
    if (esp_audio_enc_open(&enc_cfg, &encoder) != ESP_AUDIO_ERR_OK ||
        esp_audio_enc_get_frame_size(encoder, &in_size, &out_size) != ESP_AUDIO_ERR_OK ||
        in_size != HW_SAMPLE_RATE / 1000 * FRAME_MS * 2) {
        goto end;
    }
    packet = malloc((size_t)out_size);
    pcm = malloc((size_t)in_size);
    ogg_muxer_config_t ogg = {
        .base_config = {.muxer_type = ESP_MUXER_TYPE_OGG, .slice_duration = ESP_MUXER_MAX_SLICE_DURATION, .url_pattern = file_name},
        .page_cache_size = PAGE_CACHE,
    };
    muxer = esp_muxer_open(&ogg.base_config, sizeof ogg);
    esp_muxer_file_writer_t writer = {file_open, file_write, file_seek, file_close};
    esp_muxer_audio_stream_info_t info = {
        .codec = ESP_MUXER_ADEC_OPUS, .channel = 1, .bits_per_sample = 16, .sample_rate = HW_SAMPLE_RATE, .min_packet_duration = FRAME_MS};
    int stream = -1;
    if (!packet || !pcm || !muxer || esp_muxer_set_file_writer(muxer, &writer) != ESP_MUXER_ERR_OK ||
        esp_muxer_add_audio_stream(muxer, &info, &stream) != ESP_MUXER_ERR_OK) {
        goto end;
    }
    mic = hw_mic_start();
    if (!mic) {
        goto end;
    }
    int samples = in_size / 2;
    while (rec.run) {
        if (hw_mic_read(pcm, samples) != samples) {
            goto end;
        }
        esp_audio_enc_in_frame_t in = {.buffer = (uint8_t *)pcm, .len = (uint32_t)in_size};
        esp_audio_enc_out_frame_t out = {.buffer = packet, .len = (uint32_t)out_size};
        if (esp_audio_enc_process(encoder, &in, &out) != ESP_AUDIO_ERR_OK) {
            goto end;
        }
        esp_muxer_audio_packet_t pkt = {.data = packet, .len = (int)out.encoded_bytes, .pts = (uint32_t)out.pts};
        if (esp_muxer_add_audio_packet(muxer, stream, &pkt) != ESP_MUXER_ERR_OK || rec.write_error) {
            goto end;
        }
        rec.frames++;
        // once a second: is there still room? (the reserve is for the small files of an upload)
        if (rec.frames % 50 == 0 && store_free_bytes() < STORE_RESERVE_BYTES) {
            goto end;
        }
    }
    ok = true;
end:
    if (mic) {
        hw_mic_stop();
    }
    if (muxer) {
        esp_muxer_close(muxer); // writes the last page and closes the file
    }
    file_close(NULL);
    if (encoder) {
        esp_audio_enc_close(encoder);
    }
    free(packet);
    free(pcm);
    return ok && !rec.write_error;
}

static void task(void *arg)
{
    (void)arg;
    for (;;) {
        xSemaphoreTake(rec.go, portMAX_DELAY);
        bool stopped = record();
        bool self = rec.run; // it ended while still wanted
        rec.busy = false;
        xSemaphoreGive(rec.done);
        if ((self || !stopped) && rec.failed) {
            rec.failed();
        }
    }
}

bool recorder_init(void)
{
    rec.fd = -1;
    rec.go = xSemaphoreCreateBinary();
    rec.done = xSemaphoreCreateBinary();
    if (esp_opus_enc_register() != ESP_AUDIO_ERR_OK || ogg_muxer_register() != ESP_MUXER_ERR_OK) {
        return false;
    }
    return rec.go && rec.done && xTaskCreate(task, "recorder", STACK, NULL, 6, &rec.task) == pdPASS;
}

bool recorder_start(const char *id, void (*failed)(void))
{
    if (rec.busy || store_free_bytes() < STORE_RESERVE_BYTES + 64 * 1024) {
        return false;
    }
    char name[CLIP_NAME_LEN];
    clip_audio_name(id, name);
    store_path(name, rec.path, sizeof rec.path);
    rec.failed = failed;
    rec.run = rec.busy = true;
    xSemaphoreTake(rec.done, 0);
    xSemaphoreGive(rec.go);
    return true;
}

int64_t recorder_stop(void)
{
    if (!rec.busy && rec.frames == 0) {
        return -1;
    }
    rec.run = false;
    if (rec.busy) {
        xSemaphoreTake(rec.done, pdMS_TO_TICKS(5000));
    }
    int64_t ms = rec.frames * FRAME_MS;
    rec.frames = 0;
    return ms;
}

bool recorder_busy(void)
{
    return rec.busy;
}

unsigned recorder_stack_free(void)
{
    return rec.task ? (unsigned)uxTaskGetStackHighWaterMark(rec.task) : 0;
}
