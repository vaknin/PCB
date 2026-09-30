// SOURCES: firmware/components/clip/clip_upload.c firmware/components/clip/clip_queue.c firmware/components/clip/clip_power.c firmware/components/capture/cap_text.c firmware/components/capture/cap_time.c firmware/components/capture/cap_note.c firmware/components/capture/cap_gemini.c firmware/components/capture/cap_github.c
// A queued recording's way to GitHub, against a made-up Gemini and a notes repo that behaves
// like GitHub's: the plain cases, every failure, and a power loss, a dead file system or a lost
// connection at every step, after which the repo must hold exactly one note or one addition.
#include "test_clip_fake.h"

#define T0 INT64_C(1789803733000)
#define ANSWER "{\"transcript\":\"We need a skimmer for the pool.\",\"title\":\"Pool skimmer\",\"summary\":\"Buy a skimmer.\"}"
#define ANSWER_ADD "{\"transcript\":\"And a long pole.\",\"title\":\"Pool gear\",\"summary\":\"Buy a skimmer and a pole.\"}"
#define ANSWER_SILENT "{\"transcript\":\"\",\"title\":\"\",\"summary\":\"\"}"
#define NEW_NOTE_LOG "GET next-number\nPUT next-number\nPUT notes/%s.md\n"

static fake_store_t fs;
static clip_store_t store;
static fake_github_t gh;
static fake_gemini_t gm;
static cap_gate_t gate;
static clip_clock_t clk;
static cap_github_t github;
static clip_uploader_t up;
static bool power_model; // a dead store means the whole device is off: no request leaves it

static void clock_now(void *ctx, clip_clock_t *now)
{
    (void)ctx;
    *now = clk;
}

static bool http(void *ctx, const char *method, const char *url, const char *body, cap_http_response_t *response)
{
    if (power_model && fs.dead) {
        return false;
    }
    return gh_send(ctx, method, url, body, response);
}

static bool gemini(void *ctx, const char *prefix, const clip_audio_t *audio, const char *suffix, int *status, char **body)
{
    if (power_model && fs.dead) {
        return false;
    }
    return gemini_send(ctx, prefix, audio, suffix, status, body);
}

static void world(void)
{
    fake_store_free(&fs);
    gh_free(&gh);
    gemini_free(&gm);
    store = fake_store(&fs);
    gemini_answers(&gm, 200, fake_interaction(ANSWER, "completed"));
    gate = (cap_gate_t){.min_interval_ms = CAP_GEMINI_MIN_INTERVAL_MS};
    clk = (clip_clock_t){.wall_ms = T0, .boot = 77, .uptime_ms = 600000};
    github = (cap_github_t){.repo = "owner/notes", .branch = "main", .send = http, .ctx = &gh};
    up = (clip_uploader_t){
        .store = &store,
        .github = &github,
        .gemini = gemini,
        .gemini_ctx = &gm,
        .prompt = "PROMPT",
        .prompt_append = "PROMPT FOR ADDING",
        .schema = "{\"type\":\"object\"}",
        .gate = &gate,
        .clock = clock_now,
    };
    power_model = false;
}

static void later(int64_t ms)
{
    clk.wall_ms += ms;
    clk.uptime_ms += ms;
}

// A finished recording of `seconds`, a press or a hold.
static clip_meta_t record(uint8_t seed, bool hold, int seconds)
{
    clip_meta_t meta;
    char name[CLIP_NAME_LEN];
    size_t len;
    uint8_t *ogg = fake_ogg(seconds, 0, &len);
    CHECK(clip_queue_begin(&store, fake_random(seed), &clk, &meta));
    clip_audio_name(meta.id, name);
    fake_put(&fs, name, ogg, len);
    free(ogg);
    if (hold) {
        clip_queue_mark_addition(&store, &meta);
    }
    CHECK(clip_queue_finish(&store, &meta, seconds * 1000));
    return meta;
}

static const char *note_path(const char *id)
{
    static char path[96];
    snprintf(path, sizeof path, "notes/%s.md", id);
    return path;
}

// The note as GitHub has it; empty when it is not there.
static cap_note_t note_on_github(const char *id)
{
    cap_note_t note = {.duration_ms = CAP_NO_DURATION};
    gh_file_t *file = gh_find(&gh, note_path(id));
    CHECK(file != NULL);
    if (file) {
        CHECK(cap_note_parse(file->text, &note));
    }
    return note;
}

static const char *log_from(int request)
{
    const char *p = gh.log;
    for (int i = 0; i < request && p; i++) {
        p = strchr(p, '\n');
        p = p ? p + 1 : NULL;
    }
    return p ? p : "";
}

