// Commands against NVS (no console, no timers), so it also builds for ESP-IDF's linux target,
// where scripts/fw-test.sh runs it against a real NVS.
#include "provision.h"

#include <stdio.h>
#include <string.h>
#include "nvs.h"
#include "provision_text.h"

#define NS "prov"

static void say(provision_emit_fn emit, void *ctx, const char *fmt, const char *key, unsigned n)
{
    char out[64];
    snprintf(out, sizeof out, fmt, key, n);
    emit(out, ctx);
}

static void list(nvs_handle_t h, provision_emit_fn emit, void *ctx)
{
    nvs_iterator_t it = NULL;
    esp_err_t err = nvs_entry_find_in_handle(h, NVS_TYPE_BLOB, &it);
    while (err == ESP_OK) {
        nvs_entry_info_t info;
        nvs_entry_info(it, &info);
        size_t len = 0;
        nvs_get_blob(h, info.key, NULL, &len);
        say(emit, ctx, "PROV KEY %s %u", info.key, (unsigned)len);
        err = nvs_entry_next(&it);
    }
    nvs_release_iterator(it);
    emit("PROV END", ctx);
}

void provision_handle(const char *line, provision_emit_fn emit, void *ctx)
{
    static prov_cmd_t cmd; // holds a secret while it is written: cleared below
    prov_verb_t v = prov_parse(line, &cmd);
    nvs_handle_t h;
    if (v == PROV_NONE) {
        return; // not for us: the console may carry other traffic
    } else if (v == PROV_BAD) {
        char out[48];
        snprintf(out, sizeof out, "PROV ERR %s%s%s", cmd.key, cmd.key[0] ? " " : "", cmd.error);
        emit(out, ctx);
    } else if (nvs_open(NS, NVS_READWRITE, &h) != ESP_OK) {
        emit("PROV ERR nvs", ctx);
    } else {
        if (v == PROV_LIST) {
            list(h, emit, ctx);
        } else if (v == PROV_SET) {
            if (nvs_set_blob(h, cmd.key, cmd.value, cmd.len) == ESP_OK && nvs_commit(h) == ESP_OK) {
                say(emit, ctx, "PROV OK %s %u", cmd.key, (unsigned)cmd.len);
            } else {
                say(emit, ctx, "PROV ERR %s write", cmd.key, 0);
            }
        } else {
            esp_err_t err = nvs_erase_key(h, cmd.key);
            if ((err == ESP_OK || err == ESP_ERR_NVS_NOT_FOUND) && nvs_commit(h) == ESP_OK) {
                say(emit, ctx, "PROV OK %s %u", cmd.key, 0);
            } else {
                say(emit, ctx, "PROV ERR %s erase", cmd.key, 0);
            }
        }
        nvs_close(h);
    }
    memset(&cmd, 0, sizeof cmd);
}

esp_err_t provision_get(const char *key, char *buf, size_t len)
{
    if (len == 0) {
        return ESP_ERR_INVALID_ARG;
    }
    nvs_handle_t h;
    esp_err_t err = nvs_open(NS, NVS_READONLY, &h);
    if (err != ESP_OK) {
        return err;
    }
    size_t n = len - 1; // room for the NUL; a longer value is ESP_ERR_NVS_INVALID_LENGTH
    err = nvs_get_blob(h, key, buf, &n);
    nvs_close(h);
    buf[err == ESP_OK ? n : 0] = 0;
    return err;
}
