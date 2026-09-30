// The Capture client logic (D-025 Phase D.2): everything a device needs to be a third Capture
// client next to the phone and the laptop, as pure C with no ESP-IDF calls, so the laptop tests
// cover it (firmware/test/test_capture_*.c). The rules are Capture's own (its SPEC.md §6 and §9);
// each function names the Kotlin it follows. HTTP goes through a function the app supplies.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// ---- time ------------------------------------------------------------------------------------

#define CAP_ISO_LEN 32

// "2026-09-19T07:42:13Z", as Instant.toString() truncated to seconds.
void cap_iso_format(int64_t epoch_s, char out[CAP_ISO_LEN]);

// Instant.parse, then OffsetDateTime.parse: `Z` or a `±hh:mm` offset; fractions are dropped.
bool cap_iso_parse(const char *text, int64_t *epoch_s);

// Until Gemini's daily quota resets: midnight Pacific time, plus a minute (untilQuotaResetMs).
int64_t cap_until_quota_reset_ms(int64_t now_ms);

// ---- the note file (NoteFile.kt) ----------------------------------------------------------------

#define CAP_ID_LEN 65 // [0-9a-fA-F-]{16,64}
#define CAP_NO_DURATION (-1)
#define CAP_SOURCE "clip"
#define CAP_NOTHING_HEARD "Nothing heard"

// Text added to a note after it was made. The strings are malloc'd.
typedef struct {
    char id[CAP_ID_LEN];
    int64_t created_s;
    char *source;
    int64_t duration_ms; // CAP_NO_DURATION for typed text
    char *text;
} cap_addition_t;

// A note file's content. The strings are malloc'd; cap_note_free() releases them.
typedef struct {
    bool has_created; // false: missing or unreadable in the file
    int64_t created_s;
    int num;             // 0: no number yet
    int64_t duration_ms; // CAP_NO_DURATION when the file has none (a typed note)
    char *title;
    char *summary;
    char *transcript;
    char *source; // NULL when the file names none
    cap_addition_t *additions;
    size_t addition_count;
} cap_note_t;

void cap_note_free(cap_note_t *note);

// 32 lowercase hex digits from 16 random bytes (NoteFile.newId).
void cap_new_id(const uint8_t random[16], char out[33]);

// "notes/<id>.md" (malloc'd).
char *cap_note_path(const char *id);

// The file's bytes (malloc'd), exactly as NoteFile.render writes them. NULL without a `created`
// time or memory. A note with no source is written as the phone's, as the phone would.
char *cap_note_render(const char *id, const cap_note_t *note);

// Tolerant, like NoteFile.parse: hand-edited files, CRLF and a BOM are read. False only when out
// of memory; *note is then empty.
bool cap_note_parse(const char *text, cap_note_t *note);

// What Gemini is told the note already says: its own text, then each addition, blank lines
// between (Additions.wholeText). A note with no transcript and no duration was typed before
// Capture 0.8.0 and its title is its text (Additions.original). malloc'd.
char *cap_note_whole_text(const cap_note_t *note);

// Gemini's answer to one recording. The strings are malloc'd.
typedef struct {
    char *title;
    char *summary;
    char *transcript;
} cap_answer_t;

void cap_answer_free(cap_answer_t *answer);

// Fills a new note's title, summary and transcript from the answer (Repository.saveResult): a
// blank transcript makes a "Nothing heard" note; a blank title falls back to the summary's first
// 8 words, then "Untitled". A note with no source becomes this device's (CAP_SOURCE).
// note->duration_ms must already be set. False when out of memory.
bool cap_note_set_answer(cap_note_t *note, const cap_answer_t *answer);

typedef enum {
    CAP_ADDED,
    CAP_ADD_NOTHING_HEARD, // the recording had no speech: nothing is added (Repository.saveAddition)
    CAP_ADD_NO_MEMORY,
} cap_add_t;

// Adds a recording's answer to a note as a `## Added` section from this device; the answer's
// title and summary replace the note's unless blank (Repository.fold).
cap_add_t cap_note_add_answer(cap_note_t *note, const char *addition_id, int64_t created_s, int64_t duration_ms,
                              const cap_answer_t *answer);

typedef enum {
    CAP_MERGED,        // *remote now holds the union and the title and summary made here
    CAP_REMOTE_HAS_ALL, // GitHub's file already has every addition made here: it wins, untouched
    CAP_MERGE_NO_MEMORY,
} cap_merge_t;