// Tries until the queue is empty, waiting as long as each try asks.
static int run_queue(void)
{
    clip_queue_status_t after = {.queued = 1};
    int tries = 0;
    while (after.queued > 0 && tries < 80) {
        clip_upload_next(&up, &after);
        tries++;
        later(after.wait_ms > 0 ? after.wait_ms : 0);
    }
    CHECK_INT(after.queued, 0);
    return tries;
}

// The first note, already on GitHub as #1, and the queue empty again.
static clip_meta_t first_note_uploaded(void)
{
    clip_queue_status_t after;
    clip_meta_t first = record(1, false, 3);
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    later(10000);
    gemini_answers(&gm, 200, fake_interaction(ANSWER_ADD, "completed"));
    return first;
}

// ---- the plain cases -----------------------------------------------------------------------------

static void a_new_note_is_asked_numbered_and_created(void)
{
    world();
    clip_meta_t meta = record(1, false, 3);
    char audio[CLIP_NAME_LEN], expected[200];
    clip_audio_name(meta.id, audio);
    int64_t audio_size = fake_size(&fs, audio);
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    CHECK_INT(after.queued, 0);
    CHECK_INT(after.wait_ms, -1);

    CHECK_INT(gm.calls, 1);
    CHECK_INT(gm.audio_bytes, audio_size);
    CHECK(strstr(gm.prefix, "\"PROMPT\"") != NULL);
    CHECK(strstr(gm.prefix, "\"" CAP_GEMINI_USER_TEXT "\"") != NULL);
    CHECK(strstr(gm.prefix, "\"mime_type\":\"audio/ogg\"") != NULL);
    CHECK_INT(gate.next_allowed_ms, T0 + CAP_GEMINI_MIN_INTERVAL_MS);

    snprintf(expected, sizeof expected, NEW_NOTE_LOG, meta.id);
    CHECK_STR(gh.log, expected);
    CHECK_STR(gh_find(&gh, "next-number")->text, "2\n");
    cap_note_t note = note_on_github(meta.id);
    CHECK_STR(note.title, "Pool skimmer");
    CHECK_STR(note.summary, "Buy a skimmer.");
    CHECK_STR(note.transcript, "We need a skimmer for the pool.");
    CHECK_STR(note.source, "clip");
    CHECK_INT(note.num, 1);
    CHECK_INT(note.created_s, T0 / 1000);
    CHECK_INT(note.duration_ms, 3000);
    CHECK_INT(note.addition_count, 0);
    // byte for byte what the capture component renders
    char *rendered = cap_note_render(meta.id, &note);
    CHECK_STR(gh_find(&gh, note_path(meta.id))->text, rendered);
    free(rendered);
    cap_note_free(&note);

    // nothing is left on flash but the last note's id
    char last[CLIP_ID_LEN];
    CHECK_INT(fake_count(&fs), 1);
    CHECK(clip_last_note(&store, last));
    CHECK_STR(last, meta.id);
    // and with nothing queued, a try is a no-op
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
    CHECK_INT(gh.requests, 3);
}

static void an_addition_reads_the_note_asks_and_replaces_it(void)
{
    world();
    clip_meta_t first = first_note_uploaded();
    clip_meta_t add = record(2, true, 5);
    CHECK_INT(add.kind, CLIP_ADDITION);
    clip_queue_status_t after;
    int before = gh.requests;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    char expected[200];
    snprintf(expected, sizeof expected, "GET notes/%s.md\nPUT notes/%s.md\n", first.id, first.id);
    CHECK_STR(log_from(before), expected);

    // Gemini got the note so far, and the adding prompt
    CHECK_INT(gm.calls, 2);
    CHECK(strstr(gm.prefix, "\"PROMPT FOR ADDING\"") != NULL);
    CHECK(strstr(gm.prefix, "We need a skimmer for the pool.") != NULL);
    CHECK(strstr(gm.prefix, "\"" CAP_GEMINI_USER_TEXT_APPEND "\"") != NULL);

    cap_note_t note = note_on_github(first.id);
    CHECK_INT(gh_count(&gh, "notes/"), 1);
    CHECK_STR(note.title, "Pool gear");
    CHECK_STR(note.summary, "Buy a skimmer and a pole.");
    CHECK_STR(note.transcript, "We need a skimmer for the pool.");
    CHECK_INT(note.num, 1);
    CHECK_INT(note.addition_count, 1);
    if (note.addition_count == 1) {
        CHECK_STR(note.additions[0].id, add.addition_id);
        CHECK_STR(note.additions[0].text, "And a long pole.");
        CHECK_STR(note.additions[0].source, "clip");
        CHECK_INT(note.additions[0].duration_ms, 5000);
        CHECK_INT(note.additions[0].created_s, (T0 + 10000) / 1000);
    }
    cap_note_free(&note);
    CHECK_STR(gh_find(&gh, "next-number")->text, "2\n"); // an addition takes no number
    char last[CLIP_ID_LEN];
    CHECK_INT(fake_count(&fs), 1);
    CHECK(clip_last_note(&store, last));
    CHECK_STR(last, first.id);
}

