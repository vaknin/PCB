// What every pin is set to, and in which order, in each of the board's phases (board.toml's pin
// notes are the rules). Pure C over a small set of pin operations, so the laptop test
// (test/test_pins_seq.c) checks the rules against a made-up chip: QEMU has no pins.
#pragma once

#include <stdbool.h>
#include <stdint.h>

typedef enum {
    PIN_INPUT,        // no pulls
    PIN_INPUT_PULLUP, // the chip's own pull-up (about 45 kΩ)
    PIN_OUTPUT,       // push-pull
    PIN_OPEN_DRAIN,   // drives low or lets go, never high
} pin_mode_t;

typedef struct {
    void (*mode)(void *ctx, int pin, pin_mode_t mode);
    void (*level)(void *ctx, int pin, int level); // what an output drives
    void (*hold)(void *ctx, int pin, bool on);    // keeps the pin as it is through deep sleep and the wake
    void (*i2s)(void *ctx, bool run);             // the I2S clocks on SCK and WS; stopped, the pins are free
    void (*delay_ms)(void *ctx, int ms);
    void *ctx;
} pin_ops_t;

#define PINS_MIC_WAKE_MS 25 // ICS-43434: data is valid at most 20 ms after power and clocks

// After a reset or a deep-sleep wake: inputs without pulls, the light dark, the mic off, the
// charger at its slow rate, and the holds from the sleep let go.
void pins_awake(const pin_ops_t *ops);
// USB came or went: the STDBY pull-up is on only while USB is in.
void pins_usb(const pin_ops_t *ops, bool present);
// colours: CLIP_RED | CLIP_GREEN | CLIP_BLUE.
void pins_led(const pin_ops_t *ops, uint8_t colours);
// on: about 333 mA. Only ever called with Wi-Fi off (the state machine's rule).
void pins_charge_fast(const pin_ops_t *ops, bool on);
// on: power, a wait, then the clocks. off: the clocks stop and are grounded before the power goes.
void pins_mic(const pin_ops_t *ops, bool on);
// Just before deep sleep: everything off and held.
void pins_sleep(const pin_ops_t *ops);
