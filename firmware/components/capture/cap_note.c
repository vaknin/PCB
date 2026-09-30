// The note file (Capture's NoteFile.kt) and the rules for adding to a note (Additions.kt,
// Repository.saveResult/saveAddition/fold, the laptop's merge_note). Rendering is byte-exact;
// parsing is tolerant, because the files can be edited by hand.
#include "cap_internal.h"
#include "capture.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    const char *at;
    size_t len;
} span_t;

static char *dup_str(const char *text)
{
    return cap_strndup(text, strlen(text));
}

static void addition_free(cap_addition_t *addition)
{
    free(addition->source);
    free(addition->text);
}

void cap_note_free(cap_note_t *note)
{
    free(note->title);
    free(note->summary);
    free(note->transcript);
    free(note->source);
    for (size_t i = 0; i < note->addition_count; i++) {
        addition_free(&note->additions[i]);
    }
    free(note->additions);
    *note = (cap_note_t){.duration_ms = CAP_NO_DURATION};
}

void cap_answer_free(cap_answer_t *answer)
{
    free(answer->title);
    free(answer->summary);
    free(answer->transcript);
    *answer = (cap_answer_t){0};
}

void cap_new_id(const uint8_t random[16], char out[33])
{
    for (int i = 0; i < 16; i++) {
        snprintf(out + 2 * i, 3, "%02x", random[i]);
    }
}

char *cap_note_path(const char *id)
{
    cap_buf_t out = {0};
    cap_buf_str(&out, "notes/");
    cap_buf_str(&out, id);
    cap_buf_str(&out, ".md");
    return cap_buf_take(&out);
}

// ---- render --------------------------------------------------------------------------------------

// normalize(text).trim(): CRLF and CR become LF. With one_line, line breaks become spaces.
static void add_normalized(cap_buf_t *out, const char *text, bool one_line)
{
    if (!text) {
        return;
    }
    size_t len = strlen(text);
    cap_trim(&text, &len);
    for (size_t i = 0; i < len; i++) {
        char c = text[i];
        if (c == '\r') {
            if (i + 1 < len && text[i + 1] == '\n') {
                continue;
            }
            c = '\n';
        }
        if (c == '\n' && one_line) {
            c = ' ';
        }
        cap_buf_add(out, &c, 1);
    }
}

char *cap_note_render(const char *id, const cap_note_t *note)
{
    if (!note->has_created) {
        return NULL;
    }
    char iso[CAP_ISO_LEN];
    cap_buf_t out = {0};
    cap_iso_format(note->created_s, iso);
    cap_buf_str(&out, "---\nid: ");
    cap_buf_str(&out, id);
    cap_buf_str(&out, "\ncreated: ");
    cap_buf_str(&out, iso);
    cap_buf_str(&out, "\n");
    if (note->num > 0) {
        cap_buf_str(&out, "num: ");
        cap_buf_int(&out, note->num);
        cap_buf_str(&out, "\n");
    }
    if (note->duration_ms >= 0) {
        cap_buf_str(&out, "duration_ms: ");
        cap_buf_int(&out, note->duration_ms);
        cap_buf_str(&out, "\n");
    }
    cap_buf_str(&out, "source: ");
    cap_buf_str(&out, note->source && *note->source ? note->source : "phone");
    cap_buf_str(&out, "\n---\n\n# ");
    size_t before = out.len;
    add_normalized(&out, note->title, true);
    if (out.len == before) {
        cap_buf_str(&out, "Untitled");
    }
    cap_buf_str(&out, "\n\n");
    before = out.len;
    add_normalized(&out, note->summary, false);
    if (out.len != before) {
        cap_buf_str(&out, "\n\n");
    }
    cap_buf_str(&out, "## Transcript\n");
    if (!cap_is_blank(note->transcript)) {
        cap_buf_str(&out, "\n");
        add_normalized(&out, note->transcript, false);
        cap_buf_str(&out, "\n");
    }
    for (size_t i = 0; i < note->addition_count; i++) {
        const cap_addition_t *addition = &note->additions[i];
        cap_iso_format(addition->created_s, iso);
        cap_buf_str(&out, "\n## Added\n<!-- addition ");
        cap_buf_str(&out, addition->id);
        cap_buf_str(&out, " created=");
        cap_buf_str(&out, iso);
        cap_buf_str(&out, " source=");
        cap_buf_str(&out, addition->source ? addition->source : "phone");
        if (addition->duration_ms >= 0) {
            cap_buf_str(&out, " duration_ms=");
            cap_buf_int(&out, addition->duration_ms);
        }
        cap_buf_str(&out, " -->\n");
        if (!cap_is_blank(addition->text)) {
            cap_buf_str(&out, "\n");
            add_normalized(&out, addition->text, false);
            cap_buf_str(&out, "\n");
        }
    }
    return cap_buf_take(&out);
}

