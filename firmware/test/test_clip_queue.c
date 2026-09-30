// SOURCES: firmware/components/clip/clip_queue.c firmware/components/capture/cap_text.c firmware/components/capture/cap_time.c firmware/components/capture/cap_note.c
// The queue on flash: the files' formats, what a power cut at any moment leaves behind and how
// the next boot puts it right, and which recording is tried next.
#include "test_clip_fake.h"

static fake_store_t fs;
static clip_store_t store;
static const clip_clock_t clock_set = {.wall_ms = 1789803733000, .boot = 77, .uptime_ms = 5000};

static void fresh(void)
{
    fake_store_free(&fs);
    store = fake_store(&fs);
}

static uint32_t crc(const char *data, size_t len)
{
    uint32_t value = 0xFFFFFFFFu;
    for (size_t i = 0; i < len; i++) {
        value ^= (uint8_t)data[i];
        for (int bit = 0; bit < 8; bit++) {
            value = (value >> 1) ^ (0xEDB88320u & (0u - (value & 1)));
        }
    }
    return ~value;
}

static void put_audio(const char *id, int seconds)
{
    char name[CLIP_NAME_LEN];
    size_t len;
    uint8_t *ogg = fake_ogg(seconds, 0, &len);
    clip_audio_name(id, name);
    fake_put(&fs, name, ogg, len);
    free(ogg);
}

// A finished recording in the queue.
static clip_meta_t recorded(uint8_t seed, bool hold)
{
    clip_meta_t meta;
    CHECK(clip_queue_begin(&store, fake_random(seed), &clock_set, &meta));
    put_audio(meta.id, 3);
    if (hold) {
        clip_queue_mark_addition(&store, &meta);
    }
    CHECK(clip_queue_finish(&store, &meta, 3000));
    return meta;
}

static clip_meta_t full_meta(void)
{
    clip_meta_t meta = {
        .id = "00112233445566778899aabbccddeeff",
        .addition_id = "ffeeddccbbaa99887766554433221100",
        .kind = CLIP_ADDITION,
        .target = "0123456789abcdef0123456789abcdef",
        .state = CLIP_REC_QUEUED,
        .seq = 41,
        .created_s = 1789803733,
        .boot = 4000000000u,
        .uptime_ms = 123456789012,
        .duration_ms = 900000,
        .audio_bytes = 3672000,
        .num = 17,
        .not_before_ms = 1789803999000,
        .sync_tries = 3,
        .attempts = {.attempts = 2, .last_error = "HTTP 503: try later"},
    };
    return meta;
}

// ---- the formats ---------------------------------------------------------------------------------

static void meta_round_trip(void)
{
    clip_meta_t meta = full_meta(), back;
    char *text = clip_meta_render(&meta);
    CHECK(text && strncmp(text, "clip-meta 1\nid=00112233445566778899aabbccddeeff\n", 48) == 0);
    CHECK(clip_meta_parse(text, strlen(text), &back));
    CHECK_STR(back.id, meta.id);
    CHECK_STR(back.addition_id, meta.addition_id);
    CHECK_STR(back.target, meta.target);
    CHECK_INT(back.kind, CLIP_ADDITION);
    CHECK_INT(back.state, CLIP_REC_QUEUED);
    CHECK_INT(back.seq, 41);
    CHECK_INT(back.created_s, meta.created_s);
    CHECK_INT(back.boot, 4000000000u);
    CHECK_INT(back.uptime_ms, meta.uptime_ms);
    CHECK_INT(back.duration_ms, 900000);
    CHECK_INT(back.audio_bytes, 3672000);
    CHECK_INT(back.num, 17);
    CHECK_INT(back.not_before_ms, meta.not_before_ms);
    CHECK_INT(back.sync_tries, 3);
    CHECK_INT(back.attempts.attempts, 2);
    CHECK_STR(back.attempts.last_error, "HTTP 503: try later");
    free(text);

    // a new note, the other states, and an error with line breaks in it
    meta.kind = CLIP_NEW;
    meta.target[0] = 0;
    meta.state = CLIP_REC_FAILED;
    meta.audio_bytes = -1;
    snprintf(meta.attempts.last_error, sizeof meta.attempts.last_error, "two\nlines\r=here");
    text = clip_meta_render(&meta);
    CHECK(clip_meta_parse(text, strlen(text), &back));
    CHECK_INT(back.kind, CLIP_NEW);
    CHECK_STR(back.target, "");
    CHECK_INT(back.state, CLIP_REC_FAILED);
    CHECK_INT(back.audio_bytes, -1);
    CHECK_STR(back.attempts.last_error, "two lines =here");
    free(text);
    meta.state = CLIP_REC_RECORDING;
    text = clip_meta_render(&meta);
    CHECK(clip_meta_parse(text, strlen(text), &back));
    CHECK_INT(back.state, CLIP_REC_RECORDING);
    free(text);

    // the longest error there can be still fits
    memset(meta.attempts.last_error, 'x', sizeof meta.attempts.last_error - 1);
    text = clip_meta_render(&meta);
    CHECK(clip_meta_parse(text, strlen(text), &back));
    CHECK_INT(strlen(back.attempts.last_error), sizeof meta.attempts.last_error - 1);
    free(text);
}

