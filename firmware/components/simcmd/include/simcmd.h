// SIM console commands (D-025): what a simulator can't drive (QEMU has no GPIO), set over the
// console by a scenario (board.toml [[sim.scenario]] `sim = "PRESS 300"`, crates/pcbgen/src/
// scenario.rs). Only in the simulator builds: a REAL build compiles none of it (CMakeLists.txt),
// so wrap every use in `#if !BOARD_IS_REAL`.
//
//   SIM BUTTON 0|1     the button up (0) or down (1), until the next BUTTON or PRESS
//   SIM PRESS <ms>     down now, up <ms> later (1..60000) by the firmware's own clock, so a
//                      press is as long as it says however late the next console line arrives
//   SIM <VERB> [arg]   a board's own verb (simcmd_register); VERB is [A-Z][A-Z0-9_]*, arg one word
// Replies, one line each:
//   SIM OK <VERB>[ <arg>] @<uptime ms>
//   SIM ERR <the line after "SIM ">     (unknown verb, bad argument, a board verb said no)
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define SIMCMD_MAX_VERBS 8
#define SIMCMD_MAX_VERB 12 // characters
#define SIMCMD_MAX_ARG 31  // characters
#define SIMCMD_MAX_PRESS_MS 60000

typedef int64_t (*simcmd_clock_fn)(void);               // milliseconds since boot
typedef void (*simcmd_button_fn)(bool down, void *ctx); // the board's pin-state hook
typedef bool (*simcmd_verb_fn)(const char *arg, void *ctx); // arg "" if none; false: SIM ERR

// Resets the state. `now_ms` is the clock; `on_button` (may be NULL) hears every change of the
// simulated button, the end of a PRESS included.
void simcmd_init(simcmd_clock_fn now_ms, simcmd_button_fn on_button, void *ctx);

// The chip's clock (esp_timer); only on the ESP32 (simcmd_esp.c).
void simcmd_start(simcmd_button_fn on_button, void *ctx);

// Adds a board verb. False if the table is full, the verb isn't [A-Z][A-Z0-9_]* or is taken
// (BUTTON and PRESS are).
bool simcmd_register(const char *verb, simcmd_verb_fn fn, void *ctx);

// Answers one console line into `reply` (no newline). False if it isn't a SIM line (it doesn't
// start with "SIM "), and then `reply` is untouched.
bool simcmd_handle(const char *line, char *reply, size_t len);

// The same, printing the reply on stdout: a provision_serve_with() helper.
bool simcmd_serve(const char *line);

// The simulated button now. A PRESS ends here (or in simcmd_handle), by the clock: poll it.
bool simcmd_button(void);