// ---- parse ---------------------------------------------------------------------------------------

// Java's regex \s, which is ASCII only (Kotlin's trim is not).
static bool regex_space(char c)
{
    return c == ' ' || c == '\t' || c == '\n' || c == '\v' || c == '\f' || c == '\r';
}

static span_t trimmed(span_t line)
{
    cap_trim(&line.at, &line.len);
    return line;
}

static bool span_is(span_t span, const char *text)
{
    return span.len == strlen(text) && memcmp(span.at, text, span.len) == 0;
}

// ^##\s+<word>\s*$ without regard to case, on a trimmed line.
static bool is_heading(span_t line, const char *word)
{
    line = trimmed(line);
    size_t n = strlen(word);
    if (line.len < 3 + n || line.at[0] != '#' || line.at[1] != '#' || !regex_space(line.at[2])) {
        return false;
    }
    size_t i = 2;
    while (i < line.len && regex_space(line.at[i])) {
        i++;
    }
    if (line.len - i != n) {
        return false;
    }
    for (size_t k = 0; k < n; k++) {
        char c = line.at[i + k];
        if (c >= 'A' && c <= 'Z') {
            c = (char)(c - 'A' + 'a');
        }
        if (c != word[k]) {
            return false;
        }
    }
    return true;
}

typedef struct {
    span_t created, source, duration;
} mark_fields_t;

// ((?:\s+[a-z_]+=\S+)*)\s*-->$ from p, trying the longest value first, as the regex does. Of two
// fields with one name the last counts.
static bool mark_rest(const char *p, const char *end, mark_fields_t *fields)
{
    const char *q = p;
    while (q < end && regex_space(*q)) {
        q++;
    }
    if (q > p) {
        const char *key = q;
        while (q < end && ((*q >= 'a' && *q <= 'z') || *q == '_')) {
            q++;
        }
        if (q > key && q < end && *q == '=') {
            span_t name = {key, (size_t)(q - key)};
            const char *value = q + 1;
            const char *stop = value;
            while (stop < end && !regex_space(*stop)) {
                stop++;
            }
            for (; stop > value; stop--) {
                if (!mark_rest(stop, end, fields)) {
                    continue;
                }
                span_t text = {value, (size_t)(stop - value)};
                if (span_is(name, "created") && !fields->created.at) {
                    fields->created = text;
                } else if (span_is(name, "source") && !fields->source.at) {
                    fields->source = text;
                } else if (span_is(name, "duration_ms") && !fields->duration.at) {
                    fields->duration = text;
                }
                return true;
            }
        }
    }
    q = p;
    while (q < end && regex_space(*q)) {
        q++;
    }
    return end - q == 3 && memcmp(q, "-->", 3) == 0;
}