static void a_cut_or_changed_meta_does_not_read(void)
{
    clip_meta_t meta = full_meta(), back;
    char *text = clip_meta_render(&meta);
    size_t len = strlen(text);
    for (size_t cut = 0; cut < len; cut++) {
        CHECK(!clip_meta_parse(text, cut, &back));
    }
    for (size_t at = 0; at < len; at++) {
        text[at] ^= 1;
        CHECK(!clip_meta_parse(text, len, &back));
        text[at] ^= 1;
    }
    CHECK(clip_meta_parse(text, len, &back));
    free(text);
}

// Text with a correct CRC line, to test what the parser does past the checksum.
static bool parses(const char *body)
{
    char text[600];
    clip_meta_t meta;
    int len = snprintf(text, sizeof text, "%s", body);
    snprintf(text + len, sizeof text - (size_t)len, "crc=%08lx\n", (unsigned long)crc(body, (size_t)len));
    return clip_meta_parse(text, strlen(text), &meta);
}

static void meta_rules(void)
{
    CHECK(parses("clip-meta 1\nid=00112233445566778899aabbccddeeff\n"));
    // a key from a newer firmware is skipped
    CHECK(parses("clip-meta 1\nid=00112233445566778899aabbccddeeff\ncolour=blue\n"));
    CHECK(!parses("clip-meta 2\nid=00112233445566778899aabbccddeeff\n"));
    CHECK(!parses("clip-meta 1\n"));
    CHECK(!parses("clip-meta 1\nid=0011\n"));
    CHECK(!parses("clip-meta 1\nid=00112233445566778899AABBCCDDEEFF\n"));
    CHECK(!parses("clip-meta 1\nid=00112233445566778899aabbccddeeff\nnonsense\n"));
    // an addition names its note and its own id
    CHECK(!parses("clip-meta 1\nid=00112233445566778899aabbccddeeff\nkind=add\n"));
    CHECK(!parses("clip-meta 1\nid=00112233445566778899aabbccddeeff\nkind=add\ntarget=0123456789abcdef0123456789abcdef\n"));
    CHECK(parses("clip-meta 1\nid=00112233445566778899aabbccddeeff\nkind=add\ntarget=0123456789abcdef0123456789abcdef\n"
                 "addition=ffeeddccbbaa99887766554433221100\n"));
}

static void answer_round_trip(void)
{
    const cap_answer_t answers[] = {
        {.title = "Pool skimmer", .summary = "Buy a skimmer.\n\n- one\n- two", .transcript = "We need a skimmer\nfor the pool."},
        {.title = "", .summary = "", .transcript = ""},
        {.title = "\xd7\xa9\xd7\x9c\xd7\x95\xd7\x9d", .summary = "crc=00000000\n", .transcript = "12\nab\n"},
    };
    for (size_t i = 0; i < sizeof answers / sizeof *answers; i++) {
        size_t len = 0;
        cap_answer_t back;
        char *text = clip_answer_render(&answers[i], &len);
        CHECK(text && len == strlen(text));
        CHECK(clip_answer_parse(text, len, &back));
        CHECK_STR(back.title, answers[i].title);
        CHECK_STR(back.summary, answers[i].summary);
        CHECK_STR(back.transcript, answers[i].transcript);
        cap_answer_free(&back);
        for (size_t cut = 0; cut < len; cut++) {
            CHECK(!clip_answer_parse(text, cut, &back));
        }
        for (size_t at = 0; at < len; at++) {
            text[at] ^= 4;
            CHECK(!clip_answer_parse(text, len, &back));
            text[at] ^= 4;
        }
        free(text);
    }
}

