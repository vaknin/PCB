// SOURCES: firmware/components/capture/cap_text.c firmware/components/capture/cap_time.c firmware/components/capture/cap_note.c
// The note file and the rules for adding to a note, against Capture's own tests (NoteFileTest.kt,
// AdditionsTest.kt, with their expected values) and golden files in fixtures/capture. All note
// text here is made up.
#include "cap_internal.h"
#include "capture.h"
#include "unit.h"

#include <dirent.h>
#include <stdint.h>

#define AT 1789808400LL // 2026-09-19T09:00:00Z
#define NO_TIME INT64_MIN
#define ID "0f3a9c1e1b744a2b9d0e2c5f6b8a1d33"
#define GOLDEN_ID "bbbbbbbbbbbbbbbb0000000000000003"
#define TYPED_ID "aaaaaaaaaaaaaaaa0000000000000001"
#define SPOKEN_ID "aaaaaaaaaaaaaaaa0000000000000002"
#define TYPED_TEXT "And a second thought,\ntyped later."
#define SPOKEN_TEXT "Spoken third thought."
#define NBSP "\xc2\xa0"     // U+00A0
#define EM_SPACE "\xe2\x80\x83" // U+2003
#define WIDE_SPACE "\xe3\x80\x80" // U+3000

static const char GOLDEN[] = "---\nid: " GOLDEN_ID "\ncreated: 2026-09-19T09:00:00Z\nnum: 3\nsource: phone\n---\n\n"
                             "# Made on the phone\n\nA summary.\n\n## Transcript\n\nWords.\n";

// ---- helpers -------------------------------------------------------------------------------------

static char *copy(const char *text)
{
    return text ? strdup(text) : NULL;
}

// A note as the Kotlin tests write NoteFileContent. NO_TIME: no `created`.
static cap_note_t note_of(int64_t created_s, int num, int64_t duration_ms, const char *title, const char *summary,
                          const char *transcript, const char *source)
{
    return (cap_note_t){
        .has_created = created_s != NO_TIME,
        .created_s = created_s == NO_TIME ? 0 : created_s,
        .num = num,
        .duration_ms = duration_ms,
        .title = copy(title),
        .summary = copy(summary),
        .transcript = copy(transcript),
        .source = copy(source),
    };
}

static void add(cap_note_t *note, const char *id, int64_t created_s, const char *source, int64_t duration_ms,
                const char *text)
{
    note->additions = realloc(note->additions, (note->addition_count + 1) * sizeof *note->additions);
    cap_addition_t *addition = &note->additions[note->addition_count++];
    *addition = (cap_addition_t){.created_s = created_s, .duration_ms = duration_ms, .source = copy(source), .text = copy(text)};
    snprintf(addition->id, sizeof addition->id, "%s", id);
}

// Runs checks for a helper and says which line of the test called it.
#define FROM_LINE(line, checks)                                                                          \
    do {                                                                                                 \
        int failed_before = unit_test_failed;                                                            \
        unit_test_failed = 0;                                                                            \
        checks;                                                                                          \
        if (unit_test_failed) {                                                                          \
            fprintf(stderr, "  ^ called from line %d\n", line);                                          \
        }                                                                                                \
        unit_test_failed |= failed_before;                                                               \
    } while (0)

static void same_additions(const cap_note_t *got, const cap_note_t *want)
{
    CHECK_INT(got->addition_count, want->addition_count);
    for (size_t i = 0; i < got->addition_count && i < want->addition_count; i++) {
        CHECK_STR(got->additions[i].id, want->additions[i].id);
        CHECK_INT(got->additions[i].created_s, want->additions[i].created_s);
        CHECK_STR(got->additions[i].source, want->additions[i].source);
        CHECK_INT(got->additions[i].duration_ms, want->additions[i].duration_ms);
        CHECK_STR(got->additions[i].text, want->additions[i].text);
    }
}

static void same_note_checks(const cap_note_t *got, const cap_note_t *want)
{
    CHECK_INT(got->has_created, want->has_created);
    if (got->has_created && want->has_created) {
        CHECK_INT(got->created_s, want->created_s);
    }
    CHECK_INT(got->num, want->num);
    CHECK_INT(got->duration_ms, want->duration_ms);
    CHECK_STR(got->title, want->title);
    CHECK_STR(got->summary, want->summary);
    CHECK_STR(got->transcript, want->transcript);
    if (want->source) {
        CHECK_STR(got->source, want->source);
    } else {
        CHECK(got->source == NULL);
    }
    same_additions(got, want);
}

#define SAME_NOTE(got, want) FROM_LINE(__LINE__, same_note_checks(got, want))

// parse(text) is *want, which is freed here.
static void parses_checks(const char *text, cap_note_t want)
{
    cap_note_t got;
    CHECK(cap_note_parse(text, &got));
    same_note_checks(&got, &want);
    cap_note_free(&got);
    cap_note_free(&want);
}

#define PARSES(text, want) FROM_LINE(__LINE__, parses_checks(text, want))

static void renders_checks(const char *id, const cap_note_t *note, const char *expected)
{
    char *text = cap_note_render(id, note);
    CHECK_STR(text, expected);
    free(text);
}

#define RENDERS(id, note, expected) FROM_LINE(__LINE__, renders_checks(id, note, expected))

// render() of a note made from these fields, as most Kotlin cases start. The note is freed.
static char *rendered(const char *id, cap_note_t note)
{
    char *text = cap_note_render(id, &note);
    cap_note_free(&note);
    return text;
}

static char *crlf(const char *text)
{
    cap_buf_t out = {0};
    for (const char *p = text; *p; p++) {
        if (*p == '\n') {
            cap_buf_str(&out, "\r");
        }
        cap_buf_add(&out, p, 1);
    }
    return cap_buf_take(&out);
}

static cap_note_t parsed(const char *text)
{
    cap_note_t note;
    CHECK(cap_note_parse(text, &note));
    return note;
}

static void add_typed(cap_note_t *note)
{
    add(note, TYPED_ID, AT + 3600, "laptop", CAP_NO_DURATION, TYPED_TEXT);
}

static void add_spoken(cap_note_t *note)
{
    add(note, SPOKEN_ID, AT + 7200, "phone", 18341, SPOKEN_TEXT);
}

// ---- time ----------------------------------------------------------------------------------------

static void formats(int64_t epoch_s, const char *expected)
{
    char iso[CAP_ISO_LEN];
    cap_iso_format(epoch_s, iso);
    CHECK_STR(iso, expected);
    int64_t back = 0;
    CHECK(cap_iso_parse(expected, &back));
    CHECK_INT(back, epoch_s);
}

static void iso_format(void)
{
    formats(0, "1970-01-01T00:00:00Z");
    formats(AT, "2026-09-19T09:00:00Z");
    formats(86399, "1970-01-01T23:59:59Z");
    formats(1709208000, "2024-02-29T12:00:00Z"); // leap day
    formats(951825600, "2000-02-29T12:00:00Z");  // a century that is a leap year
    formats(4107542400LL, "2100-03-01T00:00:00Z"); // and one that is not; past 2038
    formats(4107542399LL, "2100-02-28T23:59:59Z");
    formats(1798761599, "2026-12-31T23:59:59Z");
    formats(1798761600, "2027-01-01T00:00:00Z");
    // before 1970
    formats(-1, "1969-12-31T23:59:59Z");
    formats(-86400, "1969-12-31T00:00:00Z");
    formats(-14182940, "1969-07-20T20:17:40Z");
    formats(-2203891200LL, "1900-03-01T00:00:00Z");
    formats(-2203891201LL, "1900-02-28T23:59:59Z");
}

static void parses_iso(const char *text, int64_t expected)
{
    int64_t got = 0;
    if (!cap_iso_parse(text, &got)) {
        fprintf(stderr, "%s:%d: \"%s\" was not read\n", __FILE__, __LINE__, text);
        unit_test_failed = 1;
        return;
    }
    if (got != expected) {
        fprintf(stderr, "%s:%d: \"%s\" is %lld, expected %lld\n", __FILE__, __LINE__, text, (long long)got,
                (long long)expected);
        unit_test_failed = 1;
    }
}