// ^<!--\s*addition\s+([0-9a-fA-F-]{16,64})((?:\s+[a-z_]+=\S+)*)\s*-->$ on a trimmed line.
static bool is_mark(span_t line, span_t *id, mark_fields_t *fields)
{
    line = trimmed(line);
    const char *p = line.at, *end = line.at + line.len;
    if (line.len < 4 || memcmp(p, "<!--", 4) != 0) {
        return false;
    }
    p += 4;
    while (p < end && regex_space(*p)) {
        p++;
    }
    if (end - p < 9 || memcmp(p, "addition", 8) != 0 || !regex_space(p[8])) {
        return false;
    }
    p += 8;
    while (p < end && regex_space(*p)) {
        p++;
    }
    size_t run = 0;
    while (p + run < end && run < 64 && (strchr("0123456789abcdefABCDEF-", p[run]) != NULL)) {
        run++;
    }
    for (; run >= 16; run--) {
        *fields = (mark_fields_t){0};
        if (mark_rest(p + run, end, fields)) {
            *id = (span_t){p, run};
            return true;
        }
    }
    return false;
}

static bool span_iso(span_t value, int64_t *epoch_s)
{
    char text[48];
    if (!value.at || value.len >= sizeof text) {
        return false;
    }
    memcpy(text, value.at, value.len);
    text[value.len] = 0;
    return cap_iso_parse(text, epoch_s);
}

// The lines first..last (last not included) joined and trimmed.
static char *lines_text(const span_t *lines, size_t first, size_t last)
{
    if (first >= last) {
        return cap_strndup("", 0);
    }
    span_t all = {lines[first].at, (size_t)(lines[last - 1].at + lines[last - 1].len - lines[first].at)};
    all = trimmed(all);
    return cap_strndup(all.at, all.len);
}

