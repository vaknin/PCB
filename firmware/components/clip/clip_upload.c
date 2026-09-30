// One queued recording's way to GitHub, through the capture component. Each call is one try
// that starts from what is on flash: the answer once Gemini gave it, the number once it was
// taken, and ids that were made with the recording. So a try that fails, or is cut by a sleep or
// a power loss, is simply made again, and the repo ends up with one note or one addition.
#include "clip.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Flash would not take a write: not a result the app sees (it becomes STUCK with a wait).
#define UP_STORAGE ((clip_upload_t)100)
#define STORE_RETRY_MS (5 * 60 * 1000)
#define LONGEST_BACKOFF 12 // cap_backoff_ms: 5 h

// ---- the status file -----------------------------------------------------------------------------

char *clip_status_render(const clip_status_t *status)
{
    char version[32] = "", time[CAP_ISO_LEN];
    size_t at = 0;
    for (const char *p = status->firmware ? status->firmware : ""; *p && at < sizeof version - 1; p++) {
        if ((*p >= '0' && *p <= '9') || (*p >= 'a' && *p <= 'z') || (*p >= 'A' && *p <= 'Z') || strchr("._+-", *p)) {
            version[at++] = *p;
        }
    }
    cap_iso_format(status->time_s, time);
    const size_t size = 320;
    char *out = malloc(size);
    if (out) {
        snprintf(out, size,
                 "{\n  \"device\": \"" CAP_SOURCE "\",\n  \"time\": \"%s\",\n  \"battery_percent\": %d,\n"
                 "  \"battery_mv\": %d,\n  \"usb\": %s,\n  \"charging\": %s,\n  \"queued\": %d,\n"
                 "  \"firmware\": \"%s\"\n}\n",
                 time, clip_battery_percent(status->cell_mv), status->cell_mv, status->usb ? "true" : "false",
                 status->charging ? "true" : "false", status->queued, version);
    }
    return out;
}

cap_result_t clip_status_report(const cap_github_t *github, const clip_status_t *status)
{
    cap_result_t out = {.status = CAP_RETRYABLE, .message = "Out of memory"};
    cap_http_response_t response = {0};
    char sha[CAP_SHA_LEN] = "", *was = NULL;
    char *get = cap_github_contents_url(github->repo, CLIP_STATUS_PATH, github->branch);
    char *put = cap_github_contents_url(github->repo, CLIP_STATUS_PATH, NULL);
    char *text = clip_status_render(status), *body = NULL;
    if (!get || !put || !text) {
        goto done;
    }
    if (!github->send(github->ctx, "GET", get, NULL, &response)) {
        snprintf(out.message, sizeof out.message, "Network error");
        goto done;
    }
    if (response.status == 200) {
        out = cap_github_parse_file(response.body ? response.body : "", &was, sha);
    } else if (response.status == 404) {
        out.status = CAP_OK; // the first report creates it
    } else {
        out = cap_github_classify(response.status, response.body ? response.body : "", response.rate_limited);
        if (out.status == CAP_OK) {
            out.status = CAP_RETRYABLE;
        }
    }
    free(response.body);
    response = (cap_http_response_t){0};
    if (out.status != CAP_OK) {
        goto done;
    }
    out = (cap_result_t){.status = CAP_RETRYABLE, .message = "Out of memory"};
    body = cap_github_put_body(CAP_SOURCE ": status", text, github->branch, *sha ? sha : NULL);
    if (!body) {
        goto done;
    }
    if (!github->send(github->ctx, "PUT", put, body, &response)) {
        snprintf(out.message, sizeof out.message, "Network error");
        goto done;
    }
    if (response.status == 200 || response.status == 201) {
        out = (cap_result_t){.status = CAP_OK};
    } else {
        out = cap_github_classify(response.status, response.body ? response.body : "", response.rate_limited);
        if (out.status == CAP_OK) {
            out.status = CAP_RETRYABLE;
        }
    }
done:
    free(response.body);
    free(was);
    free(get);
    free(put);
    free(text);
    free(body);
    return out;
}

// ---- the upload ----------------------------------------------------------------------------------

typedef struct {
    const clip_store_t *store;
    void *file;
    int64_t at, end;
} reader_t;

static size_t read_audio(void *ctx, uint8_t *buf, size_t cap)
{
    reader_t *reader = ctx;
    if ((int64_t)cap > reader->end - reader->at) {
        cap = (size_t)(reader->end - reader->at);
    }
    size_t got = cap ? reader->store->read_at(reader->store->ctx, reader->file, reader->at, buf, cap) : 0;
    reader->at += (int64_t)got;
    return got;
}

static int64_t wall_ms(const clip_uploader_t *up)
{
    clip_clock_t now;
    up->clock(up->clock_ctx, &now);
    return now.wall_ms;
}

