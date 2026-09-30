// The app's own parts (main.c ties them to the state machine in firmware/components/clip).
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "capture.h"
#include "clip.h"

// One line the scenario script and a person at the console can both read: CLIP {"ev":"...", ...}.
void say(const char *event, const char *fmt, ...) __attribute__((format(printf, 2, 3)));

// ---- store.c: the recordings' file system (LittleFS on the `storage` partition) ----
#define STORE_DIR "/q"
// Below this much free space a recording stops (and none starts): what is left is for the
// small files an upload writes.
#define STORE_RESERVE_BYTES (256 * 1024)
bool store_mount(void);
const clip_store_t *store_get(void);
int64_t store_free_bytes(void);
void store_path(const char *name, char *out, size_t len);

// ---- recorder.c: microphone -> Opus -> Ogg -> <id>.ogg, a page about every second ----
bool recorder_init(void);
// Starts writing <id>.ogg. `failed` is called from the recorder's task if it stops by itself.
bool recorder_start(const char *id, void (*failed)(void));
// Stops and closes the file. The audio's length; -1 when nothing was being recorded.
int64_t recorder_stop(void);
bool recorder_busy(void);
unsigned recorder_stack_free(void); // the least ever free, bytes

// ---- http.c ----
// The provisioned secrets are read once per wake and wiped by http_forget().
bool http_load_keys(char *why, size_t len);
void http_forget(void);
const char *http_repo(void);
const char *http_branch(void);
// cap_http_fn for api.github.com.
bool http_github(void *ctx, const char *method, const char *url, const char *body, cap_http_response_t *response);
// clip_gemini_fn: the request streamed from flash.
bool http_gemini(void *ctx, const char *prefix, const clip_audio_t *audio, const char *suffix, int *status, char **body);
// A plain GET into memory (at most `max` bytes). With `github_auth` the notes-repo token goes along.
bool http_get(const char *url, bool github_auth, size_t max, int *status, char **body, size_t *len);
// A GET handed out in blocks. False on a network error or when `block` returns false.
bool http_download(const char *url, bool github_auth, int *status, bool (*block)(void *ctx, const uint8_t *data, size_t len),
                   void *ctx);
// QEMU: where the mock server is ("http://10.0.2.2:<port>", the provisioned `sim_base`); NULL otherwise.
const char *http_sim_base(void);
void http_close(void); // drops the kept-open GitHub connection

// ---- update.c: firmware over Wi-Fi, two slots, rollback ----
// True when this boot is a new firmware on trial: update_trial() must pass or it is rolled back.
bool update_on_trial(void);
// The new firmware checks itself (storage, keys, recorder, network, the manifest) and is kept
// only when all of it works. parts_ok: storage mounted and the recorder started. Does not return
// on failure: the old firmware boots again.
void update_trial(bool parts_ok);
// The version the bootloader last rolled back from, "" when none.
const char *update_rolled_back(void);
typedef enum {
    UPDATE_NONE,   // nothing newer, or no update_url provisioned
    UPDATE_FAILED, // could not be read, fetched or verified: nothing changed
    UPDATE_READY,  // written and verified: update_apply() switches to it
} update_result_t;
update_result_t update_check(char *detail, size_t len);
void update_apply(void); // restarts into the new firmware

// ---- selftests.c ----
void selftests_run(void);