static void a_hold_whose_note_is_gone_becomes_a_new_note(void)
{
    world();
    clip_meta_t first = first_note_uploaded();
    clip_meta_t add = record(2, true, 5);
    gh_set(&gh, note_path(first.id), NULL); // ticked off on the phone
    clip_queue_status_t after;
    int before = gh.requests;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    char expected[300];
    snprintf(expected, sizeof expected, "GET notes/%s.md\n" NEW_NOTE_LOG, first.id, add.id);
    CHECK_STR(log_from(before), expected);
    CHECK(strstr(gm.prefix, "\"PROMPT\"") != NULL); // asked as a new note
    CHECK_INT(gh_count(&gh, "notes/"), 1);
    cap_note_t note = note_on_github(add.id);
    CHECK_STR(note.transcript, "And a long pole.");
    CHECK_INT(note.num, 2);
    CHECK_INT(note.addition_count, 0);
    cap_note_free(&note);
    // and it is what the next hold adds to
    char last[CLIP_ID_LEN];
    CHECK(clip_last_note(&store, last));
    CHECK_STR(last, add.id);
    CHECK_INT(fake_count(&fs), 1);
}

static void a_gone_note_does_not_take_the_place_of_a_newer_last_note(void)
{
    world();
    clip_meta_t first = first_note_uploaded();
    record(2, true, 5);
    clip_meta_t newer = record(3, false, 2);
    gh_set(&gh, note_path(first.id), NULL);
    run_queue();
    CHECK_INT(gh_count(&gh, "notes/"), 2);
    char last[CLIP_ID_LEN];
    CHECK(clip_last_note(&store, last));
    CHECK_STR(last, newer.id);
}

static void a_note_removed_while_gemini_answered_becomes_a_new_note(void)
{
    world();
    clip_meta_t first = first_note_uploaded();
    clip_meta_t add = record(2, true, 5);
    gh.edit_at = gh.requests + 1; // after the GET, before the PUT
    gh.edit_path = note_path(first.id);
    gh.edit_text = NULL;
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    CHECK_INT(gm.calls, 2); // the answer it has is used, not asked for again
    CHECK_INT(gh_count(&gh, "notes/"), 1);
    cap_note_t note = note_on_github(add.id);
    CHECK_STR(note.title, "Pool gear");
    CHECK_STR(note.transcript, "And a long pole.");
    CHECK_INT(note.num, 2);
    cap_note_free(&note);
}

static void a_note_changed_meanwhile_is_merged(void)
{
    world();
    clip_meta_t first = first_note_uploaded();
    clip_meta_t add = record(2, true, 5);
    // the phone adds to the same note between this device's GET and PUT
    cap_note_t theirs = note_on_github(first.id);
    cap_answer_t typed = {.title = "", .summary = "", .transcript = "Typed on the phone."};
    CHECK_INT(cap_note_add_answer(&theirs, "99999999999999999999999999999999", T0 / 1000 + 5, 0, &typed), CAP_ADDED);
    char *text = cap_note_render(first.id, &theirs);
    cap_note_free(&theirs);
    gh.edit_at = gh.requests + 1;
    gh.edit_path = note_path(first.id);
    gh.edit_text = text;
    clip_queue_status_t after;
    int before = gh.requests;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    char expected[300];
    snprintf(expected, sizeof expected, "GET notes/%s.md\nPUT notes/%s.md\nGET notes/%s.md\nPUT notes/%s.md\n", first.id,
             first.id, first.id, first.id);
    CHECK_STR(log_from(before), expected);
    cap_note_t note = note_on_github(first.id);
    CHECK_INT(note.addition_count, 2);
    if (note.addition_count == 2) {
        CHECK_STR(note.additions[0].text, "Typed on the phone.");
        CHECK_STR(note.additions[1].id, add.addition_id);
    }
    CHECK_STR(note.title, "Pool gear");
    cap_note_free(&note);
    free(text);
}