static void iso_parse(void)
{
    parses_iso("2026-09-19T09:00:00Z", AT);
    parses_iso("2026-09-19t09:00:00z", AT); // both parsers ignore case
    // offsets
    parses_iso("2026-09-19T12:00:00+03:00", AT);
    parses_iso("2026-09-19T02:00:00-07:00", AT);
    parses_iso("2026-09-19T14:30:00+05:30", AT);
    parses_iso("2026-09-19T09:00:00+00:00", AT);
    parses_iso("2026-09-20T01:30:00+03:00", AT + 13 * 3600 + 1800); // the date is the offset's
    parses_iso("2026-09-19T12:00:30+03:00:30", AT);
    parses_iso("2026-09-20T03:00:00+18:00", AT);
    parses_iso("2026-09-18T15:00:00-18:00", AT);
    parses_iso("1970-01-01T00:00:00+01:00", -3600);
    // fractions are dropped
    parses_iso("2026-09-19T09:00:00.999Z", AT);
    parses_iso("2026-09-19T09:00:00.5Z", AT);
    parses_iso("2026-09-19T09:00:00.123456789Z", AT);
    parses_iso("2026-09-19T12:00:00.250+03:00", AT);
    // OffsetDateTime reads a time without seconds
    parses_iso("2026-09-19T09:00Z", AT);
    parses_iso("2026-09-19T12:00+03:00", AT);
    // leap years
    parses_iso("2024-02-29T12:00:00Z", 1709208000);
    parses_iso("2000-02-29T12:00:00Z", 951825600);
    parses_iso("1969-07-20T20:17:40Z", -14182940);

    const char *bad[] = {
        "",
        "yesterday",
        "2026-09-19",
        "2026-09-19T09:00:00", // no zone
        "2026-09-19 09:00:00Z",
        "2026-09-19T09:00:00Zx",
        " 2026-09-19T09:00:00Z",
        "2026-09-19T09:00:00Z ",
        "2026-9-19T09:00:00Z",
        "26-09-19T09:00:00Z",
        "2026-13-01T00:00:00Z",
        "2026-00-10T00:00:00Z",
        "2026-09-00T00:00:00Z",
        "2026-09-31T00:00:00Z",
        "2023-02-29T00:00:00Z",
        "1900-02-29T00:00:00Z",
        "2100-02-29T00:00:00Z",
        "2026-09-19T24:00:01Z",
        "2026-09-19T09:60:00Z",
        "2026-09-19T09:00:61Z",
        "2026-09-19T9:00:00Z",
        "2026-09-19T09:00:00.1234567890Z", // more than nanoseconds
        "2026-09-19T09:00:00+0300",
        "2026-09-19T09:00:00+03",
        "2026-09-19T09:00:00+3:00",
        "2026-09-19T09:00:00+03:60",
        "2026-09-19T09:00:00+19:00",
        "2026-09-19T09:00:00+18:30", // ZoneOffset ends at 18:00
        "2026-09-19T09:00:00-18:00:01",
        "2026-09-19T09:00:00 +03:00",
        "1789808400",
    };
    for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
        int64_t got = 7;
        if (cap_iso_parse(bad[i], &got)) {
            fprintf(stderr, "%s:%d: \"%s\" was read as %lld\n", __FILE__, __LINE__, bad[i], (long long)got);
            unit_test_failed = 1;
        }
        CHECK_INT(got, 7); // untouched
    }
}

static void iso_round_trip(void)
{
    // every 100003 s (a prime, so every time of day comes up) from 1899 to 2101
    int wrong = 0;
    for (int64_t t = -2240000000LL; t < 4140000000LL; t += 100003) {
        char iso[CAP_ISO_LEN];
        int64_t back = 0;
        cap_iso_format(t, iso);
        if (strlen(iso) != 20 || !cap_iso_parse(iso, &back) || back != t) {
            if (!wrong++) {
                fprintf(stderr, "%s:%d: %lld formats as \"%s\", read back as %lld\n", __FILE__, __LINE__, (long long)t,
                        iso, (long long)back);
            }
        }
    }
    CHECK_INT(wrong, 0);
}

// ---- trim ----------------------------------------------------------------------------------------

static void trims(const char *text, const char *expected)
{
    char *got = cap_trimmed(text);
    CHECK_STR(got, expected);
    free(got);
}

// Kotlin's trim is Char.isWhitespace: Character.isWhitespace or isSpaceChar.
static void trim_unicode(void)
{
    trims(NULL, "");
    trims("", "");
    trims(" \t\r\n\v\f x y \n", "x y");
    trims("\x1c\x1d\x1e\x1f" "x" "\x1f", "x"); // the separators FS..US
    trims(NBSP "x" NBSP, "x");
    trims(EM_SPACE " x y" WIDE_SPACE EM_SPACE "\n", "x y");
    trims("\xe1\x9a\x80" "x" "\xe2\x81\x9f", "x");             // U+1680, U+205F
    trims("\xe2\x80\x80" "x" "\xe2\x80\x8a", "x");             // U+2000, U+200A
    trims("\xe2\x80\x87" "x" "\xe2\x80\xaf", "x");             // U+2007 and U+202F, the no-break ones
    trims("\xe2\x80\xa8" "x" "\xe2\x80\xa9", "x");             // U+2028, U+2029
    trims("x" NBSP "y", "x" NBSP "y");                         // inside stays
    trims(" \xe2\x80\x8b" "x" "\xe2\x80\x8b ", "\xe2\x80\x8b" "x" "\xe2\x80\x8b"); // U+200B is no whitespace
    trims("\xc2\x85" "x" "\xc2\x85", "\xc2\x85" "x" "\xc2\x85");   // nor U+0085
    trims("\xef\xbb\xbf" "x", "\xef\xbb\xbf" "x");             // nor U+FEFF
    trims("\x1b" "x" "\x08", "\x1b" "x" "\x08");               // nor other control characters
    trims("x\xe2\x80\xa6", "x\xe2\x80\xa6");                   // U+2026 shares two bytes with U+2003
    trims("caf\xc3\xa9 ", "caf\xc3\xa9");
    trims(" " NBSP WIDE_SPACE "\t", "");

    CHECK(cap_is_blank(NULL));
    CHECK(cap_is_blank(""));
    CHECK(cap_is_blank(" \n\t"));
    CHECK(cap_is_blank(NBSP EM_SPACE WIDE_SPACE "\t\x1c"));
    CHECK(!cap_is_blank("\xe2\x80\x8b"));
    CHECK(!cap_is_blank(" . "));

    const char *text = " " NBSP "x y" WIDE_SPACE "\n tail";
    const char *p = text;
    size_t len = strlen(text) - 5; // without " tail"
    cap_trim(&p, &len);
    CHECK_INT(len, 3);
    CHECK(p == text + 3 && memcmp(p, "x y", 3) == 0);

    len = strlen(text) - 5;
    cap_trim_end(text, &len);
    CHECK_INT(len, 6); // the start stays
    len = 0;
    cap_trim_end(text, &len);
    CHECK_INT(len, 0);
    p = text;
    len = 3; // all whitespace
    cap_trim(&p, &len);
    CHECK_INT(len, 0);
}

// ---- ids -----------------------------------------------------------------------------------------

// newIdsAreValid: [0-9a-f]{32}
static void new_ids(void)
{
    const uint8_t random[16] = {0x00, 0x01, 0x0f, 0x10, 0x7f, 0x80, 0x9a, 0xab, 0xbc, 0xcd, 0xde, 0xef, 0xf0, 0xfe, 0xff, 0x5a};
    char id[40];
    memset(id, '#', sizeof id);
    cap_new_id(random, id);
    CHECK_STR(id, "00010f107f809aabbccddeeff0feff5a");
    CHECK_INT(id[33], '#'); // 33 bytes and no more

    const uint8_t zero[16] = {0};
    cap_new_id(zero, id);
    CHECK_STR(id, "00000000000000000000000000000000");

    char *path = cap_note_path(ID);
    CHECK_STR(path, "notes/" ID ".md");
    free(path);
    path = cap_note_path("0F3A9C1E-1B74-4A2B-9D0E-2C5F6B8A1D33");
    CHECK_STR(path, "notes/0F3A9C1E-1B74-4A2B-9D0E-2C5F6B8A1D33.md");
    free(path);
}

// ---- NoteFileTest.kt -----------------------------------------------------------------------------

static void golden_render(void)
{
    cap_note_t note = note_of(AT, 3, CAP_NO_DURATION, "Made on the phone", "A summary.", "Words.", "phone");
    RENDERS(GOLDEN_ID, &note, GOLDEN);
    free(note.source); // a note with no source is the phone's
    note.source = NULL;
    RENDERS(GOLDEN_ID, &note, GOLDEN);
    note.source = copy("");
    RENDERS(GOLDEN_ID, &note, GOLDEN);
    cap_note_free(&note);
}

static void render_with_duration_blank_summary_and_transcript(void)
{
    const char *expected = "---\nid: " ID "\ncreated: 2026-09-19T09:00:00Z\nduration_ms: 42000\nsource: phone\n---\n\n"
                           "# Nothing heard\n\n## Transcript\n";
    cap_note_t note = note_of(AT, 0, 42000, " Nothing\nheard ", " \n", "", "phone");
    RENDERS(ID, &note, expected);
    cap_note_free(&note);
    note = note_of(AT, 0, 42000, " Nothing\r\nheard\r", NULL, NULL, NULL);
    RENDERS(ID, &note, expected);
    cap_note_free(&note);
}

