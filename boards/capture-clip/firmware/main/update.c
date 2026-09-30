// Firmware over Wi-Fi (D-025, "Right the first time"): the manifest at the provisioned
// `update_url` names a version, an image, its size and SHA-256. The image goes into the other
// slot and is checked (size, SHA-256, and ESP-IDF's own image check) before the slots are
// switched. The new firmware then runs on trial: only when update_trial() passes is it kept;
// a crash, a reset or a failed check brings the old one back (the bootloader's rollback).
// What to install is decided in clip_update.c, which the laptop tests cover.
#include "app.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "board.h"
#include "esp_app_desc.h"
#include "esp_ota_ops.h"
#include "esp_system.h"
#include "hw.h"
#include "provision.h"
#include "psa/crypto.h"
#include "sdkconfig.h"

#define MANIFEST_MAX 2048
#define GITHUB_API "https://api.github.com/"

static const esp_partition_t *ready; // written and verified, waiting for update_apply()

bool update_on_trial(void)
{
    esp_ota_img_states_t state;
    return esp_ota_get_state_partition(esp_ota_get_running_partition(), &state) == ESP_OK &&
           state == ESP_OTA_IMG_PENDING_VERIFY;
}

const char *update_rolled_back(void)
{
    static char version[32];
    esp_app_desc_t app;
    const esp_partition_t *bad = esp_ota_get_last_invalid_partition();
    if (bad && esp_ota_get_partition_description(bad, &app) == ESP_OK) {
        snprintf(version, sizeof version, "%s", app.version);
    }
    return version;
}

static bool manifest_url(char *url, size_t len)
{
    return provision_get("update_url", url, len) == ESP_OK && url[0];
}

static bool fetch_manifest(clip_manifest_t *manifest, char *detail, size_t len)
{
    char url[256], *text = NULL;
    int status = 0;
    if (!manifest_url(url, sizeof url)) {
        snprintf(detail, len, "no update_url");
        return false;
    }
    bool github = strncmp(url, GITHUB_API, strlen(GITHUB_API)) == 0;
    if (!http_get(url, github, MANIFEST_MAX, &status, &text, NULL) || status != 200) {
        snprintf(detail, len, "manifest: HTTP %d", status);
        free(text);
        return false;
    }
    bool ok = clip_manifest_parse(text, manifest);
    free(text);
    if (!ok) {
        snprintf(detail, len, "manifest: not readable");
    }
    return ok;
}

typedef struct {
    esp_ota_handle_t ota;
    psa_hash_operation_t hash;
    long bytes, limit;
    bool failed;
} download_t;

static bool on_block(void *ctx, const uint8_t *data, size_t len)
{
    download_t *d = ctx;
    d->bytes += (long)len;
    if (d->bytes > d->limit || psa_hash_update(&d->hash, data, len) != PSA_SUCCESS ||
        esp_ota_write(d->ota, data, len) != ESP_OK) {
        d->failed = true;
    }
    return !d->failed;
}