static void nothing_heard(void)
{
    // a new note: a "Nothing heard" note, as on the phone
    world();
    gemini_answers(&gm, 200, fake_interaction(ANSWER_SILENT, "completed"));
    clip_meta_t meta = record(1, false, 83);
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    cap_note_t note = note_on_github(meta.id);
    CHECK_STR(note.title, CAP_NOTHING_HEARD);
    CHECK_STR(note.summary, "No clear speech in this 1:23 recording.");
    CHECK_INT(note.num, 1);
    cap_note_free(&note);

    // an addition: nothing is added, and the recording is dropped
    later(10000);
    record(2, true, 4);
    char *before = strdup(gh_find(&gh, note_path(meta.id))->text);
    int requests = gh.requests;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_NOTHING_ADDED);
    CHECK_INT(after.queued, 0);
    CHECK_INT(gh.requests, requests + 1); // only the GET
    CHECK_STR(gh_find(&gh, note_path(meta.id))->text, before);
    CHECK_INT(fake_count(&fs), 1);
    free(before);
}

static void oldest_first_and_an_addition_after_its_note(void)
{
    // recorded out of range: a note, a hold that adds to it, and another note
    world();
    clip_meta_t a = record(1, false, 3), b = record(2, true, 2), c = record(3, false, 4);
    CHECK_STR(b.target, a.id);
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    CHECK_INT(after.queued, 2);
    CHECK(!after.due);
    CHECK_INT(after.wait_ms, CAP_GEMINI_MIN_INTERVAL_MS); // one Gemini request every 5 s
    CHECK(gh_find(&gh, note_path(a.id)) != NULL);

    // before the gate opens nothing is sent
    int calls = gm.calls, requests = gh.requests;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
    CHECK_INT(gm.calls, calls);
    CHECK_INT(gh.requests, requests);

    later(after.wait_ms);
    gemini_answers(&gm, 200, fake_interaction(ANSWER_ADD, "completed"));
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    later(after.wait_ms);
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    CHECK_INT(after.queued, 0);

    CHECK_INT(gh_count(&gh, "notes/"), 2);
    cap_note_t first = note_on_github(a.id), second = note_on_github(c.id);
    CHECK_INT(first.num, 1);
    CHECK_INT(first.addition_count, 1);
    CHECK_INT(second.num, 2);
    CHECK_INT(second.addition_count, 0);
    cap_note_free(&first);
    cap_note_free(&second);
    CHECK_STR(gh_find(&gh, "next-number")->text, "3\n");
}

static void a_cut_short_recording_sends_its_complete_pages(void)
{
    world();
    clip_meta_t meta;
    char name[CLIP_NAME_LEN];
    size_t len;
    uint8_t *ogg = fake_ogg(6, 0, &len);
    clip_queue_begin(&store, fake_random(1), &clk, &meta);
    clip_audio_name(meta.id, name);
    fake_put(&fs, name, ogg, len - 700);
    free(ogg);
    clip_recovery_t recovery;
    CHECK(clip_queue_recover(&store, &recovery));
    CHECK_INT(recovery.queued, 1);
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    CHECK_INT(gm.audio_bytes, OGG_HEADERS + 5 * OGG_AUDIO_PAGE);
    cap_note_t note = note_on_github(meta.id);
    CHECK_INT(note.duration_ms, 5000);
    cap_note_free(&note);
}

static void a_recording_made_before_the_clock_was_set_is_dated(void)
{
    // in the same power-on: the time since the recording is known
    world();
    clk.wall_ms = 0;
    clip_meta_t meta = record(1, false, 3);
    CHECK_INT(meta.created_s, 0);
    clk.wall_ms = T0;
    clk.uptime_ms += 100000; // Wi-Fi set the clock 100 s later
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    cap_note_t note = note_on_github(meta.id);
    CHECK_INT(note.created_s, T0 / 1000 - 100);
    cap_note_free(&note);

    // after a power loss it is not: dated at the upload, and the same on every later try
    world();
    clk.wall_ms = 0;
    meta = record(1, false, 3);
    clk = (clip_clock_t){.wall_ms = T0, .boot = 78, .uptime_ms = 4000};
    gh.fail_at = 0;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
    later(after.wait_ms);
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    note = note_on_github(meta.id);
    CHECK_INT(note.created_s, T0 / 1000);
    cap_note_free(&note);
}

// ---- Gemini's failures ---------------------------------------------------------------------------