// The note changed on GitHub while an addition was made here (SPEC §9, Additions; the laptop's
// merge_note): GitHub's file is the base, the additions are the union by id, oldest first, and
// the title and summary made here are kept.
cap_merge_t cap_note_merge(cap_note_t *remote, const cap_note_t *ours);

// ---- Gemini (Gemini.kt, GeminiConfig.kt) --------------------------------------------------------

#define CAP_GEMINI_ENDPOINT "https://generativelanguage.googleapis.com/v1beta/interactions"
#define CAP_GEMINI_MODEL "gemini-3.5-flash-lite"
#define CAP_GEMINI_THINKING_LEVEL "high"
#define CAP_GEMINI_TEMPERATURE "0.2"
#define CAP_GEMINI_USER_TEXT "Process this recording."
#define CAP_GEMINI_USER_TEXT_APPEND "Process this recording, which adds to the note above."
#define CAP_GEMINI_CALL_TIMEOUT_MS (5 * 60 * 1000)
#define CAP_GEMINI_MAX_ATTEMPTS 8
#define CAP_GEMINI_REQUESTS_PER_MINUTE 15
// 60 s / REQUESTS_PER_MINUTE, plus a second of margin (ProcessWorker's gate).
#define CAP_GEMINI_MIN_INTERVAL_MS (60000 / CAP_GEMINI_REQUESTS_PER_MINUTE + 1000)
#define CAP_MIME_OGG "audio/ogg"

// The request body in two halves: the audio goes between them as base64, so the device can
// stream it from flash in blocks and still send a fixed Content-Length.
typedef struct {
    char *prefix;
    char *suffix;
} cap_gemini_request_t;

// requestBody, or appendRequestBody when note_text (the note so far) is not NULL. `prompt` is
// Capture's system prompt (trailing whitespace is dropped here, as the app does) and `schema`
// its response schema, which must be JSON. False on a bad schema or no memory.
bool cap_gemini_request(const char *prompt, const char *schema, const char *note_text, const char *mime_type,
                        cap_gemini_request_t *out);
void cap_gemini_request_free(cap_gemini_request_t *request);
size_t cap_gemini_content_length(const cap_gemini_request_t *request, size_t audio_bytes);

// Standard base64 with padding. cap_base64_encode writes cap_base64_len(n) characters and no NUL;
// feed it blocks that are a multiple of 3 bytes until the last.
size_t cap_base64_len(size_t n);
size_t cap_base64_encode(const uint8_t *in, size_t n, char *out);

typedef enum {
    CAP_GEMINI_PARSED,
    CAP_GEMINI_NOT_COMPLETED, // the interaction's status is not `completed`
    CAP_GEMINI_FAILED,
} cap_gemini_kind_t;

typedef struct {
    cap_gemini_kind_t kind;
    cap_answer_t answer;       // PARSED
    long long input_tokens;    // PARSED; -1 when absent
    long long output_tokens;
    char status[32];           // NOT_COMPLETED
    char message[256];         // FAILED
    bool terminal;             // FAILED: waiting will not help
    bool unreadable;           // FAILED: the body could not be read and is worth logging
    int64_t retry_after_ms;    // FAILED on a rate limit (HTTP 429): how long to hold; else -1
} cap_gemini_result_t;

// Reads an HTTP response (interpret). code 0 is a network error. Bad request, bad key and other
// 4xx are terminal; 408, 429, 5xx and unreadable answers retry.
void cap_gemini_interpret(int code, const char *body, int64_t now_ms, cap_gemini_result_t *out);
void cap_gemini_result_free(cap_gemini_result_t *result);

bool cap_gemini_is_daily_quota(const char *body);
// How long a 429 asks to wait, or -1 (retryDelayMs).
int64_t cap_gemini_retry_delay_ms(const char *body);

// One Gemini request at a time, spaced, and held back after a rate limit (RateGate.kt). The app
// runs requests from one task, so this only keeps the time; `now_ms` is wall-clock time, and the
// struct is plain data so it can be kept across deep sleep.
typedef struct {
    int64_t min_interval_ms;
    int64_t next_allowed_ms;
} cap_gate_t;

int64_t cap_gate_wait_ms(const cap_gate_t *gate, int64_t now_ms); // 0: a request may start now
void cap_gate_finished(cap_gate_t *gate, int64_t now_ms);         // call when a request ends
void cap_gate_hold(cap_gate_t *gate, int64_t now_ms, int64_t delay_ms);