bool cap_note_parse(const char *input, cap_note_t *note)
{
    *note = (cap_note_t){.duration_ms = CAP_NO_DURATION};
    if (strncmp(input, "\xef\xbb\xbf", 3) == 0) {
        input += 3;
    }
    size_t input_len = strlen(input);
    char *text = malloc(input_len + 1);
    if (!text) {
        return false;
    }
    size_t len = 0, line_count = 1;
    for (size_t i = 0; i < input_len; i++) {
        char c = input[i];
        if (c == '\r') {
            if (input[i + 1] == '\n') {
                continue;
            }
            c = '\n';
        }
        line_count += c == '\n';
        text[len++] = c;
    }
    text[len] = 0;
    span_t *all_lines = malloc(line_count * sizeof *all_lines);
    size_t *marks = malloc(line_count * sizeof *marks);
    bool ok = all_lines && marks;
    span_t *lines = all_lines;
    size_t mark_count = 0;
    if (!ok) {
        goto done;
    }
    size_t n = 0;
    for (const char *p = text;; n++) {
        const char *nl = strchr(p, '\n');
        lines[n] = (span_t){p, nl ? (size_t)(nl - p) : strlen(p)};
        if (!nl) {
            n++;
            break;
        }
        p = nl + 1;
    }

    // frontmatter: `key: value` lines between two `---` lines
    span_t created = {0}, num = {0}, duration = {0}, source = {0};
    span_t first = lines[0];
    cap_trim_end(first.at, &first.len);
    if (span_is(first, "---")) {
        for (size_t end = 1; end < n; end++) {
            span_t line = lines[end];
            cap_trim_end(line.at, &line.len);
            if (!span_is(line, "---")) {
                continue;
            }
            for (size_t i = 1; i < end; i++) {
                const char *colon = memchr(lines[i].at, ':', lines[i].len);
                if (!colon || colon == lines[i].at) {
                    continue;
                }
                span_t key = trimmed((span_t){lines[i].at, (size_t)(colon - lines[i].at)});
                span_t value = trimmed((span_t){colon + 1, (size_t)(lines[i].at + lines[i].len - colon - 1)});
                if (span_is(key, "created")) {
                    created = value;
                } else if (span_is(key, "num")) {
                    num = value;
                } else if (span_is(key, "duration_ms")) {
                    duration = value;
                } else if (span_is(key, "source")) {
                    source = value;
                }
            }
            lines += end + 1;
            n -= end + 1;
            break;
        }
    }

    // title: the first `# ` line, else the first line that is not blank or the transcript heading
    size_t body = 0; // the line after the title
    span_t title = {"", 0};
    size_t at;
    for (at = 0; at < n; at++) {
        if (lines[at].len >= 2 && lines[at].at[0] == '#' && lines[at].at[1] == ' ') {
            title = trimmed((span_t){lines[at].at + 2, lines[at].len - 2});
            body = at + 1;
            break;
        }
    }
    if (at == n) {
        for (at = 0; at < n; at++) {
            span_t line = trimmed(lines[at]);
            if (line.len && !is_heading(line, "transcript")) {
                while (line.len && *line.at == '#') {
                    line.at++;
                    line.len--;
                }
                title = trimmed(line);
                body = at + 1;
                break;
            }
        }
    }

    // additions: a `## Added` heading with its mark on the next line
    for (at = body; at + 1 < n; at++) {
        span_t id;
        mark_fields_t fields;
        int64_t created_s;
        if (!is_heading(lines[at], "added") || !is_mark(lines[at + 1], &id, &fields) || !span_iso(fields.created, &created_s)) {
            continue;
        }
        marks[mark_count++] = at;
        bool duplicate = false; // of two additions with one id, the first is kept
        for (size_t i = 0; i < note->addition_count; i++) {
            duplicate |= span_is(id, note->additions[i].id);
        }
        if (duplicate) {
            continue;
        }
        cap_addition_t *grown = realloc(note->additions, (note->addition_count + 1) * sizeof *grown);
        if (!grown) {
            ok = false;
            goto done;
        }
        note->additions = grown;
        cap_addition_t *addition = &grown[note->addition_count++];
        *addition = (cap_addition_t){.created_s = created_s, .duration_ms = CAP_NO_DURATION};
        memcpy(addition->id, id.at, id.len);
        long long ms;
        if (fields.duration.at && cap_parse_long(fields.duration.at, fields.duration.len, &ms) && ms >= 0) {
            addition->duration_ms = ms;
        }
        addition->source = fields.source.at ? cap_strndup(fields.source.at, fields.source.len) : dup_str("phone");
        ok = addition->source != NULL;
        if (!ok) {
            goto done;
        }
    }
    // an addition's text runs to the next mark, which is known only now
    for (size_t m = 0; m < mark_count; m++) {
        span_t id;
        mark_fields_t fields;
        is_mark(lines[marks[m] + 1], &id, &fields);
        for (size_t a = 0; a < note->addition_count; a++) {
            cap_addition_t *addition = &note->additions[a];
            if (span_is(id, addition->id) && !addition->text) { // a later copy of an id has its text already
                addition->text = lines_text(lines, marks[m] + 2, m + 1 < mark_count ? marks[m + 1] : n);
                ok = ok && addition->text;
            }
        }
    }
    if (!ok) {
        goto done;
    }

    size_t body_end = mark_count ? marks[0] : n;
    size_t heading = body_end; // the last transcript heading before the first addition
    for (at = body_end; at > body; at--) {
        if (is_heading(lines[at - 1], "transcript")) {
            heading = at - 1;
            break;
        }
    }
    note->title = title.len ? cap_strndup(title.at, title.len) : dup_str("Untitled");
    note->summary = lines_text(lines, body, heading);
    note->transcript = lines_text(lines, heading + 1, body_end);
    ok = ok && note->title && note->summary && note->transcript;
    note->has_created = span_iso(created, &note->created_s);
    long long value;
    // num: ^[1-9][0-9]{0,8}$
    if (num.len >= 1 && num.len <= 9 && num.at[0] != '0' && num.at[0] != '+' && num.at[0] != '-' &&
        cap_parse_long(num.at, num.len, &value)) {
        note->num = (int)value;
    }
    if (duration.at && cap_parse_long(duration.at, duration.len, &value) && value >= 0) {
        note->duration_ms = value;
    }
    if (source.len) {
        note->source = cap_strndup(source.at, source.len);
        ok = ok && note->source;
    }

done:
    free(text);
    free(all_lines);
    free(marks);
    if (!ok) {
        cap_note_free(note);
    }
    return ok;
}

