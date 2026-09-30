// The ICS-43434 on I2S (research/2026-09-29-esp32-firmware.md §2): 16 kHz, one 32-bit slot per
// channel (64 clocks a frame, 1.024 MHz), the mic on the left. Its 24 bits sit at the top of
// each word. Not provable before hardware: QEMU and Wokwi have no I2S.
#include "hw.h"

#include "board_pins.h"
#include "driver/i2s_std.h"
#include "freertos/FreeRTOS.h"

// 16 bits from the top would be unity gain, and speech at arm's length is then very quiet
// (about -55 dBFS, INFERRED). Two bits more is +12 dB; tune at bring-up.
#define MIC_SHIFT 14
#define SETTLE_MS 50 // the first samples after power-on are the mic settling

static i2s_chan_handle_t rx;

// The clocks, as pins_mic() asks for them: after power is on, and gone before it goes off.
void mic_i2s_run(bool run)
{
    if (run && !rx) {
        i2s_chan_config_t chan = I2S_CHANNEL_DEFAULT_CONFIG(I2S_NUM_AUTO, I2S_ROLE_MASTER);
        chan.dma_frame_num = 320; // 20 ms, one Opus frame
        chan.dma_desc_num = 16;   // 320 ms of room while flash is written
        if (i2s_new_channel(&chan, NULL, &rx) != ESP_OK) {
            rx = NULL;
            return;
        }
        i2s_std_config_t std = {
            .clk_cfg = I2S_STD_CLK_DEFAULT_CONFIG(HW_SAMPLE_RATE),
            .slot_cfg = I2S_STD_PHILIPS_SLOT_DEFAULT_CONFIG(I2S_DATA_BIT_WIDTH_32BIT, I2S_SLOT_MODE_MONO),
            .gpio_cfg = {.mclk = I2S_GPIO_UNUSED, .bclk = PIN_I2S_SCK, .ws = PIN_I2S_WS, .dout = I2S_GPIO_UNUSED, .din = PIN_I2S_SD},
        };
        std.slot_cfg.slot_mask = I2S_STD_SLOT_LEFT; // the default is both, even in mono
        if (i2s_channel_init_std_mode(rx, &std) != ESP_OK || i2s_channel_enable(rx) != ESP_OK) {
            i2s_del_channel(rx);
            rx = NULL;
        }
    } else if (!run && rx) {
        i2s_channel_disable(rx);
        i2s_del_channel(rx); // lets the pins go; pins_mic() then grounds them
        rx = NULL;
    }
}

bool hw_mic_start(void)
{
    pins_mic(hw_pin_ops(), true);
    if (!rx) {
        pins_mic(hw_pin_ops(), false);
        return false;
    }
    int16_t scrap[160];
    for (int ms = 0; ms < SETTLE_MS; ms += 10) {
        hw_mic_read(scrap, 160);
    }
    return true;
}

int hw_mic_read(int16_t *pcm, int samples)
{
    int32_t raw[160];
    int done = 0;
    while (rx && done < samples) {
        int want = samples - done < 160 ? samples - done : 160;
        size_t got = 0;
        if (i2s_channel_read(rx, raw, (size_t)want * 4, &got, pdMS_TO_TICKS(500)) != ESP_OK || got == 0) {
            break;
        }
        for (size_t i = 0; i < got / 4; i++) {
            int32_t s = raw[i] >> MIC_SHIFT;
            pcm[done++] = (int16_t)(s > 32767 ? 32767 : s < -32768 ? -32768 : s);
        }
    }
    return done;
}

void hw_mic_stop(void)
{
    pins_mic(hw_pin_ops(), false);
}