// What is kept per queued recording between tries.
typedef struct {
    int attempts;
    char last_error[256];
} cap_attempts_t;

typedef enum {
    CAP_TRY_AGAIN,
    CAP_GIVE_UP, // terminal, or CAP_GEMINI_MAX_ATTEMPTS used up
} cap_retry_t;

// What a result other than PARSED does (ProcessWorker.answered, Repository.processingFailed): a
// rate limit holds the gate and does not use up an attempt; a bad status is terminal only when
// the attempt before gave the same one.
cap_retry_t cap_gemini_failed(cap_attempts_t *state, const cap_gemini_result_t *result, cap_gate_t *gate,
                              int64_t now_ms);

// The wait before try `attempts + 1`: 30 s doubling, at most 5 h (WorkManager's exponential backoff).
int64_t cap_backoff_ms(int attempts);

// ---- GitHub (GitHub.kt, GitHubClient.kt) ----------------------------------------------------------

#define CAP_SHA_LEN 65
#define CAP_COUNTER_PATH "next-number"
#define CAP_RESERVE_TRIES 5
#define CAP_UPDATE_TRIES 3

typedef enum {
    CAP_OK,
    CAP_RETRYABLE, // network, rate limit, the repo moved on: try later
    CAP_TERMINAL,  // token, repo, a hand-edited counter: waiting will not help
    CAP_GONE,      // the note's file is not on GitHub (any more)
} cap_status_t;

typedef struct {
    cap_status_t status;
    char message[160];
} cap_result_t;

typedef struct {
    int status;        // the HTTP status
    char *body;        // malloc'd, NUL-terminated; the caller of `send` frees it
    bool rate_limited; // a `retry-after` header, or `x-ratelimit-remaining: 0`
} cap_http_response_t;

// One HTTPS request to api.github.com. The app adds the headers (Authorization: Bearer, Accept:
// application/vnd.github+json, X-GitHub-Api-Version, User-Agent). `body` is NULL for a GET.
// Returns false on a network error.
typedef bool (*cap_http_fn)(void *ctx, const char *method, const char *url, const char *body,
                            cap_http_response_t *response);

typedef struct {
    const char *repo;   // "owner/name"
    const char *branch;
    cap_http_fn send;
    void *ctx;
} cap_github_t;

// https://api.github.com/repos/<repo>/contents/<path>[?ref=<branch>] (malloc'd). Reads name the
// branch with `ref`; writes name it in the body.
char *cap_github_contents_url(const char *repo, const char *path, const char *ref_branch);

// A contents PUT: `sha` replaces the file at that sha; NULL creates it (putBody). malloc'd.
char *cap_github_put_body(const char *message, const char *text, const char *branch, const char *sha);

cap_result_t cap_github_classify(int code, const char *body, bool rate_limited);
cap_result_t cap_github_parse_counter(const char *body, int *next, char sha[CAP_SHA_LEN]);
bool cap_github_parse_put_sha(const char *body, char sha[CAP_SHA_LEN]);
// A contents GET of a file: its text (malloc'd) and sha.
cap_result_t cap_github_parse_file(const char *body, char **text, char sha[CAP_SHA_LEN]);

// Takes the next note number (SPEC §9, Numbers; reserveNumber): read the counter, move it on from
// what was read, start over on 409/422, at most CAP_RESERVE_TRIES times. The caller stores the
// number before creating the note's file, so a retried upload reuses it.
cap_result_t cap_github_reserve_number(const cap_github_t *github, int *number);

// Creates notes/<id>.md. Safe to repeat: when the file is already there (an earlier try whose
// answer was lost), that counts as done.
cap_result_t cap_github_create_note(const cap_github_t *github, const char *id, const cap_note_t *note,
                                    char sha[CAP_SHA_LEN]);

// Reads notes/<id>.md. CAP_GONE when it is not there.
cap_result_t cap_github_get_note(const cap_github_t *github, const char *id, cap_note_t *note, char sha[CAP_SHA_LEN]);

// Replaces notes/<id>.md, read at `sha`, with *note, which has an addition made here. When the
// file changed meanwhile it is read again and merged (cap_note_merge), at most CAP_UPDATE_TRIES
// times. Safe to repeat. On CAP_OK *note and sha are what GitHub now has; when the tries run out
// they are the last merge and the sha it was read at, so the next call starts from there.
cap_result_t cap_github_update_note(const cap_github_t *github, const char *id, cap_note_t *note,
                                    char sha[CAP_SHA_LEN]);
