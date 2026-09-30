// QEMU has no GPIO or ADC: the pins are variables here, set by `SIM` console lines from the
// scenario script (sim/run.py) and shown as `PIN` lines, so a scenario can check them.
//   SIM BUTTON 0|1    SIM USB 0|1    SIM CHRG 0|1    SIM BATTERY <mV at rest>
//   SIM LOADED <mV with Wi-Fi up, 0 = as at rest>    SIM NET 0|1 (1: the network can be reached)
//   SIM SKIP <ms> (the clocks jump, as in a sleep)   SIM TIMER (asleep: the timer wake is now)
#include "hw.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "board_pins.h"
#include "clip.h"

void sleep_fake_timer(void); // sleep_fake.c

static struct {
    bool button, usb, charging, net_down;
    int battery_mv, loaded_mv;
    // what the firmware drives
    uint8_t led;
    bool fast, mic, stdby_pull;
    bool held;
} pins = {.battery_mv = 3900};

// The same sequences as on the board, against the variables: what they end in is shown.
static void op_mode(void *ctx, int pin, pin_mode_t mode)
{
    (void)ctx;
    if (pin == PIN_CHG_FAST) {
        pins.fast = mode == PIN_OPEN_DRAIN;
    } else if (pin == PIN_STDBY) {
        pins.stdby_pull = mode == PIN_INPUT_PULLUP;
    }
}

static void op_level(void *ctx, int pin, int level)
{
    (void)ctx;
    uint8_t colour = pin == PIN_LED_R ? CLIP_RED : pin == PIN_LED_G ? CLIP_GREEN : pin == PIN_LED_B ? CLIP_BLUE : 0;
    if (colour) {
        pins.led = level ? pins.led & (uint8_t)~colour : pins.led | colour;
    } else if (pin == PIN_MIC_PWR) {
        pins.mic = level;
    }
}

static void op_hold(void *ctx, int pin, bool on)
{
    (void)ctx;
    if (pin == PIN_MIC_PWR) {
        pins.held = on;
    }
}

static void op_i2s(void *ctx, bool run)
{
    (void)ctx;
    (void)run;
}

static void op_delay(void *ctx, int ms)
{
    (void)ctx;
    (void)ms;
}

static const pin_ops_t ops = {op_mode, op_level, op_hold, op_i2s, op_delay, NULL};

const pin_ops_t *hw_pin_ops(void)
{
    return &ops;
}

static void show(void)
{
    printf("PIN {\"led\":%d,\"charge_fast\":%s,\"mic\":%s,\"stdby_pull\":%s,\"held\":%s}\n", pins.led,
           pins.fast ? "true" : "false", pins.mic ? "true" : "false", pins.stdby_pull ? "true" : "false",
           pins.held ? "true" : "false");
}

void hw_init(void)
{
    pins_awake(&ops);
    show();
}

bool hw_button(void)
{
    return pins.button;
}

bool hw_usb(void)
{
    return pins.usb;
}

bool hw_charging(void)
{
    return pins.usb && pins.charging;
}

bool hw_charged(void)
{
    return pins.usb && !pins.charging;
}

int hw_battery_mv(void)
{
    return pins.battery_mv;
}

void hw_usb_present(bool in)
{
    pins_usb(&ops, in);
    show();
}

void hw_led(uint8_t colours)
{
    pins_led(&ops, colours);
}

void hw_charge_fast(bool on)
{
    pins_charge_fast(&ops, on);
    show();
}

void hw_sim_lock_net(bool *net_down)
{
    *net_down = pins.net_down;
}

int hw_sim_loaded_mv(void)
{
    return pins.loaded_mv;
}

bool hw_sim_command(const char *line)
{
    char what[12] = "";
    long value = 0;
    if (strncmp(line, "SIM ", 4) != 0) {
        return false;
    }
    int got = sscanf(line + 4, "%11s %ld", what, &value);
    if (got == 2 && strcmp(what, "BUTTON") == 0) {
        pins.button = value;
    } else if (got == 2 && strcmp(what, "USB") == 0) {
        pins.usb = value;
    } else if (got == 2 && strcmp(what, "CHRG") == 0) {
        pins.charging = value;
    } else if (got == 2 && strcmp(what, "BATTERY") == 0) {
        pins.battery_mv = (int)value;
    } else if (got == 2 && strcmp(what, "LOADED") == 0) {
        pins.loaded_mv = (int)value;
    } else if (got == 2 && strcmp(what, "NET") == 0) {
        pins.net_down = !value;
    } else if (got == 2 && strcmp(what, "SKIP") == 0) {
        hw_sim_skip(value);
    } else if (got == 1 && strcmp(what, "TIMER") == 0) {
        sleep_fake_timer();
    } else {
        printf("SIM ERR %s\n", line + 4);
        return true;
    }
    printf("SIM OK %s @%lld\n", line + 4, (long long)hw_uptime_ms());
    return true;
}
