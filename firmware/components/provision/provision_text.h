// The provisioning line format (pure C, no ESP-IDF calls, so scripts/fw-test.sh tests it on
// the laptop). Protocol: provision.h.
#pragma once

#include <stddef.h>
#include <stdint.h>

#define PROV_MAX_KEY 15    // NVS key names are at most 15 characters
#define PROV_MAX_VALUE 512 // bytes
// longest valid line: "PROV SET " + key + " " + 2 hex digits per byte
#define PROV_MAX_LINE (10 + PROV_MAX_KEY + 2 * PROV_MAX_VALUE)

typedef enum {
    PROV_NONE, // not a PROV line: other console traffic, ignored
    PROV_SET,
    PROV_DEL,
    PROV_LIST,
    PROV_BAD, // a PROV line that is wrong; `error` says why
} prov_verb_t;

typedef struct {
    prov_verb_t verb;
    char key[PROV_MAX_KEY + 1];
    uint8_t value[PROV_MAX_VALUE];
    size_t len;
    // PROV_BAD: "verb", "key", "value" (with `key` set) or "line" (extra words)
    const char *error;
} prov_cmd_t;

// Parses one line (a trailing CR or LF is ignored); words are separated by spaces.
prov_verb_t prov_parse(const char *line, prov_cmd_t *cmd);

// [a-z0-9_]{1,15}
int prov_key_ok(const char *key, size_t len);