static void a_gemini_error_backs_off_and_tries_again(void)
{
    world();
    clip_meta_t meta = record(1, false, 3), back;
    gemini_answers(&gm, 503, strdup("{\"error\":{\"message\":\"overloaded\"}}"));
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
    CHECK_INT(after.queued, 1);
    CHECK_INT(after.wait_ms, 30000);
    CHECK_INT(gh.requests, 0);
    CHECK(clip_meta_load(&store, meta.id, &back));
    CHECK_INT(back.attempts.attempts, 1);
    CHECK_STR(back.attempts.last_error, "HTTP 503: overloaded");
    CHECK_INT(back.not_before_ms, T0 + 30000);

    // too early: nothing is sent
    later(29999);
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
    CHECK_INT(gm.calls, 1);
    CHECK_INT(after.wait_ms, 1);
    later(1);
    gemini_answers(&gm, 200, fake_interaction(ANSWER, "completed"));
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    CHECK_INT(gm.calls, 2);
    CHECK_INT(gh_count(&gh, "notes/"), 1);
}

static void a_rate_limit_holds_the_gate_and_uses_no_attempt(void)
{
    world();
    clip_meta_t meta = record(1, false, 3), back;
    record(2, false, 3);
    gemini_answers(&gm, 429, strdup("{\"error\":{\"message\":\"limit: 15 requests per minute\"}}"));
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
    CHECK(clip_meta_load(&store, meta.id, &back));
    CHECK_INT(back.attempts.attempts, 0);
    // both recordings wait the full minute
    CHECK_INT(after.queued, 2);
    CHECK(!after.due);
    CHECK_INT(after.wait_ms, 60000);
    gemini_answers(&gm, 200, fake_interaction(ANSWER, "completed"));
    CHECK_INT(run_queue(), 2 + 1); // the first try after.. the wait, then the two uploads 5 s apart
    CHECK_INT(gh_count(&gh, "notes/"), 2);
}

static void a_terminal_gemini_error_gives_up_and_keeps_the_audio(void)
{
    world();
    clip_meta_t meta = record(1, false, 3), back;
    gemini_answers(&gm, 400, strdup("{\"error\":{\"message\":\"bad audio\"}}"));
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_GAVE_UP);
    CHECK_INT(after.queued, 0);
    CHECK_INT(after.failed, 1);
    CHECK_INT(after.wait_ms, -1);
    CHECK(clip_meta_load(&store, meta.id, &back));
    CHECK_INT(back.state, CLIP_REC_FAILED);
    char name[CLIP_NAME_LEN];
    clip_audio_name(meta.id, name);
    CHECK(fake_size(&fs, name) > 0);
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER); // not tried again by itself
    CHECK_INT(gm.calls, 1);

    // asked for over USB, it is tried again from the start
    CHECK_INT(clip_queue_retry_failed(&store), 1);
    later(10000);
    gemini_answers(&gm, 200, fake_interaction(ANSWER, "completed"));
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    CHECK_INT(gh_count(&gh, "notes/"), 1);
}

static void eight_failed_attempts_give_up(void)
{
    world();
    record(1, false, 3);
    gemini_answers(&gm, 500, strdup("{}"));
    clip_queue_status_t after;
    for (int attempt = 1; attempt < CAP_GEMINI_MAX_ATTEMPTS; attempt++) {
        CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
        CHECK_INT(after.wait_ms, cap_backoff_ms(attempt));
        later(after.wait_ms);
    }
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_GAVE_UP);
    CHECK_INT(gm.calls, CAP_GEMINI_MAX_ATTEMPTS);
    CHECK_INT(after.failed, 1);
}

static void the_same_bad_status_twice_gives_up(void)
{
    world();
    record(1, false, 3);
    gemini_answers(&gm, 200, fake_interaction("", "failed"));
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
    later(after.wait_ms);
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_GAVE_UP);
}

static void a_network_error_at_gemini_uses_no_attempt(void)
{
    world();
    clip_meta_t meta = record(1, false, 3), back;
    gm.offline = true;
    clip_queue_status_t after;
    for (int i = 0; i < 20; i++) {
        CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_OFFLINE);
        CHECK_INT(after.queued, 1);
        later(after.wait_ms);
    }
    CHECK(clip_meta_load(&store, meta.id, &back));
    CHECK_INT(back.attempts.attempts, 0);
    CHECK_INT(back.state, CLIP_REC_QUEUED);
    gm.offline = false;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
}

static void a_recording_whose_audio_is_missing_is_dropped(void)
{
    world();
    clip_meta_t meta = record(1, false, 3);
    char name[CLIP_NAME_LEN];
    clip_audio_name(meta.id, name);
    fake_delete(&fs, name);
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_GAVE_UP);
    CHECK_INT(after.queued, 0);
    CHECK_INT(after.failed, 0);
    CHECK_INT(gm.calls, 0);
}

