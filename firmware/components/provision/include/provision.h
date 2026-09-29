// Provisioning over the USB console (D-025): the laptop's `devctl provision` sends named
// values (Wi-Fi, API keys, repo) that land in NVS namespace "prov". Values are never echoed
// or printed; replies carry only the key name and the length. One command per line:
//   PROV SET <key> <hex>   ->  PROV OK <key> <bytes>      (key: [a-z0-9_]{1,15})
//   PROV DEL <key>         ->  PROV OK <key> 0
//   PROV LIST              ->  PROV KEY <key> <bytes> ... PROV END
//   anything else          ->  PROV ERR <reason>
// Whether NVS itself is encrypted is the board's sdkconfig choice (HMAC scheme, D-025).
#pragma once

#include <stddef.h>
#include <stdint.h>
#include "esp_err.h"

// Makes the console readable line by line (USB Serial/JTAG on a real board, UART0 in QEMU).
// Call once, after board_start().
esp_err_t provision_console_init(void);

// Reads and answers commands for up to `ms` milliseconds after the last one (0: forever).
void provision_serve(uint32_t ms);

// Copies a stored value into buf (NUL-terminated if it fits). ESP_ERR_NVS_NOT_FOUND if unset.
esp_err_t provision_get(const char *key, char *buf, size_t len);
