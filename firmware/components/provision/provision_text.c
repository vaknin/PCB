#include "provision_text.h"

#include <string.h>

// the next space-separated word of *s: its start, with its length in *n (0 at the end)
static const char *word(const char **s, size_t *n)
{
    const char *p = *s;
    while (*p == ' ') {
        p++;
    }
    const char *start = p;
    while (*p && *p != ' ' && *p != '\r' && *p != '\n') {
        p++;
    }
    *n = (size_t)(p - start);
    *s = p;
    return start;
}

static int nibble(char c)
{
    if (c >= '0' && c <= '9') {
        return c - '0';
    }
    if (c >= 'a' && c <= 'f') {
        return c - 'a' + 10;
    }
    if (c >= 'A' && c <= 'F') {
        return c - 'A' + 10;
    }
    return -1;
}

int prov_key_ok(const char *key, size_t len)
{
    if (len == 0 || len > PROV_MAX_KEY) {
        return 0;
    }
    for (size_t i = 0; i < len; i++) {
        char c = key[i];
        if (!((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '_')) {
            return 0;
        }
    }
    return 1;
}

static prov_verb_t bad(prov_cmd_t *cmd, const char *why)
{
    cmd->verb = PROV_BAD;
    cmd->error = why;
    return PROV_BAD;
}

prov_verb_t prov_parse(const char *line, prov_cmd_t *cmd)
{
    memset(cmd, 0, sizeof *cmd);
    size_t n;
    const char *w = word(&line, &n);
    if (n != 4 || memcmp(w, "PROV", 4) != 0) {
        return cmd->verb = PROV_NONE;
    }
    const char *verb = word(&line, &n);
    prov_verb_t v = n == 3 && memcmp(verb, "SET", 3) == 0   ? PROV_SET
                    : n == 3 && memcmp(verb, "DEL", 3) == 0 ? PROV_DEL
                    : n == 4 && memcmp(verb, "LIST", 4) == 0 ? PROV_LIST
                                                             : PROV_BAD;
    if (v == PROV_BAD) {
        return bad(cmd, "verb");
    }
    if (v != PROV_LIST) {
        const char *key = word(&line, &n);
        if (!prov_key_ok(key, n)) {
            return bad(cmd, "key");
        }
        memcpy(cmd->key, key, n);
    }
    if (v == PROV_SET) {
        const char *hex = word(&line, &n);
        if (n == 0 || n % 2 || n / 2 > PROV_MAX_VALUE) {
            return bad(cmd, "value");
        }
        for (size_t i = 0; i < n / 2; i++) {
            int hi = nibble(hex[2 * i]), lo = nibble(hex[2 * i + 1]);
            if (hi < 0 || lo < 0) {
                memset(cmd->value, 0, sizeof cmd->value);
                return bad(cmd, "value");
            }
            cmd->value[i] = (uint8_t)(hi << 4 | lo);
        }
        cmd->len = n / 2;
    }
    word(&line, &n);
    if (n != 0) {
        memset(cmd->value, 0, sizeof cmd->value);
        cmd->len = 0;
        return bad(cmd, "line");
    }
    return cmd->verb = v;
}
