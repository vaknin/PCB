// The capture-clip app's logic (D-025 Phase D.3) as pure C with no ESP-IDF calls, so the laptop
// tests cover it (firmware/test/test_clip_*.c): the button/recording/upload state machine, the
// light's patterns, the battery rules, the queue of recordings on flash and the upload of one
// recording through the capture component. The board's main/ supplies the hardware: a recorder,
// a file store (LittleFS), HTTPS for GitHub and Gemini, the LED, the button, the ADC and sleep.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "capture.h"

#define CLIP_ID_LEN 33 // 32 lowercase hex digits (cap_new_id)

typedef enum {
    CLIP_NEW,      // a press: a new note
    CLIP_ADDITION, // a hold: added to the last note made here
} clip_kind_t;

// ---- battery (research/2026-09-29-esp32-firmware.md §7) -------------------------------------------

#define CLIP_BATTERY_DIVIDER 3 // 2 x 1 MΩ over 1 MΩ

// The limits follow from the HE9073 regulator (research/2026-09-30-power-path-fix.md, "Dropout:
// the price on battery"): it drops about 0.46 V at the module's 355 mA Wi-Fi peak (datasheet
// Fig. 5, typical, no maximum given) and Q1 0.03 V, and the module needs 3.0 V. INFERRED until
// measured at bring-up: a change here is a firmware update, not a board change.
//
// Wi-Fi needs 3.55 V at the cell while it transmits. The cell sags under that load, so the limit
// for a reading at rest is higher: 0.1 V for 355 mA through a small cell and its protection
// circuit (about 0.3 Ω, INFERRED; the cell's datasheet was not read).
#define CLIP_UPLOAD_LOADED_MIN_MV 3550 // read with Wi-Fi up: below, Wi-Fi goes off again
#define CLIP_UPLOAD_SAG_MV 100
#define CLIP_UPLOAD_MIN_MV (CLIP_UPLOAD_LOADED_MIN_MV + CLIP_UPLOAD_SAG_MV) // at rest: below, no Wi-Fi
// Recording draws under 100 mA (about 0.1 V of dropout): the same research file puts its limit at 3.4 V.
#define CLIP_RECORD_MIN_MV 3400       // at rest: below, no recording
#define CLIP_BATTERY_HYSTERESIS_MV 50 // a level is left upwards only this far above its limit

typedef enum {
    CLIP_BATTERY_OK,
    CLIP_BATTERY_LOW,   // records, does not upload
    CLIP_BATTERY_EMPTY, // does neither
} clip_battery_t;

// The cell's voltage from the calibrated millivolts at the ADC pin.
int clip_battery_cell_mv(int adc_mv);

// The level for a reading taken at rest (Wi-Fi off), given the level before it: the ADC is good
// to about ±30 mV at the cell, so a reading at a limit must not flip the level back and forth.
clip_battery_t clip_battery_level(clip_battery_t before, int cell_mv);

// How full the cell is, 0-100, from its voltage at rest: a typical LiPo curve between 4.2 V and
// the voltage where the clip stops recording. A rough figure for the status file, not a gauge.
int clip_battery_percent(int cell_mv);

// A reading taken while Wi-Fi is up, as the level event to give the state machine: the reading
// itself when the cell holds up, and one just under the upload limit when it sags too far, so
// the state machine turns Wi-Fi off and keeps the queue (it records on).
int clip_battery_loaded_mv(int loaded_mv);

// ---- the light (spec.md, Inputs and outputs) ------------------------------------------------------

enum {
    CLIP_RED = 1,
    CLIP_GREEN = 2,
    CLIP_BLUE = 4,
    CLIP_AMBER = CLIP_RED | CLIP_GREEN,
};

typedef enum {
    CLIP_LED_OFF,
    CLIP_LED_RECORDING,   // red
    CLIP_LED_ADDING,      // red after one short wink: the hold was seen, the button can be let go
    CLIP_LED_SENDING,     // blue
    CLIP_LED_SAVED,       // a green flash
    CLIP_LED_WAITING,     // amber: a note is waiting for Wi-Fi
    CLIP_LED_ERROR,       // red, blinking fast
    CLIP_LED_LOW_BATTERY, // red, three short blinks
    CLIP_LED_COUNT,
} clip_led_t;