// ---- the audio -----------------------------------------------------------------------------------

static void ogg_scan_reads_whole_pages(void)
{
    fresh();
    int64_t bytes, ms;
    size_t len;
    uint8_t *ogg = fake_ogg(5, 0, &len);
    CHECK_INT(len, OGG_HEADERS + 5 * OGG_AUDIO_PAGE);
    fake_put(&fs, "a.ogg", ogg, len);
    CHECK(clip_ogg_scan(&store, "a.ogg", &bytes, &ms));
    CHECK_INT(bytes, len);
    CHECK_INT(ms, 5000);
    free(ogg);

    // the pre-skip is not audio
    ogg = fake_ogg(2, 312, &len);
    fake_put(&fs, "a.ogg", ogg, len);
    CHECK(clip_ogg_scan(&store, "a.ogg", &bytes, &ms));
    CHECK_INT(ms, (96000 - 312) / 48);
    free(ogg);

    CHECK(!clip_ogg_scan(&store, "absent.ogg", &bytes, &ms));
    fake_put(&fs, "empty.ogg", "", 0);
    CHECK(!clip_ogg_scan(&store, "empty.ogg", &bytes, &ms));
    char junk[500];
    memset(junk, 'j', sizeof junk);
    fake_put(&fs, "junk.ogg", junk, sizeof junk);
    CHECK(!clip_ogg_scan(&store, "junk.ogg", &bytes, &ms));
    fresh();
}

static void ogg_cut_at_any_byte_keeps_its_complete_pages(void)
{
    fresh();
    size_t len;
    uint8_t *ogg = fake_ogg(3, 0, &len);
    for (size_t cut = 0; cut <= len; cut += cut < 200 ? 1 : 37) {
        int64_t bytes = -1, ms = -1;
        fake_put(&fs, "a.ogg", ogg, cut);
        int pages = cut < OGG_HEADERS ? 0 : (int)((cut - OGG_HEADERS) / OGG_AUDIO_PAGE);
        CHECK_INT(clip_ogg_scan(&store, "a.ogg", &bytes, &ms), pages > 0);
        CHECK_INT(bytes, pages ? OGG_HEADERS + pages * OGG_AUDIO_PAGE : 0);
        CHECK_INT(ms, pages * 1000);
    }
    // garbage after the last good page (a sector that was erased, not written) is left out too
    memset(ogg + OGG_HEADERS + 2 * OGG_AUDIO_PAGE, 0xff, OGG_AUDIO_PAGE);
    fake_put(&fs, "a.ogg", ogg, len);
    int64_t bytes, ms;
    CHECK(clip_ogg_scan(&store, "a.ogg", &bytes, &ms));
    CHECK_INT(bytes, OGG_HEADERS + 2 * OGG_AUDIO_PAGE);
    CHECK_INT(ms, 2000);
    free(ogg);
    fresh();
}

// ---- recording -----------------------------------------------------------------------------------

static void a_recording_is_written_down_before_its_audio(void)
{
    fresh();
    clip_meta_t meta, back;
    CHECK(clip_queue_begin(&store, fake_random(1), &clock_set, &meta));
    CHECK_INT(strlen(meta.id), 32);
    CHECK_INT(strlen(meta.addition_id), 32);
    CHECK(strcmp(meta.id, meta.addition_id) != 0);
    CHECK_INT(meta.kind, CLIP_NEW);
    CHECK_INT(meta.state, CLIP_REC_RECORDING);
    CHECK_INT(meta.seq, 1);
    CHECK_INT(meta.created_s, 1789803733);
    CHECK_INT(meta.boot, 77);
    CHECK_INT(meta.uptime_ms, 5000);
    CHECK(clip_meta_load(&store, meta.id, &back));
    CHECK_INT(back.state, CLIP_REC_RECORDING);
    CHECK_INT(fake_count(&fs), 1);

    // not counted as queued while it is being recorded (an upload may run meanwhile)
    clip_queue_status_t status;
    CHECK(clip_queue_status(&store, NULL, 0, &status));
    CHECK_INT(status.queued, 0);
    CHECK_INT(status.wait_ms, -1);

    put_audio(meta.id, 4);
    CHECK(clip_queue_finish(&store, &meta, 4000));
    CHECK(clip_meta_load(&store, meta.id, &back));
    CHECK_INT(back.state, CLIP_REC_QUEUED);
    CHECK_INT(back.duration_ms, 4000);
    CHECK_INT(back.audio_bytes, -1);
    CHECK(clip_queue_status(&store, NULL, 0, &status));
    CHECK_INT(status.queued, 1);
    CHECK(status.due);
    CHECK_STR(status.id, meta.id);

    // with the clock not set, the time is left open
    clip_clock_t unset = {.boot = 78, .uptime_ms = 900};
    clip_meta_t second;
    CHECK(clip_queue_begin(&store, fake_random(2), &unset, &second));
    CHECK_INT(second.created_s, 0);
    CHECK_INT(second.seq, 2);
    CHECK(strcmp(second.id, meta.id) != 0);
    fresh();
}