// ---- GitHub's failures ---------------------------------------------------------------------------

static void gemini_is_not_asked_twice_when_github_fails(void)
{
    world();
    clip_meta_t meta = record(1, false, 3), back;
    gh.fail_at = 0; // the counter cannot be read
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
    CHECK_INT(after.wait_ms, 30000);
    CHECK(clip_meta_load(&store, meta.id, &back));
    CHECK_INT(back.sync_tries, 1);
    CHECK_INT(back.attempts.attempts, 0); // not one of Gemini's eight
    CHECK_STR(back.attempts.last_error, "Network error");
    cap_answer_t kept;
    CHECK(clip_answer_load(&store, meta.id, &kept));
    CHECK_STR(kept.title, "Pool skimmer");
    cap_answer_free(&kept);

    later(after.wait_ms);
    gemini_answers(&gm, 500, strdup("{}")); // would fail if it were asked
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    CHECK_INT(gm.calls, 1);
    CHECK_INT(gh_count(&gh, "notes/"), 1);
}

static void a_taken_number_is_kept_for_the_next_try(void)
{
    world();
    clip_meta_t meta = record(1, false, 3), back;
    gh.fail_at = 2; // the note's PUT
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
    CHECK(clip_meta_load(&store, meta.id, &back));
    CHECK_INT(back.num, 1);
    later(after.wait_ms);
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    char expected[200];
    snprintf(expected, sizeof expected, "PUT notes/%s.md\n", meta.id);
    CHECK_STR(log_from(3), expected); // the counter is left alone
    CHECK_STR(gh_find(&gh, "next-number")->text, "2\n");
    cap_note_t note = note_on_github(meta.id);
    CHECK_INT(note.num, 1);
    cap_note_free(&note);
}

static void github_refusing_keeps_the_note_for_much_later(void)
{
    world();
    clip_meta_t meta = record(1, false, 3), back;
    gh.status_at = 0;
    gh.status = 401;
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_STUCK);
    CHECK_INT(after.queued, 1);
    CHECK_INT(after.failed, 0);
    CHECK_INT(after.wait_ms, 5 * 60 * 60 * 1000);
    CHECK(clip_meta_load(&store, meta.id, &back));
    CHECK_INT(back.state, CLIP_REC_QUEUED);
    // the token is replaced over USB: it goes through
    later(after.wait_ms);
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    CHECK_INT(gm.calls, 1);
}

static void github_failing_again_and_again_waits_longer_and_never_gives_up(void)
{
    world();
    record(1, false, 3);
    gh.status_at = 0;
    gh.status = 503;
    gh.status_always = true;
    clip_queue_status_t after;
    for (int i = 1; i <= 20; i++) {
        CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
        CHECK_INT(after.queued, 1);
        CHECK_INT(after.wait_ms, cap_backoff_ms(i));
        later(after.wait_ms);
    }
    gh.status_at = -1;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    CHECK_INT(gm.calls, 1);
}

static void a_dead_file_system_is_not_retried_at_once(void)
{
    world();
    record(1, false, 3);
    fs.cut_at = fs.ops; // the first write fails, and everything after it
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_STUCK);
    CHECK(!after.due);
    CHECK(after.wait_ms >= 5 * 60 * 1000);
    CHECK_INT(after.queued, 1);
    fake_store_revive(&fs);
}

// ---- the status file -----------------------------------------------------------------------------

static bool status_now(void *ctx, clip_status_t *status)
{
    (void)ctx;
    *status = (clip_status_t){
        .cell_mv = 3800, .usb = false, .charging = false, .firmware = "0.3.1+g12ab\"}", .time_s = clk.wall_ms / 1000};
    return true;
}