// ---- adding to a note ----------------------------------------------------------------------------

// Additions.original. The clip cannot see the phone's `typed` flag: a note with no transcript and
// no duration counts as one typed before Capture 0.8.0, whose title is its text (INFERRED).
static char *original(const cap_note_t *note)
{
    if (cap_is_blank(note->transcript) && note->duration_ms < 0) {
        return cap_trimmed(note->title);
    }
    return cap_trimmed(note->transcript);
}

char *cap_note_whole_text(const cap_note_t *note)
{
    cap_buf_t out = {0};
    char *own = original(note);
    if (!own) {
        return NULL;
    }
    cap_buf_str(&out, own);
    free(own);
    for (size_t i = 0; i < note->addition_count; i++) {
        const char *text = note->additions[i].text ? note->additions[i].text : "";
        size_t len = strlen(text);
        cap_trim(&text, &len);
        if (!len) {
            continue;
        }
        if (out.len) {
            cap_buf_str(&out, "\n\n");
        }
        cap_buf_add(&out, text, len);
    }
    return cap_buf_take(&out);
}

// TextNote.titleFrom: the first 8 words, else "Untitled". A lone "-" is a list bullet, not a word.
static char *title_from(const char *text)
{
    cap_buf_t out = {0};
    int words = 0;
    for (const char *p = text; *p && words < 8;) {
        while (*p && regex_space(*p)) {
            p++;
        }
        const char *word = p;
        while (*p && !regex_space(*p)) {
            p++;
        }
        if (p == word || (p - word == 1 && *word == '-')) {
            continue;
        }
        if (words++) {
            cap_buf_str(&out, " ");
        }
        cap_buf_add(&out, word, (size_t)(p - word));
    }
    if (!words) {
        cap_buf_str(&out, "Untitled");
    }
    return cap_buf_take(&out);
}

static void replace(char **field, char *value)
{
    free(*field);
    *field = value;
}

bool cap_note_set_answer(cap_note_t *note, const cap_answer_t *answer)
{
    char *transcript = cap_trimmed(answer->transcript);
    char *summary = cap_trimmed(answer->summary);
    char *title = cap_trimmed(answer->title);
    if (transcript && summary && title && !*transcript) {
        char text[64];
        if (note->duration_ms >= 0) {
            long long seconds = note->duration_ms / 1000;
            snprintf(text, sizeof text, "No clear speech in this %lld:%02d recording.", seconds / 60, (int)(seconds % 60));
        } else {
            snprintf(text, sizeof text, "No clear speech in this recording.");
        }
        replace(&title, dup_str(CAP_NOTHING_HEARD));
        replace(&summary, dup_str(text));
    } else if (transcript && summary && title && !*title) {
        replace(&title, title_from(summary));
    }
    if (!note->source) {
        note->source = dup_str(CAP_SOURCE);
    }
    if (!transcript || !summary || !title || !note->source) {
        free(transcript);
        free(summary);
        free(title);
        return false;
    }
    replace(&note->title, title);
    replace(&note->summary, summary);
    replace(&note->transcript, transcript);
    return true;
}

static bool has_addition(const cap_note_t *note, const char *id)
{
    for (size_t i = 0; i < note->addition_count; i++) {
        if (strcmp(note->additions[i].id, id) == 0) {
            return true;
        }
    }
    return false;
}

