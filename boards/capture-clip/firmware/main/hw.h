// The board as the app sees it. Each part has two implementations, picked by the build target
// (CMakeLists.txt), so the app's logic is the same code on the board and in the simulators:
//   pins (button, light, charger, battery)  hw_gpio.c  real, Wokwi   | hw_stub.c    QEMU (SIM commands)
//   microphone                              mic_i2s.c  real          | mic_synth.c  QEMU, Wokwi
//   sleep                                   sleep_deep.c real        | sleep_fake.c QEMU, Wokwi
//   network                                 net_wifi.c real, Wokwi   | net_eth.c    QEMU
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "pins_seq.h"

#define HW_SAMPLE_RATE 16000

// ---- pins ----
void hw_init(void);           // every pin to its awake state (pins_awake)
bool hw_button(void);         // down
bool hw_usb(void);            // VBUS present
bool hw_charging(void);       // the charger's CHRG; only meaningful while USB is in
bool hw_charged(void);        // the charger's STDBY; only while USB is in
int hw_battery_mv(void);      // the cell now, averaged
void hw_usb_present(bool in); // the STDBY pull-up follows USB
void hw_led(uint8_t colours);
void hw_charge_fast(bool on);
const pin_ops_t *hw_pin_ops(void);
// QEMU only: a `SIM ...` console line. False when it is not one.
bool hw_sim_command(const char *line);
void hw_sim_lock_net(bool *net_down); // QEMU: SIM NET 0 is on
int hw_sim_loaded_mv(void);           // QEMU: the cell under Wi-Fi load; 0 = as at rest

// ---- microphone: 16 kHz, 16-bit, mono ----
bool hw_mic_start(void);
int hw_mic_read(int16_t *pcm, int samples); // blocks for them; < samples on an error
void hw_mic_stop(void);

// ---- time and sleep ----
typedef enum {
    HW_WAKE_RESET, // power-on or any reset: not out of deep sleep
    HW_WAKE_BUTTON,
    HW_WAKE_USB,
    HW_WAKE_TIMER,
} hw_wake_t;

// Milliseconds since power-on, counting through deep sleep.
int64_t hw_uptime_ms(void);
// Why app_main runs.
hw_wake_t hw_wake(void);
// Deep sleep until the button, USB or (wake_after_ms > 0) the timer. The pins must already be
// in their sleep state. On the board this does not return: the wake is a restart. In the
// simulators it returns at the wake.
hw_wake_t hw_sleep(int64_t wake_after_ms);
// Simulators: moves the clocks on, as a sleep would.
void hw_sim_skip(int64_t ms);

// ---- network ----
// Connects with the provisioned credentials and sets the clock. `why` gets a short reason on failure.
bool net_up(char *why, size_t len);
void net_down(void);
bool net_is_up(void);
// Called (from the network's own task) when a connection that was up is lost.
void net_on_lost(void (*lost)(void));
