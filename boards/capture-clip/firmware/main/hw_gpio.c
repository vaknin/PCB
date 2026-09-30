// The board's pins: real GPIO and the ADC (also in Wokwi, which has both).
#include "hw.h"

#include "board.h"
#include "board_pins.h"
#include "clip.h"
#include "driver/gpio.h"
#include "esp_adc/adc_cali.h"
#include "esp_adc/adc_cali_scheme.h"
#include "esp_adc/adc_oneshot.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

void mic_i2s_run(bool run); // mic_i2s.c on the board; nothing in Wokwi

static adc_oneshot_unit_handle_t adc;
static adc_cali_handle_t cali;
static adc_channel_t channel;

static void op_mode(void *ctx, int pin, pin_mode_t mode)
{
    (void)ctx;
    // gpio_config, not gpio_reset_pin: that one turns the pull-up on
    gpio_config_t cfg = {
        .pin_bit_mask = 1ULL << pin,
        .mode = mode == PIN_OUTPUT ? GPIO_MODE_OUTPUT : mode == PIN_OPEN_DRAIN ? GPIO_MODE_OUTPUT_OD : GPIO_MODE_INPUT,
        .pull_up_en = mode == PIN_INPUT_PULLUP ? GPIO_PULLUP_ENABLE : GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    gpio_config(&cfg);
}

static void op_level(void *ctx, int pin, int level)
{
    (void)ctx;
    gpio_set_level(pin, level);
}

static void op_hold(void *ctx, int pin, bool on)
{
    (void)ctx;
    if (on) {
        gpio_hold_en(pin);
    } else {
        gpio_hold_dis(pin);
    }
}

static void op_i2s(void *ctx, bool run)
{
    (void)ctx;
#if BOARD_IS_REAL
    mic_i2s_run(run);
#else
    (void)run;
#endif
}

static void op_delay(void *ctx, int ms)
{
    (void)ctx;
    vTaskDelay(pdMS_TO_TICKS(ms));
}

static const pin_ops_t ops = {op_mode, op_level, op_hold, op_i2s, op_delay, NULL};

const pin_ops_t *hw_pin_ops(void)
{
    return &ops;
}

void hw_init(void)
{
    pins_awake(&ops);
    adc_unit_t unit;
    adc_oneshot_io_to_channel(PIN_BAT_ADC, &unit, &channel);
    adc_oneshot_unit_init_cfg_t init = {.unit_id = unit};
    adc_oneshot_new_unit(&init, &adc);
    // 6 dB: 0-1600 mV at the pin, ±10 mV (research/2026-09-29-esp32-firmware.md §7)
    adc_oneshot_chan_cfg_t cfg = {.atten = ADC_ATTEN_DB_6, .bitwidth = ADC_BITWIDTH_DEFAULT};
    adc_oneshot_config_channel(adc, channel, &cfg);
    adc_cali_curve_fitting_config_t fit = {.unit_id = unit, .chan = channel, .atten = ADC_ATTEN_DB_6, .bitwidth = ADC_BITWIDTH_DEFAULT};
    if (adc_cali_create_scheme_curve_fitting(&fit, &cali) != ESP_OK) {
        cali = NULL;
    }
}

bool hw_button(void)
{
    return gpio_get_level(PIN_BUTTON) == 0;
}

bool hw_usb(void)
{
    return gpio_get_level(PIN_VBUS_SENSE) == 1;
}

bool hw_charging(void)
{
    return gpio_get_level(PIN_CHRG) == 0;
}

bool hw_charged(void)
{
    return gpio_get_level(PIN_STDBY) == 0;
}

int hw_battery_mv(void)
{
    // 32 readings: the divider is 3 MΩ into 100 nF, so each one is already smooth
    int sum = 0, count = 0;
    for (int i = 0; i < 32; i++) {
        int raw, mv;
        if (adc_oneshot_read(adc, channel, &raw) != ESP_OK) {
            continue;
        }
        if (cali && adc_cali_raw_to_voltage(cali, raw, &mv) == ESP_OK) {
            sum += mv;
        } else {
            sum += raw * 1600 / 4095; // uncalibrated: rough
        }
        count++;
    }
    return count ? clip_battery_cell_mv(sum / count) : 0;
}

void hw_usb_present(bool in)
{
    pins_usb(&ops, in);
}

void hw_led(uint8_t colours)
{
    pins_led(&ops, colours);
}

void hw_charge_fast(bool on)
{
    pins_charge_fast(&ops, on);
}

bool hw_sim_command(const char *line)
{
    (void)line;
    return false;
}

void hw_sim_lock_net(bool *net_down)
{
    *net_down = false;
}

int hw_sim_loaded_mv(void)
{
    return 0;
}