static void render_edges(void)
{
    // no `created`: nothing to write
    cap_note_t note = note_of(NO_TIME, 1, 1, "T", "S", "W", "phone");
    CHECK(cap_note_render(ID, &note) == NULL);
    cap_note_free(&note);

    // a blank title is "Untitled"; CR and CRLF become LF; Unicode whitespace is trimmed; 0 ms is a duration
    note = note_of(AT, 0, 0, " " NBSP "\n", "\r\n- one\r\n- two\r" WIDE_SPACE, EM_SPACE "a\rb\r\n\r\nc\n\n", "clip");
    add(&note, TYPED_ID, AT + 1, "clip", 0, " x\r\ny ");
    RENDERS(ID, &note,
            "---\nid: " ID "\ncreated: 2026-09-19T09:00:00Z\nduration_ms: 0\nsource: clip\n---\n\n"
            "# Untitled\n\n- one\n- two\n\n## Transcript\n\na\nb\n\nc\n"
            "\n## Added\n<!-- addition " TYPED_ID " created=2026-09-19T09:00:01Z source=clip duration_ms=0 -->\n\nx\ny\n");
    cap_note_free(&note);

    note = note_of(AT, 0, CAP_NO_DURATION, NULL, NULL, NULL, NULL);
    RENDERS(ID, &note, "---\nid: " ID "\ncreated: 2026-09-19T09:00:00Z\nsource: phone\n---\n\n# Untitled\n\n## Transcript\n");
    cap_note_free(&note);
}

static void round_trip(void)
{
    const char *summary = "- Sketch the deduction flow first.\n- Then the export.";
    const char *transcript = "So I was thinking about the tax thing again.\n\nAnd another paragraph.";
    char *text = rendered(ID, note_of(AT, 7, 42000, "Tax filing app concept", summary, transcript, "phone"));
    PARSES(text, note_of(AT, 7, 42000, "Tax filing app concept", summary, transcript, "phone"));
    free(text);
}

// A typed note is a note with no duration; a short one is all title.
static void typed_note_round_trip(void)
{
    char *brief = rendered(ID, note_of(AT, 0, CAP_NO_DURATION, "water the basil", "", "", "phone"));
    CHECK_STR(brief, "---\nid: " ID "\ncreated: 2026-09-19T09:00:00Z\nsource: phone\n---\n\n"
                     "# water the basil\n\n## Transcript\n");
    PARSES(brief, note_of(AT, 0, CAP_NO_DURATION, "water the basil", "", "", "phone"));
    free(brief);

    const char *typed = "Pond plan for the spring.\nBuy a skimmer.";
    char *longer = rendered(ID, note_of(AT, 0, CAP_NO_DURATION, "Spring pond plan", "- Buy a skimmer", typed, "phone"));
    PARSES(longer, note_of(AT, 0, CAP_NO_DURATION, "Spring pond plan", "- Buy a skimmer", typed, "phone"));
    free(longer);
}

static void crlf_and_bom(void)
{
    char *windows = crlf(GOLDEN);
    cap_buf_t text = {0};
    cap_buf_str(&text, "\xef\xbb\xbf");
    cap_buf_str(&text, windows);
    PARSES(text.data, note_of(AT, 3, CAP_NO_DURATION, "Made on the phone", "A summary.", "Words.", "phone"));
    free(windows);
    free(cap_buf_take(&text));
    // old Mac line ends
    PARSES("---\rcreated: 2026-09-19T09:00:00Z\r---\r# T\rS\r## Transcript\rW\r",
           note_of(AT, 0, CAP_NO_DURATION, "T", "S", "W", NULL));
}

static void no_frontmatter(void)
{
    PARSES("Just a line typed by hand\n\nNo frontmatter here.\n",
           note_of(NO_TIME, 0, CAP_NO_DURATION, "Just a line typed by hand", "No frontmatter here.", "", NULL));
}