static void a_hold_adds_to_the_last_new_note(void)
{
    fresh();
    clip_meta_t first, second, third;
    char last[CLIP_ID_LEN];
    CHECK(!clip_last_note(&store, last));

    // no note yet: a hold stays a new note
    clip_queue_begin(&store, fake_random(1), &clock_set, &first);
    CHECK(!clip_queue_mark_addition(&store, &first));
    CHECK_INT(first.kind, CLIP_NEW);
    put_audio(first.id, 2);
    CHECK(clip_queue_finish(&store, &first, 2000));
    CHECK(clip_last_note(&store, last));
    CHECK_STR(last, first.id);

    // the next hold adds to it, uploaded or not, and an addition is not itself the last note
    clip_queue_begin(&store, fake_random(2), &clock_set, &second);
    CHECK(clip_queue_mark_addition(&store, &second));
    CHECK_INT(second.kind, CLIP_ADDITION);
    CHECK_STR(second.target, first.id);
    clip_meta_t back;
    CHECK(clip_meta_load(&store, second.id, &back)); // on flash at once, in case power is lost
    CHECK_INT(back.kind, CLIP_ADDITION);
    CHECK_INT(back.state, CLIP_REC_RECORDING);
    put_audio(second.id, 2);
    CHECK(clip_queue_finish(&store, &second, 2000));
    CHECK(clip_last_note(&store, last));
    CHECK_STR(last, first.id);

    third = recorded(3, false);
    CHECK(clip_last_note(&store, last));
    CHECK_STR(last, third.id);

    // a damaged file is no last note
    fake_put(&fs, "last", "nonsense", 8);
    CHECK(!clip_last_note(&store, last));
    CHECK(!clip_set_last_note(&store, "short"));
    fresh();
}

// ---- power cuts ----------------------------------------------------------------------------------

static void a_cut_while_saving_leaves_the_old_file(void)
{
    // cut 0 falls in the write of the new file (half of it lands), cut 1 before the rename
    for (int cut = 0; cut < 2; cut++) {
        fresh();
        clip_meta_t meta = recorded(1, false), back;
        meta.num = 9;
        fs.cut_at = fs.ops + cut;
        CHECK(!clip_meta_save(&store, &meta));
        fake_store_revive(&fs);
        CHECK(clip_meta_load(&store, meta.id, &back));
        CHECK_INT(back.num, 0);
        char fresh_name[64];
        snprintf(fresh_name, sizeof fresh_name, "%s.meta.new", meta.id);
        CHECK(fake_find(&fs, fresh_name) != NULL);
        clip_recovery_t recovery;
        CHECK(clip_queue_recover(&store, &recovery));
        CHECK(fake_find(&fs, fresh_name) == NULL);
        CHECK(clip_meta_load(&store, meta.id, &back));
        CHECK_INT(back.num, 0);
        CHECK_INT(recovery.queued + recovery.dropped + recovery.rebuilt, 0);
    }
    fresh();
}