update_result_t update_check(char *detail, size_t len)
{
    clip_manifest_t manifest;
    char url[256];
    detail[0] = 0;
    if (!manifest_url(url, sizeof url)) {
        snprintf(detail, len, "no update_url");
        return UPDATE_NONE;
    }
    if (!fetch_manifest(&manifest, detail, len)) {
        return UPDATE_FAILED;
    }
    const esp_partition_t *slot = esp_ota_get_next_update_partition(NULL);
    const char *running = esp_app_get_description()->version;
    clip_update_t what = clip_update_decide(&manifest, running, update_rolled_back(), slot ? (long)slot->size : 0);
    if (what == CLIP_UPDATE_NONE) {
        snprintf(detail, len, "%s is current", running);
        return UPDATE_NONE;
    }
    if (what == CLIP_UPDATE_KNOWN_BAD) {
        snprintf(detail, len, "%s was rolled back before: not again", manifest.version);
        return UPDATE_NONE;
    }
    if (what == CLIP_UPDATE_TOO_BIG || !slot) {
        snprintf(detail, len, "%s does not fit the slot (%ld bytes)", manifest.version, manifest.size);
        return UPDATE_FAILED;
    }
    download_t d = {.limit = manifest.size, .hash = PSA_HASH_OPERATION_INIT};
    uint8_t sha[32];
    size_t sha_len = 0;
    int status = 0;
    if (psa_crypto_init() != PSA_SUCCESS || psa_hash_setup(&d.hash, PSA_ALG_SHA_256) != PSA_SUCCESS) {
        snprintf(detail, len, "%s: no SHA-256", manifest.version);
        return UPDATE_FAILED;
    }
    if (esp_ota_begin(slot, (size_t)manifest.size, &d.ota) != ESP_OK) {
        psa_hash_abort(&d.hash);
        snprintf(detail, len, "%s: the slot could not be erased", manifest.version);
        return UPDATE_FAILED;
    }
    bool github = strncmp(manifest.url, GITHUB_API, strlen(GITHUB_API)) == 0;
    bool got = http_download(manifest.url, github, &status, on_block, &d) && status == 200 && !d.failed;
    bool hashed = psa_hash_finish(&d.hash, sha, sizeof sha, &sha_len) == PSA_SUCCESS && sha_len == 32;
    if (!got || d.bytes != manifest.size) {
        esp_ota_abort(d.ota);
        snprintf(detail, len, "%s: download failed (HTTP %d, %ld of %ld bytes)", manifest.version, status, d.bytes,
                 manifest.size);
        return UPDATE_FAILED;
    }
    if (!hashed || memcmp(sha, manifest.sha256, 32) != 0) {
        esp_ota_abort(d.ota);
        snprintf(detail, len, "%s: the SHA-256 does not match", manifest.version);
        return UPDATE_FAILED;
    }
    if (esp_ota_end(d.ota) != ESP_OK) { // ESP-IDF's own check of the image
        snprintf(detail, len, "%s: not a valid image", manifest.version);
        return UPDATE_FAILED;
    }
    ready = slot;
    snprintf(detail, len, "%s", manifest.version);
    return UPDATE_READY;
}

void update_apply(void)
{
    if (ready && esp_ota_set_boot_partition(ready) == ESP_OK) {
        say("update", "\"step\":\"restart\",\"slot\":\"%s\"", ready->label);
        // a restart, not a deep sleep: the bootloader must validate the new image
        // (CONFIG_BOOTLOADER_SKIP_VALIDATE_IN_DEEP_SLEEP must stay off)
        fflush(stdout);
        esp_restart();
    }
    ready = NULL;
}

void update_trial(bool parts_ok)
{
    char why[96] = "";
    clip_manifest_t manifest;
    bool ok = true;
#if CONFIG_CLIP_SIM_FAIL_TRIAL
    snprintf(why, sizeof why, "built to fail its check (CLIP_SIM_FAIL_TRIAL)");
    ok = false;
#endif
    // What an update must never break: the queue's storage, the keys, the recorder, and the way
    // to the next update. The network is tried a few times: a router that is rebooting must not
    // cost a good firmware.
    if (ok && store_free_bytes() < 0) {
        snprintf(why, sizeof why, "storage does not mount");
        ok = false;
    }
    if (ok && !http_load_keys(why, sizeof why)) {
        ok = false;
    }
    if (ok && !parts_ok) {
        snprintf(why, sizeof why, "storage or the recorder does not start");
        ok = false;
    }
    if (ok) {
        ok = false;
        for (int attempt = 0; attempt < 3 && !ok; attempt++) {
            ok = net_up(why, sizeof why) && fetch_manifest(&manifest, why, sizeof why);
            net_down();
        }
    }
    if (ok) {
        esp_ota_mark_app_valid_cancel_rollback();
        say("update", "\"step\":\"kept\",\"version\":\"%s\"", esp_app_get_description()->version);
        return;
    }
    say("update", "\"step\":\"rollback\",\"version\":\"%s\",\"why\":\"%s\"", esp_app_get_description()->version, why);
    fflush(stdout);
    esp_ota_mark_app_invalid_rollback_and_reboot();
    esp_restart(); // only reached if there is nothing to go back to
}