typedef struct {
    uint8_t colour;
    uint16_t lead_off_ms; // dark first
    uint16_t on_ms;       // then on and off in turn; off_ms 0 is steady
    uint16_t off_ms;
    uint16_t total_ms; // over after this long, and dark; 0 lasts until replaced
} clip_led_pattern_t;

extern const clip_led_pattern_t clip_led_patterns[CLIP_LED_COUNT];

// Which colours are lit `t_ms` after the pattern started (CLIP_RED | CLIP_GREEN | CLIP_BLUE).
uint8_t clip_led_colour(clip_led_t led, int64_t t_ms);
bool clip_led_over(clip_led_t led, int64_t t_ms);

// ---- the button ------------------------------------------------------------------------------------

// The button's pin, sampled every few ms by something that is never kept waiting (a timer, not
// the app's loop), becomes presses and releases with the time they happened. The app's loop may
// be late; a short press must still not look like a hold.
#define CLIP_DEBOUNCE_MS 30

typedef struct {
    bool down;         // the debounced level
    bool changing;     // the pin differs from it since since_ms
    int64_t since_ms;
} clip_button_t;

void clip_button_init(clip_button_t *button, bool down);
// One sample of the pin. True when the debounced level changed (it is in button->down): the pin
// has been at the new level for CLIP_DEBOUNCE_MS, and *at_ms is when it was first seen there,
// the time to give clip_sm_step with the event.
bool clip_button_sample(clip_button_t *button, bool down, int64_t now_ms, int64_t *at_ms);

// ---- the state machine ----------------------------------------------------------------------------

#define CLIP_HOLD_MS 1000
#define CLIP_RECORD_MAX_MS (15 * 60 * 1000)
// A wait this short for the next upload (the 5 s between Gemini requests, the first 30 s retry)
// is spent awake with Wi-Fi up: a deep sleep and a reconnect would cost more than it saves.
#define CLIP_LINGER_MS 30000

typedef enum {
    CLIP_IDLE,       // awake with nothing to do: only while USB is in or the button is still down
    CLIP_PRESSED,    // recording, the button still down: a press or a hold?
    CLIP_RECORDING,  // until the next press, or 15 minutes
    CLIP_CONNECTING, // Wi-Fi is coming up for the queue
    CLIP_UPLOADING,  // a recording is being sent, or the next one is due in a moment
    CLIP_SHOWING,    // a pattern that ends by itself: saved, waiting, error, low battery
    CLIP_UPDATING,   // on USB with Wi-Fi up and the queue done: the app looks for new firmware
    CLIP_ASLEEP,     // CLIP_DO_SLEEP was given; clip_sm_wake() starts the next wake
} clip_state_t;

typedef enum {
    CLIP_WIFI_OFF,
    CLIP_WIFI_CONNECTING,
    CLIP_WIFI_UP,
} clip_wifi_t;

// What one try at the queue's next recording came to (clip_upload_next).
typedef enum {
    CLIP_UP_SAVED,         // the note, or the addition, is on GitHub
    CLIP_UP_NOTHING_ADDED, // an addition with no speech in it: dropped
    CLIP_UP_LATER,         // it stays queued and is tried again after a wait
    CLIP_UP_OFFLINE,       // the network went away; nothing changed
    CLIP_UP_GAVE_UP,       // Gemini will not take it: set aside (clip_queue_retry_failed)
    CLIP_UP_STUCK,         // GitHub refuses (token, repo): it stays queued, tried again much later
} clip_upload_t;

typedef enum {
    CLIP_EV_TICK,          // time passed; send one every 50 ms or so while awake
    CLIP_EV_BUTTON_DOWN,   // debounced. After a wake by the button: at the time the wake stub saw
    CLIP_EV_BUTTON_UP,
    CLIP_EV_USB,           // .on: VBUS present
    CLIP_EV_BATTERY,       // .cell_mv, read at rest
    CLIP_EV_QUEUE,         // .queued, .wait_ms: what is on flash (after a boot's clip_queue_recover)
    CLIP_EV_WIFI,          // .on: connected with the clock set; off: could not connect, or lost
    CLIP_EV_UPLOADED,      // .upload, .queued, .wait_ms: CLIP_DO_UPLOAD finished
    CLIP_EV_RECORD_FAILED, // the recorder stopped by itself (flash full, mic); .queued, .wait_ms
    CLIP_EV_UPDATE_CHECKED, // CLIP_DO_CHECK_UPDATE finished without installing anything
} clip_event_kind_t;