static void a_recording_cut_by_a_power_loss_is_queued(void)
{
    fresh();
    clip_meta_t first = recorded(1, false), meta, back;
    clip_queue_begin(&store, fake_random(2), &clock_set, &meta);
    clip_queue_mark_addition(&store, &meta);
    // 7 whole pages and a bit of the 8th reached flash
    size_t len;
    uint8_t *ogg = fake_ogg(8, 0, &len);
    char name[CLIP_NAME_LEN];
    clip_audio_name(meta.id, name);
    fake_put(&fs, name, ogg, len - 1000);
    free(ogg);

    clip_recovery_t recovery;
    CHECK(clip_queue_recover(&store, &recovery));
    CHECK_INT(recovery.queued, 1);
    CHECK_INT(recovery.dropped, 0);
    CHECK(clip_meta_load(&store, meta.id, &back));
    CHECK_INT(back.state, CLIP_REC_QUEUED);
    CHECK_INT(back.kind, CLIP_ADDITION); // still what the hold made it
    CHECK_STR(back.target, first.id);
    CHECK_INT(back.duration_ms, 7000);
    CHECK_INT(back.audio_bytes, OGG_HEADERS + 7 * OGG_AUDIO_PAGE);
    CHECK_INT(fake_size(&fs, name), (int64_t)len - 1000); // the file itself is left as it is

    // a second boot changes nothing
    CHECK(clip_queue_recover(&store, &recovery));
    CHECK_INT(recovery.queued + recovery.dropped + recovery.rebuilt, 0);

    // a cut new note becomes the last note
    clip_queue_begin(&store, fake_random(3), &clock_set, &meta);
    put_audio(meta.id, 2);
    CHECK(clip_queue_recover(&store, &recovery));
    char last[CLIP_ID_LEN];
    CHECK(clip_last_note(&store, last));
    CHECK_STR(last, meta.id);
    fresh();
}

static void a_recording_with_no_audio_is_dropped(void)
{
    fresh();
    clip_meta_t meta;
    clip_recovery_t recovery;
    // power lost before the audio file was made
    clip_queue_begin(&store, fake_random(1), &clock_set, &meta);
    CHECK(clip_queue_recover(&store, &recovery));
    CHECK_INT(recovery.dropped, 1);
    CHECK_INT(fake_count(&fs), 0);
    // and with only the Ogg headers and half a page
    clip_queue_begin(&store, fake_random(2), &clock_set, &meta);
    size_t len;
    uint8_t *ogg = fake_ogg(1, 0, &len);
    char name[CLIP_NAME_LEN];
    clip_audio_name(meta.id, name);
    fake_put(&fs, name, ogg, len - 1);
    free(ogg);
    CHECK(clip_queue_recover(&store, &recovery));
    CHECK_INT(recovery.dropped, 1);
    CHECK_INT(fake_count(&fs), 0);
    fresh();
}