// The recording stays queued and is not tried before `wait_ms` from now.
static clip_upload_t postpone(const clip_uploader_t *up, clip_meta_t *meta, int64_t wait_ms, const char *why,
                              clip_upload_t result)
{
    meta->not_before_ms = wall_ms(up) + wait_ms;
    memset(meta->attempts.last_error, 0, sizeof meta->attempts.last_error);
    strncpy(meta->attempts.last_error, why, sizeof meta->attempts.last_error - 1);
    return clip_meta_save(up->store, meta) ? result : UP_STORAGE;
}

// GitHub never makes a recording give up: a bad token or repo is put right over USB, and the
// note is still wanted then. It only waits longer.
static clip_upload_t github_failed(const clip_uploader_t *up, clip_meta_t *meta, cap_result_t result)
{
    bool terminal = result.status == CAP_TERMINAL;
    meta->sync_tries++;
    return postpone(up, meta, cap_backoff_ms(terminal ? LONGEST_BACKOFF : meta->sync_tries), result.message,
                    terminal ? CLIP_UP_STUCK : CLIP_UP_LATER);
}

// The note a hold was to add to is not on GitHub (ticked off, or never made): the recording
// becomes a new note. It also becomes the last note, unless a newer one was recorded since.
static bool become_new(const clip_uploader_t *up, clip_meta_t *meta)
{
    char last[CLIP_ID_LEN];
    // the last note first: cut between the two, the next try comes here again
    if (clip_last_note(up->store, last) && strcmp(last, meta->target) == 0 && !clip_set_last_note(up->store, meta->id)) {
        return false;
    }
    meta->kind = CLIP_NEW;
    meta->target[0] = 0;
    return clip_meta_save(up->store, meta);
}

// Sends the recording to Gemini: `note` is the note it adds to, NULL for a new note. True with
// the answer (stored on flash), or false with *out.
static bool ask_gemini(const clip_uploader_t *up, clip_meta_t *meta, const cap_note_t *note, cap_answer_t *answer,
                       clip_upload_t *out)
{
    const clip_store_t *store = up->store;
    if (cap_gate_wait_ms(up->gate, wall_ms(up)) > 0) {
        *out = CLIP_UP_LATER; // reading the note took the gate's time; the queue's status has the wait
        return false;
    }
    char name[CLIP_NAME_LEN];
    clip_audio_name(meta->id, name);
    int64_t size = store->size(store->ctx, name);
    void *file = size > 0 ? store->open(store->ctx, name) : NULL;
    if (!file) {
        clip_queue_drop(store, meta->id); // no audio: nothing can be made of it
        *out = CLIP_UP_GAVE_UP;
        return false;
    }
    char *note_text = note ? cap_note_whole_text(note) : NULL;
    cap_gemini_request_t request = {0};
    if ((note && !note_text) ||
        !cap_gemini_request(note ? up->prompt_append : up->prompt, up->schema, note_text, CAP_MIME_OGG, &request)) {
        store->close(store->ctx, file);
        free(note_text);
        *out = postpone(up, meta, cap_backoff_ms(1), "Could not build the request", CLIP_UP_LATER);
        return false;
    }
    reader_t reader = {.store = store, .file = file, .end = size};
    if (meta->audio_bytes >= 0 && meta->audio_bytes < size) {
        reader.end = meta->audio_bytes; // cut short by a power loss: only its complete pages
    }
    clip_audio_t audio = {.size = (size_t)reader.end, .read = read_audio, .ctx = &reader};
    int status = 0;
    char *body = NULL;
    bool sent = up->gemini(up->gemini_ctx, request.prefix, &audio, request.suffix, &status, &body);
    store->close(store->ctx, file);
    cap_gemini_request_free(&request);
    free(note_text);
    int64_t now = wall_ms(up);
    cap_gate_finished(up->gate, now);
    if (!sent) {
        // Like the phone, which only tries with a network: no attempt is used up.
        free(body);
        *out = CLIP_UP_OFFLINE;
        return false;
    }
    cap_gemini_result_t result;
    cap_gemini_interpret(status, body, now, &result);
    free(body);
    if (result.kind != CAP_GEMINI_PARSED) {
        if (cap_gemini_failed(&meta->attempts, &result, up->gate, now) == CAP_GIVE_UP) {
            meta->state = CLIP_REC_FAILED;
            *out = CLIP_UP_GAVE_UP;
        } else {
            // after a rate limit the gate holds every recording; otherwise this one backs off
            meta->not_before_ms = now + (result.retry_after_ms >= 0 ? 0 : cap_backoff_ms(meta->attempts.attempts));
            *out = CLIP_UP_LATER;
        }
        cap_gemini_result_free(&result);
        if (!clip_meta_save(store, meta)) {
            *out = UP_STORAGE;
        }
        return false;
    }
    *answer = result.answer;
    // Not stored is not fatal: this try goes on with it, and a later one would ask Gemini again.
    clip_answer_save(store, meta->id, answer);
    return true;
}

