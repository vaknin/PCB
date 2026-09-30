// SOURCES: firmware/components/clip/clip_status.c firmware/components/clip/clip_update.c firmware/components/clip/clip_power.c firmware/components/clip/clip_queue.c firmware/components/capture/cap_text.c firmware/components/capture/cap_time.c firmware/components/capture/cap_note.c
// When the status file is written (only when it is news), and the update manifest: reading it and
// deciding whether to install what it names.
#include "test_clip_fake.h"

#define T0 INT64_C(1789803733)
#define SHA "00112233445566778899aabbccddeeff00112233445566778899AABBCCDDEEFF"

static clip_status_t status(int cell_mv, const char *firmware, const char *error, int64_t time_s)
{
    return (clip_status_t){.cell_mv = cell_mv, .firmware = firmware, .error = error, .time_s = time_s};
}

static void the_first_report_is_always_due(void)
{
    clip_status_t now = status(3900, "1.0", "", T0);
    CHECK(clip_status_due(NULL, &now));
}

static void nothing_new_is_not_reported(void)
{
    clip_status_t first = status(3900, "1.0", "", T0), now = first;
    clip_status_mark_t mark;
    clip_status_mark(&first, &mark);
    CHECK(!clip_status_due(&mark, &now));
    // a note a minute for a day's use: the battery drifts a little, nothing is written
    now = status(3860, "1.0", NULL, T0 + 3600);
    CHECK(!clip_status_due(&mark, &now));
    now.queued = 3;
    now.usb = true;
    now.charging = true;
    CHECK(!clip_status_due(&mark, &now));
}

static void news_is_reported(void)
{
    clip_status_t first = status(3900, "1.0", "", T0), now;
    clip_status_mark_t mark;
    clip_status_mark(&first, &mark);
    now = status(3900, "1.1", "", T0 + 60);
    CHECK(clip_status_due(&mark, &now)); // new firmware
    now = status(3900, "1.0", "GitHub: Bad credentials", T0 + 60);
    CHECK(clip_status_due(&mark, &now)); // something went wrong
    clip_status_mark(&now, &mark);
    CHECK(!clip_status_due(&mark, &now)); // the same error again is not news
    now.error = "";
    CHECK(clip_status_due(&mark, &now)); // and it went away
    clip_status_mark(&first, &mark);
    now = status(3900, "1.0", "", T0 + CLIP_STATUS_MAX_AGE_S - 1);
    CHECK(!clip_status_due(&mark, &now));
    now.time_s++;
    CHECK(clip_status_due(&mark, &now)); // a day old
    now.time_s = T0 - 5;
    CHECK(clip_status_due(&mark, &now)); // the clock went backwards
}

static void the_battery_is_reported_in_steps(void)
{
    clip_status_t first = status(4000, "1.0", "", T0), now; // 78 %
    clip_status_mark_t mark;
    clip_status_mark(&first, &mark);
    CHECK_INT(mark.percent, 78);
    now = status(3880, "1.0", "", T0 + 60); // 59 %
    CHECK(!clip_status_due(&mark, &now));
    now.cell_mv = 3870; // 57 %... 58: still under the step
    CHECK_INT(clip_battery_percent(3875), 58);
    now.cell_mv = 3875;
    CHECK(clip_status_due(&mark, &now));
    // and upwards while charging
    first = status(3700, "1.0", "", T0);
    clip_status_mark(&first, &mark);
    now = status(3800, "1.0", "", T0 + 60);
    now.usb = true;
    CHECK(clip_status_due(&mark, &now));
    // going under the upload limit is news even when it is a small step
    first = status(3660, "1.0", "", T0);
    clip_status_mark(&first, &mark);
    now = status(3640, "1.0", "", T0 + 60);
    CHECK(clip_status_due(&mark, &now));
    now.usb = true; // on USB the limits do not apply
    CHECK(!clip_status_due(&mark, &now));
    // a whole discharge from full to the limit is a handful of reports
    int reports = 0;
    first = status(4200, "1.0", "", T0);
    clip_status_mark(&first, &mark);
    for (int mv = 4200; mv >= CLIP_RECORD_MIN_MV; mv--) {
        now = status(mv, "1.0", "", T0 + 60);
        if (clip_status_due(&mark, &now)) {
            reports++;
            clip_status_mark(&now, &mark);
        }
    }
    CHECK(reports >= 4 && reports <= 6);
}

static void the_mark_is_kept_on_flash(void)
{
    fake_store_t fs = {0};
    clip_store_t store = fake_store(&fs);
    clip_status_mark_t mark, read;
    CHECK(!clip_status_mark_load(&store, &read));
    clip_status_t now = status(3800, "b916044-dirty", "Gemini: quota\nused up", T0);
    clip_status_mark(&now, &mark);
    CHECK(clip_status_mark_save(&store, &mark));
    CHECK(clip_status_mark_load(&store, &read));
    CHECK_INT(read.percent, 45);
    CHECK_INT(read.level, CLIP_BATTERY_OK);
    CHECK_INT(read.time_s, T0);
    CHECK_STR(read.firmware, "b916044-dirty");
    CHECK_STR(read.error, "Gemini: quota used up");
    CHECK(!clip_status_due(&read, &now));
    // recovery at boot leaves it alone
    clip_recovery_t recovery;
    CHECK(clip_queue_recover(&store, &recovery));
    CHECK(clip_status_mark_load(&store, &read));
    // power lost while it was written: the old mark stands
    fs.cut_at = fs.ops;
    now.cell_mv = 3500;
    clip_status_mark(&now, &mark);
    CHECK(!clip_status_mark_save(&store, &mark));
    fake_store_revive(&fs);
    CHECK(clip_queue_recover(&store, &recovery));
    CHECK(clip_status_mark_load(&store, &read));
    CHECK_INT(read.percent, 45);
    // a file cut short or from something else reads as no report
    fake_put(&fs, "status", "clip-status 1\npercent=4", 23);
    CHECK(!clip_status_mark_load(&store, &read));
    fake_put(&fs, "status", "clip-status 1\npercent=45\n", 25);
    CHECK(!clip_status_mark_load(&store, &read));
    fake_put(&fs, "status", "", 0);
    CHECK(!clip_status_mark_load(&store, &read));
    fake_store_free(&fs);
}