static void the_status_file_is_written_after_a_saved_note(void)
{
    world();
    up.status = status_now;
    clip_meta_t first = record(1, false, 3);
    record(2, false, 3);
    clip_queue_status_t after;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    char expected[300];
    snprintf(expected, sizeof expected, NEW_NOTE_LOG "GET devices/clip.json\nPUT devices/clip.json\n", first.id);
    CHECK_STR(gh.log, expected);
    CHECK_STR(gh_find(&gh, CLIP_STATUS_PATH)->text,
              "{\n  \"device\": \"clip\",\n  \"time\": \"2026-09-19T07:42:13Z\",\n  \"battery_percent\": 45,\n"
              "  \"battery_mv\": 3800,\n  \"usb\": false,\n  \"charging\": false,\n  \"queued\": 1,\n"
              "  \"firmware\": \"0.3.1+g12ab\"\n}\n");
    cap_json_t root;
    CHECK(cap_json_parse(gh_find(&gh, CLIP_STATUS_PATH)->text, &root));

    // the next one replaces it
    later(after.wait_ms);
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
    CHECK(strstr(gh_find(&gh, CLIP_STATUS_PATH)->text, "\"queued\": 0,") != NULL);
    CHECK(strstr(gh_find(&gh, CLIP_STATUS_PATH)->text, "T07:42:18Z") != NULL);
    CHECK_INT(gh_count(&gh, "devices/"), 1);

    // not after anything else
    record(3, false, 3);
    later(10000);
    gemini_answers(&gm, 503, strdup("{}"));
    int requests = gh.requests;
    CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_LATER);
    CHECK_INT(gh.requests, requests);
}

static void a_status_file_that_cannot_be_written_changes_nothing(void)
{
    // the GET fails, the PUT fails, GitHub refuses either, the app has no status to give
    for (int way = 0; way < 5; way++) {
        world();
        up.status = status_now;
        clip_meta_t meta = record(1, false, 3);
        gh.fail_at = way == 0 ? 3 : way == 1 ? 4 : -1;
        gh.status_at = way == 2 ? 3 : way == 3 ? 4 : -1;
        gh.status = 500;
        up.status_ctx = NULL;
        if (way == 4) {
            up.status = NULL;
        }
        clip_queue_status_t after;
        CHECK_INT(clip_upload_next(&up, &after), CLIP_UP_SAVED);
        CHECK_INT(after.queued, 0);
        CHECK(gh_find(&gh, note_path(meta.id)) != NULL);
        CHECK(gh_find(&gh, CLIP_STATUS_PATH) == NULL);
        CHECK_INT(fake_count(&fs), 1);
    }
    // and reported on its own, the failure is told
    world();
    clip_status_t status;
    status_now(NULL, &status);
    gh.fail_at = 0;
    CHECK_INT(clip_status_report(&github, &status).status, CAP_RETRYABLE);
    gh.status_at = 1;
    gh.status = 401;
    CHECK_INT(clip_status_report(&github, &status).status, CAP_TERMINAL);
    CHECK_INT(clip_status_report(&github, &status).status, CAP_OK);
    CHECK_INT(clip_status_report(&github, &status).status, CAP_OK);
    CHECK_INT(gh_count(&gh, "devices/"), 1);
}

// ---- cut at every step ---------------------------------------------------------------------------

typedef enum {
    SCENE_NEW,           // a press
    SCENE_ADDITION,      // a hold, its note on GitHub
    SCENE_GONE,          // a hold, its note removed from GitHub
    SCENE_BOTH_QUEUED,   // a press and a hold, both recorded out of range
    SCENE_COUNT,
} scene_t;

static clip_meta_t scene_first, scene_add;

static void scene_set(scene_t scene)
{
    world();
    if (scene == SCENE_NEW) {
        scene_first = record(1, false, 3);
        return;
    }
    if (scene == SCENE_BOTH_QUEUED) {
        scene_first = record(1, false, 3);
    } else {
        scene_first = first_note_uploaded();
    }
    scene_add = record(2, true, 5);
    if (scene == SCENE_GONE) {
        gh_set(&gh, note_path(scene_first.id), NULL);
    }
}

// Whatever happened on the way, the repo has the one note, with the one addition.
static void scene_check(scene_t scene)
{
    const char *id = scene == SCENE_GONE ? scene_add.id : scene_first.id;
    size_t additions = scene == SCENE_ADDITION || scene == SCENE_BOTH_QUEUED ? 1 : 0;
    CHECK_INT(gh_count(&gh, "notes/"), 1);
    cap_note_t note = note_on_github(id);
    CHECK(note.num >= 1);
    CHECK(note.title && *note.title);
    CHECK_INT(note.addition_count, additions);
    if (additions && note.addition_count == 1) {
        CHECK_STR(note.additions[0].id, scene_add.addition_id);
    }
    // the counter is past every number given out
    long long next = 0;
    gh_file_t *counter = gh_find(&gh, "next-number");
    CHECK(counter && cap_parse_long(counter->text, strlen(counter->text) - 1, &next) && next > note.num);
    cap_note_free(&note);
    // flash holds nothing but the last note, which is that one
    char last[CLIP_ID_LEN];
    CHECK_INT(fake_count(&fs), 1);
    CHECK(clip_last_note(&store, last));
    CHECK_STR(last, id);
}