typedef struct {
    clip_event_kind_t kind;
    bool on;
    int cell_mv;
    clip_upload_t upload;
    int queued;      // recordings waiting on flash
    int64_t wait_ms; // until the first of them can be tried; 0 now
} clip_event_t;

// What the app must do after a step, in this order when several are set. The charger's fast
// setting (CHG_FAST driven: about 300 mA, released: 100 mA) is on only while USB is in and Wi-Fi
// is off, so the port is never asked for both: it goes off before Wi-Fi starts and comes back
// after Wi-Fi stops. The app's own Wi-Fi use outside the state machine (the self-test) must
// release it the same way.
enum {
    CLIP_DO_RECORD_STOP = 1 << 0,     // close the recording and queue it (clip_queue_finish)
    CLIP_DO_RECORD_START = 1 << 1,    // mic on, a new recording as a new note (clip_queue_begin)
    CLIP_DO_RECORD_ADDITION = 1 << 2, // the recording in progress is an addition (clip_queue_mark_addition)
    CLIP_DO_CHARGE_SLOW = 1 << 3,     // release CHG_FAST
    CLIP_DO_WIFI_OFF = 1 << 4,
    CLIP_DO_WIFI_ON = 1 << 5,         // answer with CLIP_EV_WIFI
    CLIP_DO_UPLOAD = 1 << 6,          // run clip_upload_next in the upload task; answer with CLIP_EV_UPLOADED
    CLIP_DO_CHARGE_FAST = 1 << 7,     // drive CHG_FAST low
    CLIP_DO_SLEEP = 1 << 8,           // deep sleep; wake on the button, USB, and after wake_after_ms
    CLIP_DO_LED = 1 << 9,             // the pattern changed (clip_sm_t.led, .led_since_ms)
    // Look for a firmware update; answer with CLIP_EV_UPDATE_CHECKED. Given once per time on USB,
    // when Wi-Fi is up anyway after the queue was sent. How an update is fetched is the app's.
    CLIP_DO_CHECK_UPDATE = 1 << 10,
};

typedef struct {
    unsigned actions;
    int64_t wake_after_ms; // CLIP_DO_SLEEP: the timer wake; 0 is none
} clip_output_t;

// Plain data: the app keeps it in RTC memory, so the retry times survive deep sleep. Times are
// milliseconds on a clock that keeps counting through deep sleep (not the wall clock).
typedef struct {
    clip_state_t state;
    clip_led_t led;
    int64_t led_since_ms;
    bool usb, button_down, upload_busy, has_last_note;
    bool charge_fast; // CHG_FAST is driven
    bool update_checked; // since USB was plugged in
    bool user_active; // the button was used in this wake: a low battery is shown before sleeping
    clip_battery_t battery;
    clip_wifi_t wifi;
    clip_kind_t kind;   // of the recording in progress
    int64_t pressed_ms; // when it started
    int queued;
    int64_t due_ms;        // when the queue's next recording can be tried
    int offline_tries;     // Wi-Fi connects that failed in a row
    int64_t wifi_retry_ms; // no connect before this
} clip_sm_t;

// After power-on. has_last_note: clip_last_note() found one.
void clip_sm_init(clip_sm_t *sm, bool has_last_note);

// After a deep-sleep wake: awake and idle, the retry times kept. The app then sends USB, BATTERY
// and QUEUE, then BUTTON_DOWN if the button woke it, then ticks; only ticks and button, Wi-Fi and
// upload events make decisions, so the order of the first three does not matter.
void clip_sm_wake(clip_sm_t *sm);

clip_output_t clip_sm_step(clip_sm_t *sm, const clip_event_t *event, int64_t now_ms);

// The wait before Wi-Fi connect number `tries + 1`: 1 minute doubling, at most 1 hour.
int64_t clip_offline_backoff_ms(int tries);

// ---- files ----------------------------------------------------------------------------------------

