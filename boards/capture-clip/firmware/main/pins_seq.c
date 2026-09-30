#include "pins_seq.h"

#include "board_pins.h"
#include "clip.h"

static const struct {
    int pin;
    uint8_t colour;
} leds[] = {{PIN_LED_R, CLIP_RED}, {PIN_LED_G, CLIP_GREEN}, {PIN_LED_B, CLIP_BLUE}};

// An output pin comes out of a hold at the level it was held at: the level first, then the
// mode, then the hold off (research/2026-09-29-esp32-firmware.md §1, "Holding pins").
static void output(const pin_ops_t *ops, int pin, int level)
{
    ops->level(ops->ctx, pin, level);
    ops->mode(ops->ctx, pin, PIN_OUTPUT);
    ops->hold(ops->ctx, pin, false);
}

void pins_awake(const pin_ops_t *ops)
{
    // the charger first: released, whatever the last firmware left
    ops->mode(ops->ctx, PIN_CHG_FAST, PIN_INPUT);
    ops->hold(ops->ctx, PIN_CHG_FAST, false);
    ops->mode(ops->ctx, PIN_BUTTON, PIN_INPUT);     // R8 pulls it up
    ops->mode(ops->ctx, PIN_BAT_ADC, PIN_INPUT);
    ops->mode(ops->ctx, PIN_VBUS_SENSE, PIN_INPUT); // its divider pulls it down
    ops->mode(ops->ctx, PIN_CHRG, PIN_INPUT);       // never a pull-up: a low would read as ~2.4 V
    ops->mode(ops->ctx, PIN_STDBY, PIN_INPUT);      // pins_usb() adds the pull-up
    ops->mode(ops->ctx, PIN_I2S_SD, PIN_INPUT);     // R9 pulls it down
    for (int i = 0; i < 3; i++) {
        output(ops, leds[i].pin, 1); // cathodes: high is dark
    }
    output(ops, PIN_MIC_PWR, 0);
    output(ops, PIN_I2S_SCK, 0);
    output(ops, PIN_I2S_WS, 0);
}

void pins_usb(const pin_ops_t *ops, bool present)
{
    ops->mode(ops->ctx, PIN_STDBY, present ? PIN_INPUT_PULLUP : PIN_INPUT);
}

void pins_led(const pin_ops_t *ops, uint8_t colours)
{
    for (int i = 0; i < 3; i++) {
        ops->level(ops->ctx, leds[i].pin, colours & leds[i].colour ? 0 : 1);
    }
}

void pins_charge_fast(const pin_ops_t *ops, bool on)
{
    if (on) {
        ops->level(ops->ctx, PIN_CHG_FAST, 0); // before the driver is on, so it is never high
        ops->mode(ops->ctx, PIN_CHG_FAST, PIN_OPEN_DRAIN);
    } else {
        ops->mode(ops->ctx, PIN_CHG_FAST, PIN_INPUT);
    }
}

void pins_mic(const pin_ops_t *ops, bool on)
{
    if (on) {
        ops->level(ops->ctx, PIN_MIC_PWR, 1);
        ops->i2s(ops->ctx, true);
        ops->delay_ms(ops->ctx, PINS_MIC_WAKE_MS);
    } else {
        // no clock into an unpowered mic: it would be fed through its input pins
        ops->i2s(ops->ctx, false);
        ops->level(ops->ctx, PIN_I2S_SCK, 0);
        ops->mode(ops->ctx, PIN_I2S_SCK, PIN_OUTPUT);
        ops->level(ops->ctx, PIN_I2S_WS, 0);
        ops->mode(ops->ctx, PIN_I2S_WS, PIN_OUTPUT);
        ops->level(ops->ctx, PIN_MIC_PWR, 0);
    }
}

void pins_sleep(const pin_ops_t *ops)
{
    pins_charge_fast(ops, false);
    pins_mic(ops, false);
    pins_led(ops, 0);
    pins_usb(ops, false);
    static const int held[] = {PIN_LED_R, PIN_LED_G, PIN_LED_B, PIN_MIC_PWR, PIN_I2S_SCK, PIN_I2S_WS};
    for (unsigned i = 0; i < sizeof held / sizeof *held; i++) {
        ops->hold(ops->ctx, held[i], true);
    }
}