static void empty_file(void)
{
    PARSES("", note_of(NO_TIME, 0, CAP_NO_DURATION, "Untitled", "", "", NULL));
    PARSES("\n \n" NBSP "\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "Untitled", "", "", NULL));
    PARSES("# \n", note_of(NO_TIME, 0, CAP_NO_DURATION, "Untitled", "", "", NULL));
}

static void unclosed_frontmatter_is_body(void)
{
    PARSES("---\nid: x\n# Title\nText", note_of(NO_TIME, 0, CAP_NO_DURATION, "Title", "Text", "", NULL));
}

static void bad_frontmatter_values_are_null(void)
{
    PARSES("---\ncreated: yesterday\nduration_ms: -5\n---\n# T\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "", "", NULL));
    PARSES("---\ncreated: 2026-09-19T12:00:00+03:00\n---\n# T\n", note_of(AT, 0, CAP_NO_DURATION, "T", "", "", NULL));
    PARSES("---\nduration_ms: 12x\nsource:\n---\n# T\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "", "", NULL));
    PARSES("---\nduration_ms: 99999999999999999999\n---\n# T\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "", "", NULL));
}

static void frontmatter_edges(void)
{
    // spaces around keys and values, a closing line with trailing spaces, unknown keys, a line with
    // no key; of two lines with one key the last counts; the value keeps its colons
    PARSES("--- \n  created :  2026-09-19T09:00:00Z  \nnum: 4\nnum: 5\nmood: good\n: x\nplain\nduration_ms:0\n"
           "source: my laptop: the old one\n---\t\n# T\n",
           note_of(AT, 5, 0, "T", "", "", "my laptop: the old one"));
    // a second `---` is body
    PARSES("---\nnum: 2\n---\n# T\nabove\n---\nbelow\n", note_of(NO_TIME, 2, CAP_NO_DURATION, "T", "above\n---\nbelow", "", NULL));
    // "----" does not open frontmatter
    PARSES("----\nnum: 2\n---\n# T\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "", "", NULL));
}

// The line goes right after `created`, and only a whole number >= 1 counts.
static void num_round_trip(void)
{
    char *text = rendered(ID, note_of(AT, 12, 42000, "Numbered", "", "Words.", "phone"));
    CHECK_STR(text, "---\nid: " ID "\ncreated: 2026-09-19T09:00:00Z\nnum: 12\nduration_ms: 42000\nsource: phone\n---\n\n"
                    "# Numbered\n\n## Transcript\n\nWords.\n");
    cap_note_t note = parsed(text);
    CHECK_INT(note.num, 12);
    cap_note_free(&note);
    free(text);

    text = rendered(ID, note_of(AT, 0, CAP_NO_DURATION, "T", "", "", "phone"));
    note = parsed(text);
    CHECK_INT(note.num, 0);
    cap_note_free(&note);
    free(text);
}

static void bad_nums_are_null(void)
{
    const char *bad[] = {"0", "-3", "abc", "7.5", "", "99999999999", "+7", "007", "1234567890", "1 2", "0x10"};
    for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
        char text[128];
        snprintf(text, sizeof text, "---\ncreated: 2026-09-19T09:00:00Z\nnum: %s\n---\n# T\n", bad[i]);
        cap_note_t note = parsed(text);
        if (note.num != 0) {
            fprintf(stderr, "%s:%d: num \"%s\" is %d, expected none\n", __FILE__, __LINE__, bad[i], note.num);
            unit_test_failed = 1;
        }
        CHECK(note.has_created);
        cap_note_free(&note);
    }
    cap_note_t note = parsed("---\nnum: 1\n---\n# T\n");
    CHECK_INT(note.num, 1);
    cap_note_free(&note);
    note = parsed("---\nnum: 999999999\n---\n# T\n");
    CHECK_INT(note.num, 999999999);
    cap_note_free(&note);
}

static void mangled_headings(void)
{
    // "##Transcript" is not a heading: everything after the title is summary.
    PARSES("# Title\nSummary\n##Transcript\nwords\n",
           note_of(NO_TIME, 0, CAP_NO_DURATION, "Title", "Summary\n##Transcript\nwords", "", NULL));
    // No "# " title: the first non-blank line, without its hashes.
    PARSES("\n\n##  Title here\nbody\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "Title here", "body", "", NULL));
    PARSES("# T\n\n## transcript  \nwords", note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "", "words", NULL));
}

static void heading_edges(void)
{
    PARSES("# T\nS\n  ##\t TRANSCRIPT\nW\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "S", "W", NULL));
    // the line is trimmed as Kotlin trims; the regex's \s is ASCII only
    PARSES("# T\nS\n" NBSP "## Transcript" WIDE_SPACE "\nW\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "S", "W", NULL));
    PARSES("# T\nS\n##" NBSP "Transcript\nW\n",
           note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "S\n##" NBSP "Transcript\nW", "", NULL));
    PARSES("# T\nS\n## Transcripts\nW\n### Transcript\n## Transcript:\n",
           note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "S\n## Transcripts\nW\n### Transcript\n## Transcript:", "", NULL));
    // with no "# " line the transcript heading is not the title, and a heading above the title is no heading
    PARSES("## Transcript\nfirst\nsecond\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "first", "second", "", NULL));
    PARSES("## Transcript\nfirst\n## Transcript\nsecond\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "first", "", "second", NULL));
    // what is above a late title is dropped
    PARSES("above\n## Transcript\nold\n# " NBSP "T" EM_SPACE "\nS\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "S", "", NULL));
    // "#T" is no title line, but it is the first line
    PARSES("#T\nS\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "S", "", NULL));
    PARSES("###\nS\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "Untitled", "S", "", NULL));
}

static void transcript_heading_inside_summary(void)
{
    const char *summary = "Notes on the file format:\n\n## Transcript\n\nlooks like this.";
    char *text = rendered(ID, note_of(AT, 0, CAP_NO_DURATION, "Format", summary, "The real transcript.", "phone"));
    PARSES(text, note_of(AT, 0, CAP_NO_DURATION, "Format", summary, "The real transcript.", "phone"));
    free(text);
}

#define GOLDEN_ADDITIONS                                                                                 \
    "\n## Added\n<!-- addition " TYPED_ID " created=2026-09-19T10:00:00Z source=laptop -->\n\n"          \
    "And a second thought,\ntyped later.\n"                                                              \
    "\n## Added\n<!-- addition " SPOKEN_ID " created=2026-09-19T11:00:00Z source=phone duration_ms=18341 -->\n\n" \
    "Spoken third thought.\n"

static void golden_render_with_additions(void)
{
    cap_note_t note = note_of(AT, 3, CAP_NO_DURATION, "Made on the phone", "A summary.", "Words.", "phone");
    add_typed(&note);
    add_spoken(&note);
    char expected[sizeof GOLDEN + sizeof GOLDEN_ADDITIONS];
    snprintf(expected, sizeof expected, "%s%s", GOLDEN, GOLDEN_ADDITIONS);
    RENDERS(GOLDEN_ID, &note, expected);
    cap_note_free(&note);
}

static void additions_round_trip(void)
{
    cap_note_t note = note_of(AT, 7, 42000, "T", "S.", "Original.", "phone");
    add_typed(&note);
    add_spoken(&note);
    char *text = cap_note_render(ID, &note);
    cap_note_t back = parsed(text);
    SAME_NOTE(&back, &note);
    cap_note_free(&back);
    // CRLF, like a file edited on Windows.
    char *windows = crlf(text);
    back = parsed(windows);
    SAME_NOTE(&back, &note);
    cap_note_free(&back);
    free(windows);
    free(text);
    cap_note_free(&note);
}

static void source_is_kept(void)
{
    char *text = rendered(ID, note_of(AT, 0, CAP_NO_DURATION, "T", "", "O.", "laptop"));
    PARSES(text, note_of(AT, 0, CAP_NO_DURATION, "T", "", "O.", "laptop"));
    free(text);
    PARSES("# T\n", note_of(NO_TIME, 0, CAP_NO_DURATION, "T", "", "", NULL));
}

// A parser from before additions reads everything after `## Transcript`: nothing is lost.
static void old_parser_sees_additions_in_transcript(void)
{
    cap_note_t note = note_of(AT, 0, CAP_NO_DURATION, "T", "", "Original.", "phone");
    add_typed(&note);
    char *text = cap_note_render(ID, &note);
    const char *heading = text ? strstr(text, "## Transcript\n") : NULL;
    CHECK(heading != NULL);
    if (heading) {
        char *old = cap_trimmed(heading + strlen("## Transcript\n"));
        size_t len = strlen(old), tail = strlen("typed later.");
        CHECK(strncmp(old, "Original.", 9) == 0);
        CHECK(len >= tail && strcmp(old + len - tail, "typed later.") == 0);
        free(old);
    }
    free(text);
    cap_note_free(&note);
}

static void empty_addition_and_no_transcript(void)
{
    cap_note_t note = note_of(AT, 0, CAP_NO_DURATION, "water the basil", "", "", "phone");
    add(&note, "aaaaaaaaaaaaaaaa0000000000000009", AT, "phone", 1000, "");
    char *text = cap_note_render(ID, &note);
    CHECK_STR(text, "---\nid: " ID "\ncreated: 2026-09-19T09:00:00Z\nsource: phone\n---\n\n# water the basil\n\n## Transcript\n"
                    "\n## Added\n<!-- addition aaaaaaaaaaaaaaaa0000000000000009 created=2026-09-19T09:00:00Z source=phone"
                    " duration_ms=1000 -->\n");
    cap_note_t back = parsed(text);
    SAME_NOTE(&back, &note);
    cap_note_free(&back);
    free(text);
    cap_note_free(&note);
}

#define MARKS_BODY "---\ncreated: 2026-09-19T09:00:00Z\n---\n# T\n\n## Transcript\n\nOriginal.\n\n"

// A heading without a mark, a mark with an unreadable time, a mark not right under its heading.
static void malformed_addition_marks_are_text(void)
{
    const char *tails[] = {
        "## Added\n\nNo mark.\n",
        "## Added\n<!-- addition " TYPED_ID " created=yesterday -->\n\nBad time.\n",
        "## Added\n\n<!-- addition " TYPED_ID " created=2026-09-19T10:00:00Z -->\n\nLoose mark.\n",
        // and more of the same: no time, a short id, a long id, no id, a bad field, no end, a comment of another kind
        "## Added\n<!-- addition " TYPED_ID " -->\n\nNo time.\n",
        "## Added\n<!-- addition aaaaaaaaaaaaaaa created=2026-09-19T10:00:00Z -->\n\n15 digits.\n",
        "## Added\n<!-- addition " TYPED_ID TYPED_ID "a created=2026-09-19T10:00:00Z -->\n\n65 digits.\n",
        "## Added\n<!-- addition gggggggggggggggggggg created=2026-09-19T10:00:00Z -->\n\nNot hex.\n",
        "## Added\n<!-- addition created=2026-09-19T10:00:00Z -->\n\nNo id.\n",
        "## Added\n<!-- addition " TYPED_ID " created=2026-09-19T10:00:00Z Source=x -->\n\nUpper case key.\n",
        "## Added\n<!-- addition " TYPED_ID " created=2026-09-19T10:00:00Z loose -->\n\nA word.\n",
        "## Added\n<!-- addition " TYPED_ID " created=2026-09-19T10:00:00Z source= -->\n\nNo value.\n",
        "## Added\n<!-- addition " TYPED_ID " created=2026-09-19T10:00:00Z\n\nNo end.\n",
        "## Added\n<!-- addition " TYPED_ID " created=2026-09-19T10:00:00Z --> x\n\nText after.\n",
        "## Added\n<!-- additions " TYPED_ID " created=2026-09-19T10:00:00Z -->\n\nPlural.\n",
        "## Added\n<!-- Addition " TYPED_ID " created=2026-09-19T10:00:00Z -->\n\nUpper case.\n",
        "##Added\n<!-- addition " TYPED_ID " created=2026-09-19T10:00:00Z -->\n\nNo space.\n",
        "# Added\n<!-- addition " TYPED_ID " created=2026-09-19T10:00:00Z -->\n\nOne hash.\n",
    };
    for (size_t i = 0; i < sizeof tails / sizeof *tails; i++) {
        cap_buf_t text = {0}, transcript = {0};
        cap_buf_str(&text, MARKS_BODY);
        cap_buf_str(&text, tails[i]);
        char *tail = cap_trimmed(tails[i]);
        cap_buf_str(&transcript, "Original.\n\n");
        cap_buf_str(&transcript, tail);
        free(tail);
        cap_note_t note = parsed(text.data);
        if (note.addition_count != 0 || !note.transcript || strcmp(note.transcript, transcript.data) != 0) {
            fprintf(stderr, "%s:%d: tail %zu: %zu additions, transcript \"%s\"\n", __FILE__, __LINE__, i,
                    note.addition_count, note.transcript ? note.transcript : "(null)");
            unit_test_failed = 1;
        }
        cap_note_free(&note);
        free(cap_buf_take(&text));
        free(cap_buf_take(&transcript));
    }
}

// What ADDITION_MARK does read.
static void addition_mark_edges(void)
{
    cap_note_t want = note_of(AT, 0, CAP_NO_DURATION, "T", "", "Original.", NULL);
    // no spaces at the ends; an id in upper case with dashes; of two fields the last counts; unknown fields
    add(&want, "0F3A9C1E-1B74-4A2B-9D0E-2C5F6B8A1D33", AT + 60, "laptop", 5, "One.");
    // no source is the phone's; a bad duration is none; an offset time; a value that holds "-->"
    add(&want, "aaaaaaaaaaaaaaaa", AT + 3600, "phone", CAP_NO_DURATION, "Two.\n\n## Added\n\nnot a mark");
    add(&want, "bbbbbbbbbbbbbbbb", AT + 7200, "a-->b", CAP_NO_DURATION, "");
    // 64 digits; tabs between fields; a heading in other case, indented
    add(&want, TYPED_ID TYPED_ID, AT + 7201, "=x=", 0, "Four.");
    PARSES(MARKS_BODY
           "## Added\n<!--addition 0F3A9C1E-1B74-4A2B-9D0E-2C5F6B8A1D33 source=phone created=2020-01-01T00:00:00Z duration_ms=5"
           " mood=good source=laptop created=2026-09-19T09:01:00Z-->\nOne.\n"
           "## Added\n  <!--   addition   aaaaaaaaaaaaaaaa   created=2026-09-19T13:00:00+03:00   duration_ms=-4   -->  \n\n"
           "Two.\n\n## Added\n\nnot a mark\n"
           "##  ADDED \n<!-- addition bbbbbbbbbbbbbbbb created=2026-09-19T11:00:00Z duration_ms=1.5 source=a-->b -->\n"
           "  ##\tadded\n<!-- addition " TYPED_ID TYPED_ID "\tcreated=2026-09-19T11:00:01Z\tsource==x=\tduration_ms=0 -->\n"
           "\nFour.\n\n",
           want);
}

// The file ends at a mark, with no line end.
static void addition_mark_on_the_last_line(void)
{
    cap_note_t want = note_of(AT, 0, CAP_NO_DURATION, "T", "", "Original.", NULL);
    add(&want, TYPED_ID, AT + 3600, "phone", CAP_NO_DURATION, "");
    PARSES(MARKS_BODY "## Added\n<!-- addition " TYPED_ID " created=2026-09-19T10:00:00Z -->", want);
}

static void duplicate_addition_id_keeps_first(void)
{
    cap_note_t note = note_of(AT, 0, CAP_NO_DURATION, "T", "", "O.", "phone");
    add_typed(&note);
    add(&note, TYPED_ID, AT + 3600, "laptop", CAP_NO_DURATION, "Other.");
    add_spoken(&note);
    char *text = cap_note_render(ID, &note);
    cap_note_free(&note);

    cap_note_t want = note_of(AT, 0, CAP_NO_DURATION, "T", "", "O.", "phone");
    add_typed(&want);
    add_spoken(&want);
    PARSES(text, want);
    free(text);
}

// A `## Transcript` line typed into an addition stays in it.
static void transcript_heading_inside_addition(void)
{
    cap_note_t note = note_of(AT, 0, CAP_NO_DURATION, "T", "S.", "Original.", "phone");
    add(&note, TYPED_ID, AT + 3600, "laptop", CAP_NO_DURATION, "About the format:\n\n## Transcript\n\nis a heading.");
    char *text = cap_note_render(ID, &note);
    cap_note_t back = parsed(text);
    SAME_NOTE(&back, &note);
    cap_note_free(&back);
    free(text);
    cap_note_free(&note);
}

// ---- AdditionsTest.kt ----------------------------------------------------------------------------

static void whole_text_is(cap_note_t note, const char *expected)
{
    char *text = cap_note_whole_text(&note);
    CHECK_STR(text, expected);
    free(text);
    cap_note_free(&note);
}

static void original_is_the_transcript(void)
{
    whole_text_is(note_of(AT, 0, 5000, "Tax app", "", " So I was thinking\n", NULL), "So I was thinking");
    // A spoken note in which nothing was heard has no text, whatever its title says.
    whole_text_is(note_of(AT, 0, 5000, "Nothing heard", "", "", NULL), "");
    whole_text_is(note_of(AT, 0, 0, "Nothing heard", "", NULL, NULL), "");
}

// A short typed note from before 0.8.0 is only a title. Typed is inferred here: no transcript, no duration.
static void old_short_typed_note_is_its_title(void)
{
    whole_text_is(note_of(AT, 0, CAP_NO_DURATION, "water the basil", "", NULL, NULL), "water the basil");
    whole_text_is(note_of(AT, 0, CAP_NO_DURATION, " water the basil\n", "", " " NBSP "\n", NULL), "water the basil");
    whole_text_is(note_of(AT, 0, CAP_NO_DURATION, "water", "", "water the basil", NULL), "water the basil");
    whole_text_is(note_of(AT, 0, CAP_NO_DURATION, NULL, NULL, NULL, NULL), "");
}

static void whole_text_joins_with_blank_lines(void)
{
    cap_note_t note = note_of(AT, 0, 1000, "T", "", "One.", NULL);
    add(&note, TYPED_ID, AT + 1, "phone", CAP_NO_DURATION, " Two.\n");
    add(&note, SPOKEN_ID, AT + 2, "phone", 1000, "Three.");
    whole_text_is(note, "One.\n\nTwo.\n\nThree.");

    note = note_of(AT, 0, 1000, "T", "", "", NULL);
    add(&note, TYPED_ID, AT + 1, "phone", CAP_NO_DURATION, "");
    add(&note, SPOKEN_ID, AT + 2, "phone", 1000, "Two.");
    whole_text_is(note, "Two.");

    // an old typed note with an addition; an addition with no text at all
    note = note_of(AT, 0, CAP_NO_DURATION, "water the basil", "", "", NULL);
    add(&note, TYPED_ID, AT + 1, "laptop", CAP_NO_DURATION, NULL);
    add(&note, SPOKEN_ID, AT + 2, "clip", 1000, WIDE_SPACE "and the mint" NBSP);
    whole_text_is(note, "water the basil\n\nand the mint");
}

// ---- Gemini's answer (Repository.saveResult) -----------------------------------------------------

static void set_answer(void)
{
    cap_note_t note = note_of(AT, 0, 12500, NULL, NULL, NULL, NULL);
    cap_answer_t answer = {" A title\n", "\n- one\n- two " NBSP, " Words said. \n"};
    CHECK(cap_note_set_answer(&note, &answer));
    CHECK_STR(note.title, "A title");
    CHECK_STR(note.summary, "- one\n- two");
    CHECK_STR(note.transcript, "Words said.");
    CHECK_STR(note.source, CAP_SOURCE); // a new note is this device's
    CHECK_INT(note.duration_ms, 12500);
    CHECK_INT(note.addition_count, 0);
    cap_note_free(&note);

    // a source already there stays, and old text goes
    note = note_of(AT, 4, 1000, "Old", "Old", "Old", "phone");
    CHECK(cap_note_set_answer(&note, &answer));
    CHECK_STR(note.title, "A title");
    CHECK_STR(note.summary, "- one\n- two");
    CHECK_STR(note.transcript, "Words said.");
    CHECK_STR(note.source, "phone");
    CHECK_INT(note.num, 4);
    cap_note_free(&note);
}

static void nothing_heard_is(int64_t duration_ms, const char *summary)
{
    cap_note_t note = note_of(AT, 0, duration_ms, NULL, NULL, NULL, NULL);
    cap_answer_t answer = {"A title for silence", "A summary of silence.", " \n" NBSP};
    CHECK(cap_note_set_answer(&note, &answer));
    CHECK_STR(note.title, CAP_NOTHING_HEARD);
    CHECK_STR(note.title, "Nothing heard");
    CHECK_STR(note.summary, summary);
    CHECK_STR(note.transcript, "");
    CHECK_STR(note.source, "clip");
    cap_note_free(&note);
}

// A blank transcript. The duration is formatDuration's m:ss.
static void set_answer_nothing_heard(void)
{
    nothing_heard_is(67900, "No clear speech in this 1:07 recording.");
    nothing_heard_is(0, "No clear speech in this 0:00 recording.");
    nothing_heard_is(999, "No clear speech in this 0:00 recording.");
    nothing_heard_is(9023, "No clear speech in this 0:09 recording.");
    nothing_heard_is(59999, "No clear speech in this 0:59 recording.");
    nothing_heard_is(60000, "No clear speech in this 1:00 recording.");
    nothing_heard_is(600000, "No clear speech in this 10:00 recording.");
    nothing_heard_is(3723000, "No clear speech in this 62:03 recording."); // no hours
    nothing_heard_is(CAP_NO_DURATION, "No clear speech in this recording.");

    cap_note_t note = note_of(AT, 0, 5000, NULL, NULL, NULL, NULL);
    cap_answer_t answer = {NULL, NULL, NULL};
    CHECK(cap_note_set_answer(&note, &answer));
    CHECK_STR(note.title, "Nothing heard");
    CHECK_STR(note.summary, "No clear speech in this 0:05 recording.");
    CHECK_STR(note.transcript, "");
    cap_note_free(&note);
}

static void title_is(const char *title, const char *summary, const char *expected)
{
    cap_note_t note = note_of(AT, 0, 5000, NULL, NULL, NULL, NULL);
    cap_answer_t answer = {(char *)title, (char *)summary, "Words."};
    CHECK(cap_note_set_answer(&note, &answer));
    CHECK_STR(note.title, expected);
    cap_note_free(&note);
}

// A blank title: the summary's first 8 words (TextNote.titleFrom), then "Untitled".
static void set_answer_title_fallback(void)
{
    title_is(" \n", "one two three four five six seven eight nine ten", "one two three four five six seven eight");
    title_is(NULL, "- one two\n- three\tfour   five\r\nsix seven eight nine", "one two three four five six seven eight");
    title_is("", "- just three words", "just three words");
    title_is("", "-one - two -- - -", "-one two --"); // only a lone "-" is a bullet
    title_is("", "a" NBSP "b c", "a" NBSP "b c");     // \s is ASCII: U+00A0 does not part words
    title_is("", "", "Untitled");
    title_is("", " \n ", "Untitled");
    title_is("", NULL, "Untitled");
    title_is("", "- -\n-", "Untitled");
    title_is("Given", "one two", "Given");
    // the transcript is never the fallback for a spoken note
    cap_note_t note = note_of(AT, 0, 5000, NULL, NULL, NULL, NULL);
    cap_answer_t answer = {"", "", "Words that were said."};
    CHECK(cap_note_set_answer(&note, &answer));
    CHECK_STR(note.title, "Untitled");
    CHECK_STR(note.summary, "");
    CHECK_STR(note.transcript, "Words that were said.");
    cap_note_free(&note);
}

// ---- adding a recording (Repository.saveAddition, fold) ------------------------------------------

static void add_answer(void)
{
    cap_note_t note = note_of(AT, 7, 42000, "Old title", "Old summary.", "Original.", "phone");
    cap_answer_t answer = {" New title ", "\nNew summary.\n", " Spoken more. \n"};
    CHECK_INT(cap_note_add_answer(&note, SPOKEN_ID, AT + 7200, 18341, &answer), CAP_ADDED);
    CHECK_STR(note.title, "New title");
    CHECK_STR(note.summary, "New summary.");
    CHECK_STR(note.transcript, "Original.");
    CHECK_STR(note.source, "phone"); // the note stays whose it was
    CHECK_INT(note.num, 7);
    CHECK_INT(note.duration_ms, 42000);
    CHECK_INT(note.addition_count, 1);
    if (note.addition_count == 1) {
        CHECK_STR(note.additions[0].id, SPOKEN_ID);
        CHECK_INT(note.additions[0].created_s, AT + 7200);
        CHECK_STR(note.additions[0].source, "clip");
        CHECK_INT(note.additions[0].duration_ms, 18341);
        CHECK_STR(note.additions[0].text, "Spoken more.");
    }
    RENDERS(ID, &note,
            "---\nid: " ID "\ncreated: 2026-09-19T09:00:00Z\nnum: 7\nduration_ms: 42000\nsource: phone\n---\n\n"
            "# New title\n\nNew summary.\n\n## Transcript\n\nOriginal.\n"
            "\n## Added\n<!-- addition " SPOKEN_ID " created=2026-09-19T11:00:00Z source=clip duration_ms=18341 -->\n\n"
            "Spoken more.\n");

    // the same id again is a repeated try: nothing changes
    cap_answer_t other = {"Other title", "Other summary.", "Other words."};
    CHECK_INT(cap_note_add_answer(&note, SPOKEN_ID, AT + 9000, 1, &other), CAP_ADDED);
    CHECK_INT(note.addition_count, 1);
    CHECK_STR(note.title, "New title");
    CHECK_STR(note.summary, "New summary.");
    if (note.addition_count == 1) {
        CHECK_STR(note.additions[0].text, "Spoken more.");
        CHECK_INT(note.additions[0].created_s, AT + 7200);
    }

    // a second one goes after the first
    CHECK_INT(cap_note_add_answer(&note, TYPED_ID, AT + 9000, 2000, &other), CAP_ADDED);
    CHECK_INT(note.addition_count, 2);
    if (note.addition_count == 2) {
        CHECK_STR(note.additions[0].id, SPOKEN_ID);
        CHECK_STR(note.additions[1].id, TYPED_ID);
        CHECK_STR(note.additions[1].text, "Other words.");
    }
    CHECK_STR(note.title, "Other title");
    cap_note_free(&note);
}

static void add_answer_nothing_heard(void)
{
    cap_note_t note = note_of(AT, 7, 42000, "Old title", "Old summary.", "Original.", "phone");
    cap_answer_t blank[] = {{"New title", "New summary.", ""}, {"New title", "New summary.", " \n" NBSP}, {"New title", "New summary.", NULL}};
    for (size_t i = 0; i < sizeof blank / sizeof *blank; i++) {
        CHECK_INT(cap_note_add_answer(&note, SPOKEN_ID, AT + 7200, 18341, &blank[i]), CAP_ADD_NOTHING_HEARD);
    }
    CHECK_INT(note.addition_count, 0);
    CHECK_STR(note.title, "Old title");
    CHECK_STR(note.summary, "Old summary.");
    CHECK_STR(note.transcript, "Original.");
    cap_note_free(&note);
}

static void add_answer_blank_title_and_summary(void)
{
    cap_note_t note = note_of(AT, 7, 42000, "Old title", "Old summary.", "Original.", "phone");
    cap_answer_t answer = {" \n", NULL, "More."};
    CHECK_INT(cap_note_add_answer(&note, SPOKEN_ID, AT + 7200, 18341, &answer), CAP_ADDED);
    CHECK_STR(note.title, "Old title");
    CHECK_STR(note.summary, "Old summary.");
    CHECK_INT(note.addition_count, 1);

    answer = (cap_answer_t){"New title", "", "Even more."};
    CHECK_INT(cap_note_add_answer(&note, TYPED_ID, AT + 7300, 1000, &answer), CAP_ADDED);
    CHECK_STR(note.title, "New title");
    CHECK_STR(note.summary, "Old summary.");

    answer = (cap_answer_t){"", "New summary.", "And more."};
    CHECK_INT(cap_note_add_answer(&note, "aaaaaaaaaaaaaaaa0000000000000004", AT + 7400, 1000, &answer), CAP_ADDED);
    CHECK_STR(note.title, "New title");
    CHECK_STR(note.summary, "New summary.");
    CHECK_INT(note.addition_count, 3);
    cap_note_free(&note);
}

// A short typed note from before 0.8.0 is only a title: its text becomes the transcript.
static void add_answer_to_an_old_typed_note(void)
{
    cap_note_t note = parsed("---\ncreated: 2026-09-19T09:00:00Z\nsource: phone\n---\n\n# water the basil\n\n## Transcript\n");
    cap_answer_t answer = {"Basil and mint", "- Water both.", "and the mint"};
    CHECK_INT(cap_note_add_answer(&note, SPOKEN_ID, AT + 60, 3000, &answer), CAP_ADDED);
    CHECK_STR(note.title, "Basil and mint");
    CHECK_STR(note.transcript, "water the basil");
    RENDERS(ID, &note,
            "---\nid: " ID "\ncreated: 2026-09-19T09:00:00Z\nsource: phone\n---\n\n# Basil and mint\n\n- Water both.\n\n"
            "## Transcript\n\nwater the basil\n"
            "\n## Added\n<!-- addition " SPOKEN_ID " created=2026-09-19T09:01:00Z source=clip duration_ms=3000 -->\n\n"
            "and the mint\n");
    cap_note_free(&note);

    // a spoken note in which nothing was heard keeps its empty transcript
    note = note_of(AT, 0, 4000, "Nothing heard", "No clear speech in this 0:04 recording.", "", "clip");
    CHECK_INT(cap_note_add_answer(&note, SPOKEN_ID, AT + 60, 3000, &answer), CAP_ADDED);
    CHECK_STR(note.transcript, "");
    CHECK_STR(note.title, "Basil and mint");
    cap_note_free(&note);
}

// ---- merge (Additions.mergeNeeded, the laptop's merge_note) --------------------------------------

#define A1 "a1000000000000000000000000000000"
#define A2 "a2000000000000000000000000000000"
#define B1 "b1000000000000000000000000000000"
#define B2 "b2000000000000000000000000000000"
#define C1 "c1000000000000000000000000000000"

// mergeOnlyWhenLocalHasAdditionsTheFileLacks: ours {a, b} or {b} against GitHub's {a} merge; ours
// {a} against {a, c} does not. (Its last case, nothing waiting to go up, is the caller's to know.)
static bool merges(const char *ours_ids[], size_t ours_count, const char *remote_ids[], size_t remote_count)
{
    cap_note_t ours = note_of(AT, 1, 1000, "Ours", "Our summary.", "Original.", "phone");
    cap_note_t remote = note_of(AT, 1, 1000, "Theirs", "Their summary.", "Original.", "phone");
    for (size_t i = 0; i < ours_count; i++) {
        add(&ours, ours_ids[i], AT + 10, "clip", 1000, "ours");
    }
    for (size_t i = 0; i < remote_count; i++) {
        add(&remote, remote_ids[i], AT + 10, "phone", 1000, "theirs");
    }
    cap_merge_t result = cap_note_merge(&remote, &ours);
    CHECK(result == CAP_MERGED || result == CAP_REMOTE_HAS_ALL);
    if (result == CAP_REMOTE_HAS_ALL) { // untouched
        CHECK_STR(remote.title, "Theirs");
        CHECK_STR(remote.summary, "Their summary.");
        CHECK_INT(remote.addition_count, remote_count);
    } else {
        CHECK_STR(remote.title, "Ours");
        CHECK_STR(remote.summary, "Our summary.");
    }
    cap_note_free(&ours);
    cap_note_free(&remote);
    return result == CAP_MERGED;
}

static void merge_only_when_ours_has_additions_the_file_lacks(void)
{
    const char *ab[] = {A1, B1}, *a[] = {A1}, *b[] = {B1}, *ac[] = {A1, C1};
    CHECK(merges(ab, 2, a, 1));
    CHECK(merges(b, 1, a, 1));
    CHECK(merges(a, 1, NULL, 0));
    // The file already has all of ours: another device merged them. It wins.
    CHECK(!merges(a, 1, ac, 2));
    CHECK(!merges(a, 1, a, 1));
    CHECK(!merges(NULL, 0, a, 1));
    CHECK(!merges(NULL, 0, NULL, 0));
}

static void merge_union_is_sorted(void)
{
    cap_note_t remote = note_of(AT, 9, 42000, "Their title", "Their summary.", "Their original.", "phone");
    add(&remote, A1, AT + 3600, "phone", CAP_NO_DURATION, "in both, their text");
    add(&remote, C1, AT + 7200, "laptop", CAP_NO_DURATION, "theirs, at noon");
    add(&remote, B2, AT + 9000, "laptop", 4000, "theirs, last");
    cap_note_t ours = note_of(AT, 9, 42000, "Our title", "Our summary.", "Our original.", "phone");
    add(&ours, A1, AT + 3600, "phone", CAP_NO_DURATION, "in both, our text");
    add(&ours, B1, AT + 7200, "clip", 18341, "ours, at noon too"); // same time as C1: by id
    add(&ours, A2, AT + 5400, "clip", 2000, "ours, between");

    CHECK_INT(cap_note_merge(&remote, &ours), CAP_MERGED);
    // GitHub's file is the base; the title and summary made here are kept
    cap_note_t want = note_of(AT, 9, 42000, "Our title", "Our summary.", "Their original.", "phone");
    add(&want, A1, AT + 3600, "phone", CAP_NO_DURATION, "in both, their text");
    add(&want, A2, AT + 5400, "clip", 2000, "ours, between");
    add(&want, B1, AT + 7200, "clip", 18341, "ours, at noon too");
    add(&want, C1, AT + 7200, "laptop", CAP_NO_DURATION, "theirs, at noon");
    add(&want, B2, AT + 9000, "laptop", 4000, "theirs, last");
    SAME_NOTE(&remote, &want);
    RENDERS(ID, &remote,
            "---\nid: " ID "\ncreated: 2026-09-19T09:00:00Z\nnum: 9\nduration_ms: 42000\nsource: phone\n---\n\n"
            "# Our title\n\nOur summary.\n\n## Transcript\n\nTheir original.\n"
            "\n## Added\n<!-- addition " A1 " created=2026-09-19T10:00:00Z source=phone -->\n\nin both, their text\n"
            "\n## Added\n<!-- addition " A2 " created=2026-09-19T10:30:00Z source=clip duration_ms=2000 -->\n\nours, between\n"
            "\n## Added\n<!-- addition " B1 " created=2026-09-19T11:00:00Z source=clip duration_ms=18341 -->\n\nours, at noon too\n"
            "\n## Added\n<!-- addition " C1 " created=2026-09-19T11:00:00Z source=laptop -->\n\ntheirs, at noon\n"
            "\n## Added\n<!-- addition " B2 " created=2026-09-19T11:30:00Z source=laptop duration_ms=4000 -->\n\ntheirs, last\n");
    // ours is not touched, and shares nothing with the result
    CHECK_INT(ours.addition_count, 3);
    CHECK_STR(ours.additions[1].text, "ours, at noon too");
    cap_note_free(&ours);
    CHECK_STR(remote.additions[2].text, "ours, at noon too");
    CHECK_STR(remote.additions[2].source, "clip");
    cap_note_free(&remote);
    cap_note_free(&want);

    // GitHub's own additions are sorted too (jq's sort_by takes them all)
    remote = note_of(AT, 0, 1000, "T", "", "O.", "phone");
    add(&remote, C1, AT + 300, "phone", 1, "third");
    add(&remote, B1, AT + 100, "phone", 1, "first");
    ours = note_of(AT, 0, 1000, "T", "", "O.", "phone");
    add(&ours, A1, AT + 200, "clip", 1, "second");
    CHECK_INT(cap_note_merge(&remote, &ours), CAP_MERGED);
    CHECK_INT(remote.addition_count, 3);
    if (remote.addition_count == 3) {
        CHECK_STR(remote.additions[0].text, "first");
        CHECK_STR(remote.additions[1].text, "second");
        CHECK_STR(remote.additions[2].text, "third");
    }
    cap_note_free(&ours);
    cap_note_free(&remote);
}

static void merge_remote_has_all(void)
{
    cap_note_t remote = note_of(AT, 9, 42000, "Their title", "Their summary.", "", NULL);
    add(&remote, C1, AT + 7200, "laptop", CAP_NO_DURATION, "theirs");
    add(&remote, A1, AT + 3600, "clip", 1000, "ours, as they have it");
    cap_note_t ours = note_of(AT, 3, 42000, "Our title", "Our summary.", "Our original.", "clip");
    add(&ours, A1, AT + 3600, "clip", 1000, "ours");

    CHECK_INT(cap_note_merge(&remote, &ours), CAP_REMOTE_HAS_ALL);
    // untouched: not the title, not the order, not the fields it lacks
    cap_note_t want = note_of(AT, 9, 42000, "Their title", "Their summary.", "", NULL);
    add(&want, C1, AT + 7200, "laptop", CAP_NO_DURATION, "theirs");
    add(&want, A1, AT + 3600, "clip", 1000, "ours, as they have it");
    SAME_NOTE(&remote, &want);
    cap_note_free(&want);
    cap_note_free(&ours);
    cap_note_free(&remote);
}

// GitHub's file is the base, but what it lacks comes from here: a transcript with no text (the
// laptop's test("\\S")), the number, the source.
static void merge_fallbacks(void)
{
    cap_note_t remote = note_of(AT + 5, 0, CAP_NO_DURATION, "Their title", "Their summary.", " \n", NULL);
    cap_note_t ours = note_of(AT, 12, 42000, "Our title", "Our summary.", "Our original.", "clip");
    add(&ours, A1, AT + 3600, "clip", 1000, "ours");
    CHECK_INT(cap_note_merge(&remote, &ours), CAP_MERGED);
    // `created` and the duration are GitHub's, even when it has none
    cap_note_t want = note_of(AT + 5, 12, CAP_NO_DURATION, "Our title", "Our summary.", "Our original.", "clip");
    add(&want, A1, AT + 3600, "clip", 1000, "ours");
    SAME_NOTE(&remote, &want);
    cap_note_free(&want);
    cap_note_free(&remote);
    cap_note_free(&ours);

    // and what it has stays
    remote = note_of(AT, 8, 5000, "Their title", "Their summary.", "Their original.", "laptop");
    ours = note_of(AT, 12, 42000, "Our title", "", "Our original.", "clip");
    add(&ours, A1, AT + 3600, "clip", 1000, "ours");
    CHECK_INT(cap_note_merge(&remote, &ours), CAP_MERGED);
    want = note_of(AT, 8, 5000, "Our title", "", "Their original.", "laptop"); // a blank summary made here is kept too
    add(&want, A1, AT + 3600, "clip", 1000, "ours");
    SAME_NOTE(&remote, &want);
    cap_note_free(&want);
    cap_note_free(&remote);
    cap_note_free(&ours);

    // neither has a number, a source or a transcript
    remote = note_of(AT, 0, CAP_NO_DURATION, "Their title", "", NULL, NULL);
    ours = note_of(AT, 0, CAP_NO_DURATION, "Our title", NULL, NULL, NULL);
    add(&ours, A1, AT + 3600, "clip", 1000, NULL);
    CHECK_INT(cap_note_merge(&remote, &ours), CAP_MERGED);
    CHECK_INT(remote.num, 0);
    CHECK(remote.source == NULL);
    CHECK_STR(remote.title, "Our title");
    CHECK_STR(remote.summary, "");
    CHECK_STR(remote.transcript, "");
    CHECK_INT(remote.addition_count, 1);
    RENDERS(ID, &remote,
            "---\nid: " ID "\ncreated: 2026-09-19T09:00:00Z\nsource: phone\n---\n\n# Our title\n\n## Transcript\n"
            "\n## Added\n<!-- addition " A1 " created=2026-09-19T10:00:00Z source=clip duration_ms=1000 -->\n");
    cap_note_free(&remote);
    cap_note_free(&ours);
}

// ---- golden files --------------------------------------------------------------------------------

// The file's bytes (malloc'd), or NULL. A NUL inside would cut a note short: that reads as NULL too.
static char *read_file(const char *path)
{
    FILE *file = fopen(path, "rb");
    if (!file) {
        return NULL;
    }
    cap_buf_t out = {0};
    char block[4096];
    size_t n;
    while ((n = fread(block, 1, sizeof block, file)) > 0) {
        cap_buf_add(&out, block, n);
    }
    fclose(file);
    size_t len = out.len;
    char *text = cap_buf_take(&out);
    if (text && strlen(text) != len) {
        free(text);
        return NULL;
    }
    return text;
}

static char *fixture(const char *name)
{
    char path[1024];
    const char *slash = strrchr(__FILE__, '/');
    snprintf(path, sizeof path, "%.*s/fixtures/capture/%s", slash ? (int)(slash - __FILE__) : 1, slash ? __FILE__ : ".",
             name);
    char *text = read_file(path);
    if (!text) {
        fprintf(stderr, "%s:%d: cannot read %s\n", __FILE__, __LINE__, path);
        unit_test_failed = 1;
    }
    return text;
}

// render(note) is the file, parse(file) is the note, and render(parse(file)) is the file.
static void golden_checks(const char *name, const char *id, const cap_note_t *note)
{
    char *bytes = fixture(name);
    if (!bytes) {
        return;
    }
    renders_checks(id, note, bytes);
    cap_note_t back;
    CHECK(cap_note_parse(bytes, &back));
    same_note_checks(&back, note);
    renders_checks(id, &back, bytes);
    cap_note_free(&back);
    // the frontmatter's id is the one asked for
    char line[64];
    snprintf(line, sizeof line, "---\nid: %s\n", id);
    CHECK(strncmp(bytes, line, strlen(line)) == 0);
    free(bytes);
}

#define IS_GOLDEN(name, id, note) FROM_LINE(__LINE__, golden_checks(name, id, note))

#define FIXTURE_AT "2026-09-19T09:00:00Z"

static void golden_phone_list(void)
{
    cap_note_t note = note_of(AT, 41, 56634, "Bird feeder with a camera",
                              "- Put a small camera on the bird feeder.\n- Sort the pictures by species.",
                              "I want a bird feeder that takes a picture of every visitor. Later it could sort them by species.",
                              "phone");
    IS_GOLDEN("phone_list.md", "c0ffee00000000000000000000000001", &note);
    cap_note_free(&note);
}

// A typed addition: its mark has no duration_ms.
static void golden_phone_typed_addition(void)
{
    cap_note_t note = note_of(AT, 42, 9023, "Kite with lights", "- Build a kite with a strip of lights.\n- Power it from a coin cell.",
                              "A kite with lights on it, for flying at dusk.", "phone");
    add(&note, "add00000000000000000000000000001", AT + 20165, "phone", CAP_NO_DURATION, "A coin cell should be enough to power it.");
    IS_GOLDEN("phone_typed_addition.md", "c0ffee00000000000000000000000002", &note);
    cap_note_free(&note);
}

// A note typed on the laptop has no duration_ms.
static void golden_laptop_addition(void)
{
    cap_note_t note = note_of(AT, 43, CAP_NO_DURATION, "Label maker for jars",
                              "- Print labels for the spice jars.\n- Use the old thermal printer.", "Print labels for the spice jars.",
                              "laptop");
    add(&note, "add00000000000000000000000000002", AT + 156, "laptop", CAP_NO_DURATION, "Use the old thermal printer for it.");
    IS_GOLDEN("laptop_addition.md", "c0ffee00000000000000000000000003", &note);
    cap_note_free(&note);
}

static void golden_phone_paragraph(void)
{
    cap_note_t note = note_of(AT, 44, 20343, "Compost thermometer",
                              "A probe that reports how warm the compost heap is, so it is turned at the right time.",
                              "The compost heap should tell me how warm it is. Then I know when to turn it.", "phone");
    IS_GOLDEN("phone_paragraph.md", "c0ffee00000000000000000000000004", &note);
    cap_note_free(&note);
}

// What this device writes: a recording, its number, Gemini's answer.
static void golden_clip(void)
{
    cap_note_t note = note_of(AT, 45, 12500, NULL, NULL, NULL, NULL);
    cap_answer_t answer = {"Umbrella reminder by the door\n", "- A light by the door when rain is forecast.\n",
                           " A light by the door that comes on when it is going to rain.\n\nSo I take the umbrella.\n"};
    CHECK(cap_note_set_answer(&note, &answer));
    IS_GOLDEN("clip.md", "c0ffee00000000000000000000000005", &note);
    cap_note_free(&note);
}

static void golden_clip_nothing_heard(void)
{
    cap_note_t note = note_of(AT, 46, 67900, NULL, NULL, NULL, NULL);
    cap_answer_t answer = {"", "", ""};
    CHECK(cap_note_set_answer(&note, &answer));
    IS_GOLDEN("clip_nothing_heard.md", "c0ffee00000000000000000000000006", &note);
    cap_note_free(&note);
}

// A phone note this device spoke an addition to.
static void golden_clip_spoken_addition(void)
{
    cap_note_t note = parsed("---\nid: c0ffee00000000000000000000000007\ncreated: " FIXTURE_AT "\nnum: 47\nduration_ms: 31000\n"
                             "source: phone\n---\n\n# Plant shelf\n\n- A shelf for the plants by the window.\n\n"
                             "## Transcript\n\nA shelf for the plants by the window.\n");
    cap_answer_t answer = {"Plant shelf with a water gauge",
                           "- A shelf for the plants by the window.\n- A gauge shows when the tank is empty.",
                           "And a gauge that shows when the water tank is empty."};
    CHECK_INT(cap_note_add_answer(&note, "add00000000000000000000000000003", AT + 7200, 18341, &answer), CAP_ADDED);
    IS_GOLDEN("clip_spoken_addition.md", "c0ffee00000000000000000000000007", &note);
    cap_note_free(&note);
}

// The owner's own notes, where there are any: render(parse(file)) is the file. Only names and
// offsets are printed, never text.
static void local_notes_round_trip(void)
{
    const char *home = getenv("HOME");
    char dir_path[1024];
    snprintf(dir_path, sizeof dir_path, "%s/Ideas/notes", home ? home : "");
    DIR *dir = opendir(dir_path);
    if (!dir) {
        return;
    }
    int files = 0, wrong = 0;
    for (struct dirent *entry; (entry = readdir(dir)) != NULL;) {
        size_t name_len = strlen(entry->d_name);
        if (name_len < 4 || name_len > CAP_ID_LEN + 2 || strcmp(entry->d_name + name_len - 3, ".md") != 0) {
            continue;
        }
        char path[1400], id[CAP_ID_LEN + 3];
        snprintf(path, sizeof path, "%s/%s", dir_path, entry->d_name);
        snprintf(id, sizeof id, "%.*s", (int)(name_len - 3), entry->d_name);
        char *bytes = read_file(path);
        if (!bytes) {
            fprintf(stderr, "%s:%d: %s: cannot be read\n", __FILE__, __LINE__, entry->d_name);
            wrong++;
            continue;
        }
        files++;
        cap_note_t note;
        char *again = cap_note_parse(bytes, &note) ? cap_note_render(id, &note) : NULL;
        if (!again) {
            fprintf(stderr, "%s:%d: %s: not parsed, or no `created`\n", __FILE__, __LINE__, entry->d_name);
            wrong++;
        } else if (strcmp(again, bytes) != 0) {
            size_t at = 0;
            while (again[at] && again[at] == bytes[at]) {
                at++;
            }
            fprintf(stderr, "%s:%d: %s: differs at byte %zu\n", __FILE__, __LINE__, entry->d_name, at);
            wrong++;
        }
        free(again);
        cap_note_free(&note);
        free(bytes);
    }
    closedir(dir);
    printf("local notes: %d round-tripped, %d differ\n", files - wrong, wrong);
    CHECK_INT(wrong, 0);
}

int main(void)
{
    RUN(iso_format);
    RUN(iso_parse);
    RUN(iso_round_trip);
    RUN(trim_unicode);
    RUN(new_ids);
    RUN(golden_render);
    RUN(render_with_duration_blank_summary_and_transcript);
    RUN(render_edges);
    RUN(round_trip);
    RUN(typed_note_round_trip);
    RUN(crlf_and_bom);
    RUN(no_frontmatter);
    RUN(empty_file);
    RUN(unclosed_frontmatter_is_body);
    RUN(bad_frontmatter_values_are_null);
    RUN(frontmatter_edges);
    RUN(num_round_trip);
    RUN(bad_nums_are_null);
    RUN(mangled_headings);
    RUN(heading_edges);
    RUN(transcript_heading_inside_summary);
    RUN(golden_render_with_additions);
    RUN(additions_round_trip);
    RUN(source_is_kept);
    RUN(old_parser_sees_additions_in_transcript);
    RUN(empty_addition_and_no_transcript);
    RUN(malformed_addition_marks_are_text);
    RUN(addition_mark_edges);
    RUN(addition_mark_on_the_last_line);
    RUN(duplicate_addition_id_keeps_first);
    RUN(transcript_heading_inside_addition);
    RUN(original_is_the_transcript);
    RUN(old_short_typed_note_is_its_title);
    RUN(whole_text_joins_with_blank_lines);
    RUN(set_answer);
    RUN(set_answer_nothing_heard);
    RUN(set_answer_title_fallback);
    RUN(add_answer);
    RUN(add_answer_nothing_heard);
    RUN(add_answer_blank_title_and_summary);
    RUN(add_answer_to_an_old_typed_note);
    RUN(merge_only_when_ours_has_additions_the_file_lacks);
    RUN(merge_union_is_sorted);
    RUN(merge_remote_has_all);
    RUN(merge_fallbacks);
    RUN(golden_phone_list);
    RUN(golden_phone_typed_addition);
    RUN(golden_laptop_addition);
    RUN(golden_phone_paragraph);
    RUN(golden_clip);
    RUN(golden_clip_nothing_heard);
    RUN(golden_clip_spoken_addition);
    RUN(local_notes_round_trip);
    return unit_done(__FILE__);
}