// The file system stops at its n-th operation, for every n. With `power`, the device is off from
// then on; without, only flash is dead and the requests of that try still go out.
static void stopped_at_every_file_operation(bool power)
{
    for (int scene = 0; scene < SCENE_COUNT; scene++) {
        int cuts = 0;
        for (int cut = 0;; cut++) {
            scene_set((scene_t)scene);
            power_model = power;
            fs.cut_at = fs.ops + cut;
            clip_queue_status_t after = {.queued = 1};
            for (int tries = 0; after.queued > 0 && !fs.dead && tries < 10; tries++) {
                clip_upload_next(&up, &after);
                later(after.wait_ms > 0 ? after.wait_ms : 0);
            }
            bool was_cut = fs.dead;
            // the next boot
            fake_store_revive(&fs);
            clip_recovery_t recovery;
            CHECK(clip_queue_recover(&store, &recovery));
            later(5000);
            if (was_cut) {
                cuts++;
                clip_queue_status_t status;
                CHECK(clip_queue_status(&store, &gate, clk.wall_ms, &status));
                if (status.queued > 0) {
                    run_queue();
                }
            }
            scene_check((scene_t)scene);
            if (unit_test_failed) {
                fprintf(stderr, "scene %d, cut at file operation %d, power %d\n", scene, cut, power);
                return;
            }
            if (!was_cut) {
                break;
            }
        }
        CHECK(cuts >= 5); // .ans, the number, the three removals at least
    }
}

static void power_lost_at_every_file_operation(void)
{
    stopped_at_every_file_operation(true);
}

static void flash_dead_at_every_file_operation(void)
{
    stopped_at_every_file_operation(false);
}

// Request n to GitHub fails, for every n: before it arrives, or with its answer lost.
static void network_lost_at_every_request(void)
{
    for (int scene = 0; scene < SCENE_COUNT; scene++) {
        for (int lose = 0; lose < 2; lose++) {
            for (int n = 0;; n++) {
                scene_set((scene_t)scene);
                int base = gh.requests;
                if (lose) {
                    gh.lose_at = base + n;
                } else {
                    gh.fail_at = base + n;
                }
                run_queue();
                scene_check((scene_t)scene);
                if (unit_test_failed) {
                    fprintf(stderr, "scene %d, request %d, answer lost %d\n%s", scene, n, lose, log_from(base));
                    return;
                }
                if (gh.requests - base <= n) {
                    CHECK(n >= 2);
                    break; // past the last request
                }
            }
        }
    }
}

static void free_everything(void)
{
    world();
    fake_store_free(&fs);
    gh_free(&gh);
    gemini_free(&gm);
}

int main(void)
{
    RUN(a_new_note_is_asked_numbered_and_created);
    RUN(an_addition_reads_the_note_asks_and_replaces_it);
    RUN(a_hold_whose_note_is_gone_becomes_a_new_note);
    RUN(a_gone_note_does_not_take_the_place_of_a_newer_last_note);
    RUN(a_note_removed_while_gemini_answered_becomes_a_new_note);
    RUN(a_note_changed_meanwhile_is_merged);
    RUN(nothing_heard);
    RUN(oldest_first_and_an_addition_after_its_note);
    RUN(a_cut_short_recording_sends_its_complete_pages);
    RUN(a_recording_made_before_the_clock_was_set_is_dated);
    RUN(a_gemini_error_backs_off_and_tries_again);
    RUN(a_rate_limit_holds_the_gate_and_uses_no_attempt);
    RUN(a_terminal_gemini_error_gives_up_and_keeps_the_audio);
    RUN(eight_failed_attempts_give_up);
    RUN(the_same_bad_status_twice_gives_up);
    RUN(a_network_error_at_gemini_uses_no_attempt);
    RUN(a_recording_whose_audio_is_missing_is_dropped);
    RUN(gemini_is_not_asked_twice_when_github_fails);
    RUN(a_taken_number_is_kept_for_the_next_try);
    RUN(github_refusing_keeps_the_note_for_much_later);
    RUN(github_failing_again_and_again_waits_longer_and_never_gives_up);
    RUN(a_dead_file_system_is_not_retried_at_once);
    RUN(the_status_file_is_written_after_a_saved_note);
    RUN(a_status_file_that_cannot_be_written_changes_nothing);
    RUN(power_lost_at_every_file_operation);
    RUN(flash_dead_at_every_file_operation);
    RUN(network_lost_at_every_request);
    free_everything();
    return unit_done(__FILE__);
}