// The flash file system, as much of it as the queue needs (LittleFS on the device, a fake in the
// tests). Names are flat. `write` must not return true before the data is on flash, and `rename`
// must be atomic and replace `to`; LittleFS gives both.
typedef struct {
    void *ctx;
    bool (*list)(void *ctx, void (*each)(void *arg, const char *name), void *arg);
    // A whole small file, malloc'd, with a NUL after its `len` bytes. False when it is not there.
    bool (*read)(void *ctx, const char *name, char **data, size_t *len);
    bool (*write)(void *ctx, const char *name, const char *data, size_t len);
    bool (*rename)(void *ctx, const char *from, const char *to);
    bool (*remove)(void *ctx, const char *name); // true when the file is gone afterwards
    int64_t (*size)(void *ctx, const char *name); // -1: not there
    void *(*open)(void *ctx, const char *name);   // for reading; NULL when it cannot
    size_t (*read_at)(void *ctx, void *file, int64_t offset, uint8_t *buf, size_t cap);
    void (*close)(void *ctx, void *file);
} clip_store_t;

// Time as the device knows it. The wall clock is unknown after a power loss until Wi-Fi sets it,
// so a recording also keeps which power-on it was made in and the time since then.
typedef struct {
    int64_t wall_ms;   // Unix time; 0: not set yet
    uint32_t boot;     // random, made at power-on, kept through deep sleep
    int64_t uptime_ms; // since that power-on, counting through deep sleep
} clip_clock_t;

typedef enum {
    CLIP_REC_RECORDING, // the file is being written (or power was lost while it was)
    CLIP_REC_QUEUED,
    CLIP_REC_FAILED, // given up; kept, not tried
} clip_rec_state_t;

// What is kept per recording, in <id>.meta next to <id>.ogg; Gemini's answer, once there, is in
// <id>.ans. Everything a retry needs is made once and stored before it is used, so repeating any
// step after a failure, a sleep or a power cut gives the same note.
typedef struct {
    char id[CLIP_ID_LEN];          // the recording, and the note it becomes when it is a new note
    char addition_id[CLIP_ID_LEN]; // its id as an addition
    clip_kind_t kind;
    char target[CLIP_ID_LEN]; // ADDITION: the note it adds to
    clip_rec_state_t state;
    uint32_t seq;       // the queue's order: oldest first
    int64_t created_s;  // 0: the clock was not set; worked out at upload from boot and uptime_ms
    uint32_t boot;
    int64_t uptime_ms;
    int64_t duration_ms;
    int64_t audio_bytes;   // how much of the file is sent; -1 all of it
    int num;               // the note number taken from the repo's counter; 0 none yet
    int64_t not_before_ms; // wall clock: no try before this
    int sync_tries;        // GitHub failures in a row
    cap_attempts_t attempts; // Gemini's
} clip_meta_t;

// The file's text (malloc'd): `key=value` lines under a `clip-meta 1` line, closed by a CRC-32
// line. clip_meta_parse is false for anything cut short or changed.
char *clip_meta_render(const clip_meta_t *meta);
bool clip_meta_parse(const char *text, size_t len, clip_meta_t *meta);

// Files are replaced by writing <name>.new and renaming it, so a power cut leaves the old file or
// the new one, never half of either.
bool clip_meta_save(const clip_store_t *store, const clip_meta_t *meta);
bool clip_meta_load(const clip_store_t *store, const char *id, clip_meta_t *meta);

char *clip_answer_render(const cap_answer_t *answer, size_t *len);
bool clip_answer_parse(const char *text, size_t len, cap_answer_t *answer);
bool clip_answer_save(const clip_store_t *store, const char *id, const cap_answer_t *answer);
bool clip_answer_load(const clip_store_t *store, const char *id, cap_answer_t *answer);

// "<id>.ogg", "<id>.meta", "<id>.ans".
#define CLIP_NAME_LEN 48
void clip_audio_name(const char *id, char out[CLIP_NAME_LEN]);

// An Ogg/Opus file that may have been cut short: how many bytes its complete pages take and how
// long the audio in them is (the last granule position, 48 kHz, less the pre-skip). False when
// no complete page holds audio. A file cut at any byte decodes up to there (Phase D.1).
bool clip_ogg_scan(const clip_store_t *store, const char *name, int64_t *complete_bytes, int64_t *duration_ms);

