// Provisioning over the USB console (D-025): the laptop's `devctl provision` sends named
// values (Wi-Fi, API keys, repo) that land in NVS namespace "prov". Values are never echoed
// or printed; replies carry only the key name and the length. One command per line:
//   PROV SET <key> <hex>   ->  PROV OK <key> <bytes>      (key: [a-z0-9_]{1,15})
//   PROV DEL <key>         ->  PROV OK <key> 0
//   PROV LIST              ->  PROV KEY <key> <bytes> ... PROV END
//   a wrong PROV line      ->  PROV ERR [<key> ]<verb|key|value|line|nvs|write|erase>
//   (lines not starting with PROV are ignored; a line over PROV_MAX_LINE is "line")
// Whether NVS itself is encrypted is the board's sdkconfig choice (HMAC scheme, D-025).
#pragma once

#include <stddef.h>
#include <stdint.h>
#include "esp_err.h"

// Makes the console readable line by line (USB Serial/JTAG on a real board, UART0 in QEMU).
// Call once, after board_start().
esp_err_t provision_console_init(void);

// Answers one command line; each reply line (without newline) goes to emit. Needs only NVS,
// so the laptop tests (scripts/fw-test.sh) run it on ESP-IDF's linux target.
typedef void (*provision_emit_fn)(const char *reply, void *ctx);
void provision_handle(const char *line, provision_emit_fn emit, void *ctx);

// Reads and answers commands for up to `ms` milliseconds after the last one (0: forever).
void provision_serve(uint32_t ms);

// The same, for an app with console commands of its own: every other line (not empty, not
// starting with PROV, within the line limit) is handed to `other`.
typedef void (*provision_other_fn)(const char *line, void *ctx);
void provision_serve_with(uint32_t ms, provision_other_fn other, void *ctx);

// Copies a stored value into buf, NUL-terminated. ESP_ERR_NVS_NOT_FOUND if unset,
// ESP_ERR_NVS_INVALID_LENGTH if it doesn't fit in len - 1 bytes.
esp_err_t provision_get(const char *key, char *buf, size_t len);