static void leftovers_are_removed_and_the_rest_is_kept(void)
{
    fresh();
    clip_meta_t queued = recorded(1, false), failed = recorded(2, false);
    failed.state = CLIP_REC_FAILED;
    clip_meta_save(&store, &failed);
    cap_answer_t answer = {.title = "t", .summary = "s", .transcript = "x"};
    CHECK(clip_answer_save(&store, queued.id, &answer));
    // an upload that was done: its .meta went first, then power was lost
    fake_put(&fs, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.ogg", "OggS", 4);
    fake_put(&fs, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.ans", "x", 1);
    fake_put(&fs, "last.new", "half", 4);
    fake_put(&fs, "notes.txt", "something else", 14);
    int before = fake_count(&fs);

    clip_recovery_t recovery;
    CHECK(clip_queue_recover(&store, &recovery));
    CHECK_INT(recovery.dropped, 2);
    CHECK_INT(recovery.queued, 0);
    CHECK_INT(fake_count(&fs), before - 3);
    clip_meta_t back;
    cap_answer_t kept;
    CHECK(clip_meta_load(&store, queued.id, &back) && back.state == CLIP_REC_QUEUED);
    CHECK(clip_meta_load(&store, failed.id, &back) && back.state == CLIP_REC_FAILED);
    CHECK(clip_answer_load(&store, queued.id, &kept));
    cap_answer_free(&kept);
    CHECK(fake_find(&fs, "notes.txt") != NULL);
    CHECK(fake_find(&fs, "last") != NULL);
    fresh();
}

static void an_unreadable_meta_is_made_again_from_its_audio(void)
{
    fresh();
    clip_meta_t meta = recorded(1, true), back; // no last note: a new note anyway
    char name[CLIP_NAME_LEN];
    snprintf(name, sizeof name, "%s.meta", meta.id);
    fake_put(&fs, name, "clip-meta 1\nid=", 15);
    clip_recovery_t recovery;
    CHECK(clip_queue_recover(&store, &recovery));
    CHECK_INT(recovery.rebuilt, 1);
    CHECK(clip_meta_load(&store, meta.id, &back));
    CHECK_INT(back.state, CLIP_REC_QUEUED);
    CHECK_INT(back.kind, CLIP_NEW);
    CHECK_INT(back.duration_ms, 3000);
    CHECK_INT(back.created_s, 0); // dated when it is uploaded

    // with no audio to go with it, it goes
    fresh();
    fake_put(&fs, "00112233445566778899aabbccddeeff.meta", "junk", 4);
    CHECK(clip_queue_recover(&store, &recovery));
    CHECK_INT(recovery.dropped, 1);
    CHECK_INT(fake_count(&fs), 0);
    fresh();
}

// A recording whose audio is `audio_at` file operations in: power is lost at operation `cut`.
// Afterwards the boot's recovery must leave a queue that reads, and never lose audio that the
// recorder had written.
static void power_lost_at_every_step_of_a_recording(void)
{
    for (int hold = 0; hold < 2; hold++) {
        for (int cut = 0;; cut++) {
            fresh();
            clip_meta_t first = recorded(1, false), meta;
            int start = fs.ops;
            fs.cut_at = start + cut;
            bool audio = false;
            if (clip_queue_begin(&store, fake_random(2), &clock_set, &meta)) {
                put_audio(meta.id, 2); // the recorder's own writes are not part of the cut
                audio = true;
                if (hold) {
                    clip_queue_mark_addition(&store, &meta);
                }
                clip_queue_finish(&store, &meta, 2000);
            }
            bool was_cut = fs.dead;
            fake_store_revive(&fs);
            clip_recovery_t recovery;
            CHECK(clip_queue_recover(&store, &recovery));
            clip_queue_status_t status;
            CHECK(clip_queue_status(&store, NULL, 0, &status));
            CHECK_INT(status.queued, audio ? 2 : 1);
            CHECK_INT(fake_count(&fs), audio ? 5 : 3); // per recording .meta and .ogg, and `last`
            clip_meta_t back;
            CHECK(clip_meta_load(&store, first.id, &back));
            if (audio) {
                CHECK(clip_meta_load(&store, meta.id, &back));
                CHECK_INT(back.state, CLIP_REC_QUEUED);
                CHECK_INT(back.duration_ms, 2000);
                if (back.kind == CLIP_ADDITION) {
                    CHECK_STR(back.target, first.id);
                }
            }
            char last[CLIP_ID_LEN];
            CHECK(clip_last_note(&store, last));
            CHECK_STR(last, audio && back.kind == CLIP_NEW ? meta.id : first.id);
            if (!was_cut) {
                CHECK(cut >= (hold ? 6 : 5)); // every operation had its turn
                break;
            }
        }
    }
    fresh();
}

// ---- which one is next ---------------------------------------------------------------------------

static void the_oldest_due_recording_is_next(void)
{
    fresh();
    clip_queue_status_t status;
    CHECK(clip_queue_status(&store, NULL, 1000, &status));
    CHECK_INT(status.queued, 0);
    CHECK(!status.due);
    CHECK_INT(status.wait_ms, -1);

    clip_meta_t a = recorded(1, false), b = recorded(2, false), c = recorded(3, false);
    CHECK(a.seq < b.seq && b.seq < c.seq);
    CHECK(clip_queue_status(&store, NULL, 1000, &status));
    CHECK_INT(status.queued, 3);
    CHECK_STR(status.id, a.id);
    CHECK_INT(status.wait_ms, 0);

    // one that is backing off does not hold up the ones behind it
    a.not_before_ms = 31000;
    clip_meta_save(&store, &a);
    CHECK(clip_queue_status(&store, NULL, 1000, &status));
    CHECK_STR(status.id, b.id);
    b.not_before_ms = 61000;
    c.not_before_ms = 121000;
    clip_meta_save(&store, &b);
    clip_meta_save(&store, &c);
    CHECK(clip_queue_status(&store, NULL, 1000, &status));
    CHECK(!status.due);
    CHECK_INT(status.queued, 3);
    CHECK_INT(status.wait_ms, 30000);
    CHECK(clip_queue_status(&store, NULL, 31000, &status));
    CHECK_STR(status.id, a.id);

    // a given-up one is kept and counted, not tried
    a.state = CLIP_REC_FAILED;
    clip_meta_save(&store, &a);
    CHECK(clip_queue_status(&store, NULL, 31000, &status));
    CHECK_INT(status.queued, 2);
    CHECK_INT(status.failed, 1);
    CHECK_INT(status.wait_ms, 30000);
    CHECK_INT(clip_queue_retry_failed(&store), 1);
    CHECK(clip_queue_status(&store, NULL, 31000, &status));
    CHECK_INT(status.queued, 3);
    CHECK_INT(status.failed, 0);
    CHECK_STR(status.id, a.id);

    clip_queue_drop(&store, a.id);
    clip_queue_drop(&store, b.id);
    clip_queue_drop(&store, c.id);
    CHECK_INT(fake_count(&fs), 1); // `last`
    fresh();
}

static void the_gemini_gate_holds_those_without_an_answer(void)
{
    fresh();
    clip_meta_t a = recorded(1, false), b = recorded(2, false);
    cap_gate_t gate = {.min_interval_ms = CAP_GEMINI_MIN_INTERVAL_MS, .next_allowed_ms = 9000};
    clip_queue_status_t status;
    CHECK(clip_queue_status(&store, &gate, 4000, &status));
    CHECK(!status.due);
    CHECK_INT(status.wait_ms, 5000);
    // b has its answer and only needs GitHub
    cap_answer_t answer = {.title = "t", .summary = "s", .transcript = "x"};
    clip_answer_save(&store, b.id, &answer);
    CHECK(clip_queue_status(&store, &gate, 4000, &status));
    CHECK(status.due);
    CHECK_STR(status.id, b.id);
    CHECK(clip_queue_status(&store, &gate, 9000, &status));
    CHECK_STR(status.id, a.id);
    fresh();
}

static void an_addition_waits_for_its_note(void)
{
    fresh();
    clip_meta_t note = recorded(1, false), addition = recorded(2, true);
    CHECK_INT(addition.kind, CLIP_ADDITION);
    note.not_before_ms = 60000;
    clip_meta_save(&store, &note);
    clip_queue_status_t status;
    CHECK(clip_queue_status(&store, NULL, 1000, &status));
    CHECK(!status.due); // the addition is due, but its note is not on GitHub yet
    CHECK_INT(status.queued, 2);
    CHECK_INT(status.wait_ms, 59000);
    CHECK(clip_queue_status(&store, NULL, 60000, &status));
    CHECK_STR(status.id, note.id);
    // the note is uploaded (its files go), or given up: the addition has its turn
    note.state = CLIP_REC_FAILED;
    clip_meta_save(&store, &note);
    CHECK(clip_queue_status(&store, NULL, 1000, &status));
    CHECK_STR(status.id, addition.id);
    clip_queue_drop(&store, note.id);
    CHECK(clip_queue_status(&store, NULL, 1000, &status));
    CHECK_STR(status.id, addition.id);
    fresh();
}

int main(void)
{
    RUN(meta_round_trip);
    RUN(a_cut_or_changed_meta_does_not_read);
    RUN(meta_rules);
    RUN(answer_round_trip);
    RUN(ogg_scan_reads_whole_pages);
    RUN(ogg_cut_at_any_byte_keeps_its_complete_pages);
    RUN(a_recording_is_written_down_before_its_audio);
    RUN(a_hold_adds_to_the_last_new_note);
    RUN(a_cut_while_saving_leaves_the_old_file);
    RUN(a_recording_cut_by_a_power_loss_is_queued);
    RUN(a_recording_with_no_audio_is_dropped);
    RUN(leftovers_are_removed_and_the_rest_is_kept);
    RUN(an_unreadable_meta_is_made_again_from_its_audio);
    RUN(power_lost_at_every_step_of_a_recording);
    RUN(the_oldest_due_recording_is_next);
    RUN(the_gemini_gate_holds_those_without_an_answer);
    RUN(an_addition_waits_for_its_note);
    return unit_done(__FILE__);
}