// The note a hold adds to: the last new note recorded here (uploaded yet or not).
bool clip_last_note(const clip_store_t *store, char id[CLIP_ID_LEN]);
bool clip_set_last_note(const clip_store_t *store, const char *id);

// ---- the queue ------------------------------------------------------------------------------------

// A recording starts: its ids are made from 32 random bytes and its .meta is written, as a new
// note, before the recorder creates <id>.ogg.
bool clip_queue_begin(const clip_store_t *store, const uint8_t random[32], const clip_clock_t *now, clip_meta_t *meta);

// The button was held: the recording adds to the last note. With no last note it stays a new note
// (false, which is not an error).
bool clip_queue_mark_addition(const clip_store_t *store, clip_meta_t *meta);

// The recording was closed: it is queued. A new note becomes the last note.
bool clip_queue_finish(const clip_store_t *store, clip_meta_t *meta, int64_t duration_ms);

typedef struct {
    int queued;    // cut-short recordings that were queued
    int dropped;   // recordings with no readable audio, and leftovers, removed
    int rebuilt;   // unreadable .meta files made again from their audio, as new notes
} clip_recovery_t;

// Once per boot, before anything else touches the store, and never while recording:
// - every *.new is removed (the rename did not happen; the old file stands);
// - a recording still marked as being written was cut by a power loss: it is queued with the
//   pages that are complete, or removed when none holds audio;
// - a .meta that does not read, with audio that does, becomes a new note under the file's id;
// - audio and answers without a .meta are removed: the .meta goes first when an upload is done.
bool clip_queue_recover(const clip_store_t *store, clip_recovery_t *out);

typedef struct {
    int queued;
    int failed;
    bool due;              // `id` can be tried now
    char id[CLIP_ID_LEN];
    int64_t wait_ms;       // until the first one can be tried (0 when due); -1 when nothing is queued
} clip_queue_status_t;

// Oldest first among those whose wait is over. A recording with no answer yet also waits for the
// Gemini gate (NULL: no gate), and an addition waits for the note it adds to while that is queued.
bool clip_queue_status(const clip_store_t *store, const cap_gate_t *gate, int64_t now_ms, clip_queue_status_t *out);

// Puts every given-up recording back in the queue with its tries forgotten (a console command).
int clip_queue_retry_failed(const clip_store_t *store);
// Removes a recording and everything kept for it.
void clip_queue_drop(const clip_store_t *store, const char *id);

// ---- the status file ------------------------------------------------------------------------------

#define CLIP_STATUS_PATH "devices/clip.json"
#define CLIP_STATUS_ERROR_LEN 96

// What the owner can see of the clip from the phone or laptop: a small file in the notes repo.
// The phone and the laptop only read `notes/` and `next-number`, so they ignore it.
typedef struct {
    int cell_mv;          // read at rest, before Wi-Fi came up
    bool usb;
    bool charging;        // the charger's CHRG pin
    const char *firmware; // a version: letters, digits and . _ + - only (anything else is left out)
    int64_t time_s;
    int queued;           // recordings still waiting
    const char *error;    // the last thing that went wrong, NULL or "" for nothing
} clip_status_t;

char *clip_status_render(const clip_status_t *status); // malloc'd JSON, one key per line

// Reads the file's sha, then writes it (or creates it). One try: a later report writes a
// newer one anyway.
cap_result_t clip_status_report(const cap_github_t *github, const clip_status_t *status);

// Every report is a commit in the notes repo, which costs the laptop a full sync and can make
// a note's write there collide once. So the file is written only when something in it is news.
#define CLIP_STATUS_PERCENT_STEP 20             // the battery moved this far since the last report
#define CLIP_STATUS_MAX_AGE_S (24 * 60 * 60)    // or the last report is a day old

// What the last report that reached GitHub said: kept on flash as the file "status".
typedef struct {
    int percent;
    clip_battery_t level;
    int64_t time_s;
    char firmware[32];
    char error[CLIP_STATUS_ERROR_LEN];
} clip_status_mark_t;