cap_add_t cap_note_add_answer(cap_note_t *note, const char *addition_id, int64_t created_s, int64_t duration_ms,
                              const cap_answer_t *answer)
{
    if (cap_is_blank(answer->transcript)) {
        return CAP_ADD_NOTHING_HEARD;
    }
    if (has_addition(note, addition_id)) {
        return CAP_ADDED; // a repeated try
    }
    cap_addition_t addition = {.created_s = created_s, .duration_ms = duration_ms};
    snprintf(addition.id, sizeof addition.id, "%s", addition_id);
    addition.source = dup_str(CAP_SOURCE);
    addition.text = cap_trimmed(answer->transcript);
    char *transcript = original(note);
    char *title = cap_trimmed(answer->title);
    char *summary = cap_trimmed(answer->summary);
    cap_addition_t *grown = NULL;
    if (addition.source && addition.text && transcript && title && summary) {
        grown = realloc(note->additions, (note->addition_count + 1) * sizeof *grown);
    }
    if (!grown) {
        addition_free(&addition);
        free(transcript);
        free(title);
        free(summary);
        return CAP_ADD_NO_MEMORY;
    }
    note->additions = grown;
    grown[note->addition_count++] = addition;
    replace(&note->transcript, transcript);
    if (*title) {
        replace(&note->title, title);
    } else {
        free(title);
    }
    if (*summary) {
        replace(&note->summary, summary);
    } else {
        free(summary);
    }
    return CAP_ADDED;
}

cap_merge_t cap_note_merge(cap_note_t *remote, const cap_note_t *ours)
{
    size_t missing = 0;
    for (size_t i = 0; i < ours->addition_count; i++) {
        missing += !has_addition(remote, ours->additions[i].id);
    }
    if (!missing) {
        return CAP_REMOTE_HAS_ALL;
    }
    size_t count = remote->addition_count;
    cap_addition_t *all = realloc(remote->additions, (count + missing) * sizeof *all);
    char *title = dup_str(ours->title ? ours->title : "");
    char *summary = dup_str(ours->summary ? ours->summary : "");
    char *transcript = cap_is_blank(remote->transcript) ? dup_str(ours->transcript ? ours->transcript : "") : NULL;
    char *source = !remote->source && ours->source ? dup_str(ours->source) : NULL;
    bool ok = all && title && summary && (transcript || !cap_is_blank(remote->transcript)) &&
              (source || remote->source || !ours->source);
    if (all) {
        remote->additions = all;
    }
    for (size_t i = 0; ok && i < ours->addition_count; i++) {
        const cap_addition_t *from = &ours->additions[i];
        if (has_addition(remote, from->id)) {
            continue;
        }
        cap_addition_t copy = *from;
        copy.source = dup_str(from->source ? from->source : "phone");
        copy.text = dup_str(from->text ? from->text : "");
        if (!copy.source || !copy.text) {
            addition_free(&copy);
            ok = false;
            break;
        }
        all[remote->addition_count++] = copy;
    }
    if (!ok) {
        free(title);
        free(summary);
        free(transcript);
        free(source);
        return CAP_MERGE_NO_MEMORY;
    }
    // oldest first, then by id; equal ones keep their order (jq's sort_by)
    for (size_t i = 1; i < remote->addition_count; i++) {
        cap_addition_t moved = all[i];
        size_t at = i;
        for (; at > 0; at--) {
            const cap_addition_t *before = &all[at - 1];
            if (before->created_s < moved.created_s || (before->created_s == moved.created_s && strcmp(before->id, moved.id) <= 0)) {
                break;
            }
            all[at] = all[at - 1];
        }
        all[at] = moved;
    }
    replace(&remote->title, title);
    replace(&remote->summary, summary);
    if (transcript) {
        replace(&remote->transcript, transcript);
    }
    if (source) {
        remote->source = source;
    }
    if (remote->num <= 0) {
        remote->num = ours->num;
    }
    return CAP_MERGED;
}