static clip_upload_t upload_one(const clip_uploader_t *up, clip_meta_t *meta)
{
    const clip_store_t *store = up->store;
    if (meta->created_s == 0) {
        // Recorded before the clock was ever set. In the same power-on the time since is known;
        // after a power loss it is not, and the note is dated now, as an import would be.
        clip_clock_t now;
        up->clock(up->clock_ctx, &now);
        int64_t made = now.wall_ms;
        if (meta->boot == now.boot && now.uptime_ms >= meta->uptime_ms) {
            made -= now.uptime_ms - meta->uptime_ms;
        }
        meta->created_s = made / 1000;
        if (!clip_meta_save(store, meta)) {
            return UP_STORAGE;
        }
    }
    cap_answer_t answer;
    cap_note_t note = {.duration_ms = CAP_NO_DURATION};
    char sha[CAP_SHA_LEN] = "";
    clip_upload_t out = CLIP_UP_SAVED;
    bool have_answer = clip_answer_load(store, meta->id, &answer);

    if (meta->kind == CLIP_ADDITION) {
        cap_result_t got = cap_github_get_note(up->github, meta->target, &note, sha);
        if (got.status == CAP_GONE) {
            if (!become_new(up, meta)) {
                out = UP_STORAGE;
                goto done;
            }
        } else if (got.status != CAP_OK) {
            out = github_failed(up, meta, got);
            goto done;
        }
        for (size_t i = 0; i < note.addition_count; i++) {
            if (strcmp(note.additions[i].id, meta->addition_id) == 0) {
                clip_queue_drop(store, meta->id); // an earlier try got through and its answer was lost
                goto done;
            }
        }
    }
    if (!have_answer && !ask_gemini(up, meta, meta->kind == CLIP_ADDITION ? &note : NULL, &answer, &out)) {
        goto done;
    }
    if (meta->kind == CLIP_ADDITION) {
        cap_add_t added = cap_note_add_answer(&note, meta->addition_id, meta->created_s, meta->duration_ms, &answer);
        if (added == CAP_ADD_NO_MEMORY) {
            out = postpone(up, meta, cap_backoff_ms(1), "Out of memory", CLIP_UP_LATER);
            goto done;
        }
        if (added == CAP_ADD_NOTHING_HEARD) {
            clip_queue_drop(store, meta->id);
            out = CLIP_UP_NOTHING_ADDED;
            goto done;
        }
        cap_result_t put = cap_github_update_note(up->github, meta->target, &note, sha);
        if (put.status == CAP_OK) {
            clip_queue_drop(store, meta->id);
            goto done;
        }
        if (put.status != CAP_GONE) {
            out = github_failed(up, meta, put);
            goto done;
        }
        if (!become_new(up, meta)) { // removed while the answer was being made
            out = UP_STORAGE;
            goto done;
        }
    }
    if (meta->num == 0) {
        int number = 0;
        cap_result_t reserved = cap_github_reserve_number(up->github, &number);
        if (reserved.status != CAP_OK) {
            out = github_failed(up, meta, reserved);
            goto done;
        }
        // Stored before the note is created, so a repeated create keeps its number. A cut right
        // here loses the number (a gap in the numbering, as on the phone), never the note.
        meta->num = number;
        if (!clip_meta_save(store, meta)) {
            out = UP_STORAGE;
            goto done;
        }
    }
    cap_note_free(&note);
    note = (cap_note_t){
        .has_created = true, .created_s = meta->created_s, .num = meta->num, .duration_ms = meta->duration_ms};
    if (!cap_note_set_answer(&note, &answer)) {
        out = postpone(up, meta, cap_backoff_ms(1), "Out of memory", CLIP_UP_LATER);
        goto done;
    }
    cap_result_t created = cap_github_create_note(up->github, meta->id, &note, sha);
    if (created.status != CAP_OK) {
        out = github_failed(up, meta, created);
        goto done;
    }
    clip_queue_drop(store, meta->id);
done:
    cap_answer_free(&answer);
    cap_note_free(&note);
    return out;
}

clip_upload_t clip_upload_next(const clip_uploader_t *up, clip_queue_status_t *after)
{
    clip_queue_status_t status;
    clip_meta_t meta;
    clip_upload_t out = CLIP_UP_LATER;
    if (!clip_queue_status(up->store, up->gate, wall_ms(up), &status)) {
        out = UP_STORAGE;
        status = (clip_queue_status_t){.queued = 1};
    } else if (status.due) {
        out = clip_meta_load(up->store, status.id, &meta) ? upload_one(up, &meta) : UP_STORAGE;
    }
    if (!clip_queue_status(up->store, up->gate, wall_ms(up), after)) {
        *after = status;
        out = UP_STORAGE;
    }
    if (out == CLIP_UP_SAVED && up->status) {
        clip_status_t report = {0};
        if (up->status(up->status_ctx, &report)) {
            report.queued = after->queued;
            clip_status_report(up->github, &report); // the note is saved whatever this says
        }
    }
    if (out == UP_STORAGE) {
        // the file system is in trouble: not again at once
        out = CLIP_UP_STUCK;
        after->due = false;
        if (after->wait_ms < STORE_RETRY_MS) {
            after->wait_ms = STORE_RETRY_MS;
        }
    }
    return out;
}