// The mark a report of `status` leaves.
void clip_status_mark(const clip_status_t *status, clip_status_mark_t *mark);
// True when `status` should be reported: there was no report yet (last NULL), the firmware
// or the last error changed, the battery went below (or back above) the upload limit or moved
// CLIP_STATUS_PERCENT_STEP, or the last report is older than CLIP_STATUS_MAX_AGE_S (also when
// the clock went backwards).
bool clip_status_due(const clip_status_mark_t *last, const clip_status_t *status);
bool clip_status_mark_load(const clip_store_t *store, clip_status_mark_t *mark);
bool clip_status_mark_save(const clip_store_t *store, const clip_status_mark_t *mark);

// ---- firmware updates over Wi-Fi ------------------------------------------------------------------

// The update manifest: a small text file of `key=value` lines at the provisioned `update_url`:
//   version=<the firmware's version>   (letters, digits and . _ + - only)
//   url=<where the image is>           (http:// or https://)
//   size=<bytes>
//   sha256=<64 hex digits of the image's SHA-256>
// Lines starting with # and unknown keys are skipped.
typedef struct {
    char version[32];
    char url[256];
    long size;
    uint8_t sha256[32];
} clip_manifest_t;

bool clip_manifest_parse(const char *text, clip_manifest_t *manifest);

typedef enum {
    CLIP_UPDATE_NONE,      // the manifest names what is running
    CLIP_UPDATE_INSTALL,
    CLIP_UPDATE_TOO_BIG,   // it does not fit the slot
    CLIP_UPDATE_KNOWN_BAD, // this version was installed before and rolled back: not again
} clip_update_t;

// running: this firmware's version. rolled_back: the version of the image the bootloader last
// gave up on (NULL or "" when there is none). slot_bytes: the size of the slot it would go into.
clip_update_t clip_update_decide(const clip_manifest_t *manifest, const char *running, const char *rolled_back,
                                 long slot_bytes);

// ---- the upload -----------------------------------------------------------------------------------

// The recording's bytes, in order. `read` fills up to `cap` bytes and returns how many; 0 is the end.
typedef struct {
    size_t size;
    size_t (*read)(void *ctx, uint8_t *buf, size_t cap);
    void *ctx;
} clip_audio_t;

// One POST to CAP_GEMINI_ENDPOINT whose body is prefix, the audio as base64, suffix: the device
// sends it in blocks with Content-Length cap_gemini_content_length(.., audio->size). *body is
// malloc'd (the caller of this function frees it). False on a network error.
typedef bool (*clip_gemini_fn)(void *ctx, const char *prefix, const clip_audio_t *audio, const char *suffix,
                               int *status, char **body);

typedef struct {
    const clip_store_t *store;
    const cap_github_t *github;
    clip_gemini_fn gemini;
    void *gemini_ctx;
    const char *prompt;        // capture/prompts/system_prompt.txt
    const char *prompt_append; // system_prompt_append.txt
    const char *schema;        // response_schema.json
    cap_gate_t *gate;          // kept in RTC memory
    // NULL, or what to report in the status file after a try that saved a note or gave one up
    // (queued is filled in here). False: nothing to report now (clip_status_due).
    bool (*status)(void *ctx, clip_upload_t result, clip_status_t *status);
    // NULL, or told how the report went, so the app can keep the mark of one that arrived.
    void (*status_done)(void *ctx, const clip_status_t *status, bool ok);
    void *status_ctx;
    void (*clock)(void *ctx, clip_clock_t *now); // the wall clock must be set: Wi-Fi is up
    void *clock_ctx;
} clip_uploader_t;

// Tries the queue's next due recording once, from wherever an earlier try got to:
//   an addition: read its note (gone: it becomes a new note)
//   no answer yet: Gemini, the answer stored
//   a new note: take a number, stored; create the note
//   an addition: add the answer to the note and replace it, merging when it changed meanwhile
//   done: .meta, .ans and the audio removed
// *after is the queue afterwards, for CLIP_EV_UPLOADED. With nothing due it is CLIP_UP_LATER.
// After a saved note, or one given up, the status file is written when the app has something to
// report; whether that works changes nothing here.
clip_upload_t clip_upload_next(const clip_uploader_t *up, clip_queue_status_t *after);
