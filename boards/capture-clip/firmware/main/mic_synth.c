// The simulators have no I2S: a made-up voice instead (three tones under a syllable rhythm),
// handed out at the real rate so a recording lasts as long as the button says. No recording of
// a person is in this repo.
#include "hw.h"

#include <math.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define TABLE 256

static int16_t sine[TABLE];
static uint32_t phase[3], at;
static TickType_t last;

bool hw_mic_start(void)
{
    if (sine[TABLE / 4] == 0) {
        for (int i = 0; i < TABLE; i++) {
            sine[i] = (int16_t)(6000 * sinf(2 * (float)M_PI * i / TABLE));
        }
    }
    pins_mic(hw_pin_ops(), true);
    at = 0;
    last = xTaskGetTickCount();
    return true;
}

int hw_mic_read(int16_t *pcm, int samples)
{
    static const uint32_t hz[3] = {180, 720, 1850};
    for (int i = 0; i < samples; i++, at++) {
        int32_t sum = 0;
        for (int k = 0; k < 3; k++) {
            phase[k] += hz[k] * (UINT32_MAX / HW_SAMPLE_RATE);
            sum += sine[phase[k] >> 24] >> k;
        }
        // four syllables a second, a pause every third second
        uint32_t in_syllable = at % (HW_SAMPLE_RATE / 4);
        int32_t envelope = in_syllable < 2000 ? (int32_t)in_syllable : 4000 - (int32_t)in_syllable;
        if (envelope < 0 || at / HW_SAMPLE_RATE % 3 == 2) {
            envelope = 0;
        }
        pcm[i] = (int16_t)(sum * envelope / 2000);
    }
    vTaskDelayUntil(&last, pdMS_TO_TICKS(samples * 1000 / HW_SAMPLE_RATE));
    return samples;
}

void hw_mic_stop(void)
{
    pins_mic(hw_pin_ops(), false);
}