// ---- the update manifest -------------------------------------------------------------------------

static void a_manifest_reads(void)
{
    clip_manifest_t m;
    CHECK(clip_manifest_parse("# capture-clip firmware\nversion=1.2.0+g12ab\r\nurl=https://example.org/clip.bin\n"
                              "size=1500000\nnotes=anything\nsha256=" SHA "\n",
                              &m));
    CHECK_STR(m.version, "1.2.0+g12ab");
    CHECK_STR(m.url, "https://example.org/clip.bin");
    CHECK_INT(m.size, 1500000);
    CHECK_INT(m.sha256[0], 0x00);
    CHECK_INT(m.sha256[1], 0x11);
    CHECK_INT(m.sha256[31], 0xff);
    // no newline at the end
    CHECK(clip_manifest_parse("version=2\nurl=http://10.0.2.2:1/f\nsize=1\nsha256=" SHA, &m));
}

static void a_bad_manifest_is_refused(void)
{
    clip_manifest_t m;
    const char *bad[] = {
        "",
        "<html>404</html>",
        "version=1\nurl=https://e.org/f\nsize=10\n",                                  // no hash
        "version=1\nurl=https://e.org/f\nsha256=" SHA "\n",                           // no size
        "url=https://e.org/f\nsize=10\nsha256=" SHA "\n",                             // no version
        "version=1\nsize=10\nsha256=" SHA "\n",                                       // no url
        "version=1 2\nurl=https://e.org/f\nsize=10\nsha256=" SHA "\n",                // a space
        "version=\nurl=https://e.org/f\nsize=10\nsha256=" SHA "\n",
        "version=1234567890123456789012345678901x\nurl=https://e.org/f\nsize=10\nsha256=" SHA "\n",
        "version=1\nurl=ftp://e.org/f\nsize=10\nsha256=" SHA "\n",
        "version=1\nurl=https://\nsize=10\nsha256=" SHA "\n",
        "version=1\nurl=https://e.org/a b\nsize=10\nsha256=" SHA "\n",
        "version=1\nurl=https://e.org/f\nsize=0\nsha256=" SHA "\n",
        "version=1\nurl=https://e.org/f\nsize=-5\nsha256=" SHA "\n",
        "version=1\nurl=https://e.org/f\nsize=12a\nsha256=" SHA "\n",
        "version=1\nurl=https://e.org/f\nsize=99999999999999999999\nsha256=" SHA "\n",
        "version=1\nurl=https://e.org/f\nsize=10\nsha256=0011\n",
        "version=1\nurl=https://e.org/f\nsize=10\nsha256=zz112233445566778899aabbccddeeff00112233445566778899aabbccddeeff\n",
    };
    for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
        if (clip_manifest_parse(bad[i], &m)) {
            fprintf(stderr, "accepted: %s\n", bad[i]);
            CHECK(false);
        }
    }
    // a url too long for the struct
    char big[600];
    int at = snprintf(big, sizeof big, "version=1\nsize=10\nsha256=" SHA "\nurl=https://e.org/");
    memset(big + at, 'a', 300);
    strcpy(big + at + 300, "\n");
    CHECK(!clip_manifest_parse(big, &m));
}

static void what_to_do_with_a_manifest(void)
{
    clip_manifest_t m;
    CHECK(clip_manifest_parse("version=1.1\nurl=https://e.org/f\nsize=1500000\nsha256=" SHA "\n", &m));
    const long slot = 3 * 1024 * 1024;
    CHECK_INT(clip_update_decide(&m, "1.0", NULL, slot), CLIP_UPDATE_INSTALL);
    CHECK_INT(clip_update_decide(&m, "1.0", "", slot), CLIP_UPDATE_INSTALL);
    CHECK_INT(clip_update_decide(&m, "1.0", "0.9", slot), CLIP_UPDATE_INSTALL);
    CHECK_INT(clip_update_decide(&m, "1.1", NULL, slot), CLIP_UPDATE_NONE);
    // it was tried, failed its own check and was rolled back: not again, or it would loop
    CHECK_INT(clip_update_decide(&m, "1.0", "1.1", slot), CLIP_UPDATE_KNOWN_BAD);
    // running it counts more than an old rollback of the same version
    CHECK_INT(clip_update_decide(&m, "1.1", "1.1", slot), CLIP_UPDATE_NONE);
    CHECK_INT(clip_update_decide(&m, "1.0", NULL, 1500000), CLIP_UPDATE_INSTALL);
    CHECK_INT(clip_update_decide(&m, "1.0", NULL, 1499999), CLIP_UPDATE_TOO_BIG);
}

int main(void)
{
    RUN(the_first_report_is_always_due);
    RUN(nothing_new_is_not_reported);
    RUN(news_is_reported);
    RUN(the_battery_is_reported_in_steps);
    RUN(the_mark_is_kept_on_flash);
    RUN(a_manifest_reads);
    RUN(a_bad_manifest_is_refused);
    RUN(what_to_do_with_a_manifest);
    return unit_done(__FILE__);
}
