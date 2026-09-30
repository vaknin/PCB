// The queue of recordings on flash: one <id>.ogg per recording, its <id>.meta, and <id>.ans once
// Gemini has answered. Every change to a small file is a whole new file renamed over the old
// one, so after a power cut each file is either the old or the new version, and
// clip_queue_recover() puts the set of files right.
#include "clip.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define META_HEAD "clip-meta 1\n"
#define ANSWER_HEAD "clip-answer 1\n"
#define LAST_NAME "last"
#define OGG_HEADER 27

// ---- small things --------------------------------------------------------------------------------

// CRC-32 as zlib's.
static uint32_t crc32(const char *data, size_t len)
{
    uint32_t crc = 0xFFFFFFFFu;
    for (size_t i = 0; i < len; i++) {
        crc ^= (uint8_t)data[i];
        for (int bit = 0; bit < 8; bit++) {
            crc = (crc >> 1) ^ (0xEDB88320u & (0u - (crc & 1)));
        }
    }
    return ~crc;
}

static bool valid_id(const char *id)
{
    size_t len = strlen(id);
    if (len != CLIP_ID_LEN - 1) {
        return false;
    }
    return strspn(id, "0123456789abcdef") == len;
}

static void name_of(const char *id, const char *ext, char out[CLIP_NAME_LEN])
{
    snprintf(out, CLIP_NAME_LEN, "%s%s", id, ext);
}

void clip_audio_name(const char *id, char out[CLIP_NAME_LEN])
{
    name_of(id, ".ogg", out);
}

// `name` is "<id><ext>": copies the id out.
static bool id_of(const char *name, const char *ext, char id[CLIP_ID_LEN])
{
    size_t len = strlen(name), ext_len = strlen(ext);
    if (len != CLIP_ID_LEN - 1 + ext_len || strcmp(name + len - ext_len, ext) != 0) {
        return false;
    }
    memcpy(id, name, CLIP_ID_LEN - 1);
    id[CLIP_ID_LEN - 1] = 0;
    return valid_id(id);
}

static bool ends_with(const char *name, const char *ext)
{
    size_t len = strlen(name), ext_len = strlen(ext);
    return len >= ext_len && strcmp(name + len - ext_len, ext) == 0;
}

// Replaces `name` with `data`, all or nothing.
static bool save(const clip_store_t *store, const char *name, const char *data, size_t len)
{
    char fresh[CLIP_NAME_LEN + 8];
    snprintf(fresh, sizeof fresh, "%s.new", name);
    return store->write(store->ctx, fresh, data, len) && store->rename(store->ctx, fresh, name);
}

static bool exists(const clip_store_t *store, const char *name)
{
    return store->size(store->ctx, name) >= 0;
}

// ---- the .meta file ------------------------------------------------------------------------------

char *clip_meta_render(const clip_meta_t *meta)
{
    char error[sizeof meta->attempts.last_error];
    snprintf(error, sizeof error, "%s", meta->attempts.last_error);
    for (char *p = error; *p; p++) {
        if ((unsigned char)*p < 0x20) {
            *p = ' '; // one line
        }
    }
    size_t size = 768 + sizeof error;
    char *out = malloc(size);
    if (!out) {
        return NULL;
    }
    int len = snprintf(out, size - 16,
                       META_HEAD "id=%s\naddition=%s\nkind=%s\ntarget=%s\nstate=%s\nseq=%lu\ncreated=%lld\n"
                                 "boot=%lu\nuptime_ms=%lld\nduration_ms=%lld\naudio_bytes=%lld\nnum=%d\n"
                                 "not_before_ms=%lld\nsync_tries=%d\nattempts=%d\nerror=%s\n",
                       meta->id, meta->addition_id, meta->kind == CLIP_ADDITION ? "add" : "new", meta->target,
                       meta->state == CLIP_REC_RECORDING ? "recording" : meta->state == CLIP_REC_FAILED ? "failed" : "queued",
                       (unsigned long)meta->seq, (long long)meta->created_s, (unsigned long)meta->boot,
                       (long long)meta->uptime_ms, (long long)meta->duration_ms, (long long)meta->audio_bytes, meta->num,
                       (long long)meta->not_before_ms, meta->sync_tries, meta->attempts.attempts, error);
    snprintf(out + len, 16, "crc=%08lx\n", (unsigned long)crc32(out, (size_t)len));
    return out;
}

// The text before a closing "crc=xxxxxxxx\n" line that matches it; 0 when there is none.
static size_t checked_len(const char *text, size_t len)
{
    const size_t tail = 13;
    if (len < tail || text[len - 1] != '\n' || memcmp(text + len - tail, "crc=", 4) != 0) {
        return 0;
    }
    char digits[9] = {0}, *end;
    memcpy(digits, text + len - 9, 8);
    unsigned long crc = strtoul(digits, &end, 16);
    if (end != digits + 8 || crc != crc32(text, len - tail)) {
        return 0;
    }
    return len - tail;
}

static void copy_id(char out[CLIP_ID_LEN], const char *value, size_t len, bool *ok)
{
    if (len == 0) {
        out[0] = 0;
        return;
    }
    if (len != CLIP_ID_LEN - 1) {
        *ok = false;
        return;
    }
    memcpy(out, value, len);
    out[len] = 0;
    *ok &= valid_id(out);
}

bool clip_meta_parse(const char *text, size_t len, clip_meta_t *meta)
{
    *meta = (clip_meta_t){.audio_bytes = -1};
    size_t body = checked_len(text, len), head = strlen(META_HEAD);
    if (body < head || memcmp(text, META_HEAD, head) != 0) {
        return false;
    }
    bool ok = true;
    for (size_t at = head; at < body;) {
        const char *line = text + at, *end = memchr(line, '\n', body - at);
        if (!end) {
            return false;
        }
        at += (size_t)(end - line) + 1;
        const char *eq = memchr(line, '=', (size_t)(end - line));
        if (!eq) {
            return false;
        }
        size_t key_len = (size_t)(eq - line), value_len = (size_t)(end - eq - 1);
        const char *value = eq + 1;
        char number[24] = {0};
        memcpy(number, value, value_len < sizeof number - 1 ? value_len : sizeof number - 1);
        long long n = strtoll(number, NULL, 10);
#define KEY(name) (key_len == strlen(name) && memcmp(line, name, key_len) == 0)
        if (KEY("id")) {
            copy_id(meta->id, value, value_len, &ok);
        } else if (KEY("addition")) {
            copy_id(meta->addition_id, value, value_len, &ok);
        } else if (KEY("target")) {
            copy_id(meta->target, value, value_len, &ok);
        } else if (KEY("kind")) {
            meta->kind = value_len == 3 && memcmp(value, "add", 3) == 0 ? CLIP_ADDITION : CLIP_NEW;
        } else if (KEY("state")) {
            meta->state = value_len == 9 && memcmp(value, "recording", 9) == 0 ? CLIP_REC_RECORDING
                          : value_len == 6 && memcmp(value, "failed", 6) == 0  ? CLIP_REC_FAILED
                                                                               : CLIP_REC_QUEUED;
        } else if (KEY("seq")) {
            meta->seq = (uint32_t)n;
        } else if (KEY("created")) {
            meta->created_s = n;
        } else if (KEY("boot")) {
            meta->boot = (uint32_t)n;
        } else if (KEY("uptime_ms")) {
            meta->uptime_ms = n;
        } else if (KEY("duration_ms")) {
            meta->duration_ms = n;
        } else if (KEY("audio_bytes")) {
            meta->audio_bytes = n;
        } else if (KEY("num")) {
            meta->num = (int)n;
        } else if (KEY("not_before_ms")) {
            meta->not_before_ms = n;
        } else if (KEY("sync_tries")) {
            meta->sync_tries = (int)n;
        } else if (KEY("attempts")) {
            meta->attempts.attempts = (int)n;
        } else if (KEY("error")) {
            size_t max = sizeof meta->attempts.last_error - 1;
            memcpy(meta->attempts.last_error, value, value_len < max ? value_len : max);
        } // a key from a newer firmware is skipped
#undef KEY
    }
    // an addition must say what it adds to and under which id
    ok &= valid_id(meta->id);
    ok &= meta->kind != CLIP_ADDITION || (valid_id(meta->target) && valid_id(meta->addition_id));
    return ok;
}

bool clip_meta_save(const clip_store_t *store, const clip_meta_t *meta)
{
    char name[CLIP_NAME_LEN];
    name_of(meta->id, ".meta", name);
    char *text = clip_meta_render(meta);
    bool ok = text && save(store, name, text, strlen(text));
    free(text);
    return ok;
}

bool clip_meta_load(const clip_store_t *store, const char *id, clip_meta_t *meta)
{
    char name[CLIP_NAME_LEN], *text = NULL;
    size_t len = 0;
    name_of(id, ".meta", name);
    if (!store->read(store->ctx, name, &text, &len)) {
        return false;
    }
    bool ok = clip_meta_parse(text, len, meta) && strcmp(meta->id, id) == 0;
    free(text);
    return ok;
}

// ---- the .ans file -------------------------------------------------------------------------------

// Three blocks, each "<length>\n<bytes>\n": the texts can hold anything, so nothing is escaped.
char *clip_answer_render(const cap_answer_t *answer, size_t *len)
{
    const char *parts[3] = {answer->title, answer->summary, answer->transcript};
    size_t size = strlen(ANSWER_HEAD) + 16;
    for (int i = 0; i < 3; i++) {
        size += strlen(parts[i] ? parts[i] : "") + 24;
    }
    char *out = malloc(size);
    if (!out) {
        return NULL;
    }
    size_t at = (size_t)snprintf(out, size, ANSWER_HEAD);
    for (int i = 0; i < 3; i++) {
        const char *part = parts[i] ? parts[i] : "";
        size_t part_len = strlen(part);
        at += (size_t)snprintf(out + at, size - at, "%lu\n", (unsigned long)part_len);
        memcpy(out + at, part, part_len);
        at += part_len;
        out[at++] = '\n';
    }
    at += (size_t)snprintf(out + at, size - at, "crc=%08lx\n", (unsigned long)crc32(out, at));
    *len = at;
    return out;
}

bool clip_answer_parse(const char *text, size_t len, cap_answer_t *answer)
{
    *answer = (cap_answer_t){0};
    size_t body = checked_len(text, len), at = strlen(ANSWER_HEAD);
    if (body < at || memcmp(text, ANSWER_HEAD, at) != 0) {
        return false;
    }
    char **parts[3] = {&answer->title, &answer->summary, &answer->transcript};
    for (int i = 0; i < 3; i++) {
        size_t part_len = 0, digits = 0;
        while (at < body && text[at] >= '0' && text[at] <= '9' && digits++ < 9) {
            part_len = part_len * 10 + (size_t)(text[at++] - '0');
        }
        if (!digits || at >= body || text[at++] != '\n' || body - at < part_len + 1 || text[at + part_len] != '\n' ||
            memchr(text + at, 0, part_len)) {
            cap_answer_free(answer);
            return false;
        }
        *parts[i] = malloc(part_len + 1);
        if (!*parts[i]) {
            cap_answer_free(answer);
            return false;
        }
        memcpy(*parts[i], text + at, part_len);
        (*parts[i])[part_len] = 0;
        at += part_len + 1;
    }
    if (at != body) {
        cap_answer_free(answer);
        return false;
    }
    return true;
}

bool clip_answer_save(const clip_store_t *store, const char *id, const cap_answer_t *answer)
{
    char name[CLIP_NAME_LEN];
    size_t len = 0;
    name_of(id, ".ans", name);
    char *text = clip_answer_render(answer, &len);
    bool ok = text && save(store, name, text, len);
    free(text);
    return ok;
}

bool clip_answer_load(const clip_store_t *store, const char *id, cap_answer_t *answer)
{
    char name[CLIP_NAME_LEN], *text = NULL;
    size_t len = 0;
    name_of(id, ".ans", name);
    *answer = (cap_answer_t){0};
    if (!store->read(store->ctx, name, &text, &len)) {
        return false;
    }
    bool ok = clip_answer_parse(text, len, answer);
    free(text);
    return ok;
}

// ---- the audio -----------------------------------------------------------------------------------

static uint64_t le(const uint8_t *p, int bytes)
{
    uint64_t value = 0;
    for (int i = bytes - 1; i >= 0; i--) {
        value = value << 8 | p[i];
    }
    return value;
}

bool clip_ogg_scan(const clip_store_t *store, const char *name, int64_t *complete_bytes, int64_t *duration_ms)
{
    *complete_bytes = 0;
    *duration_ms = 0;
    int64_t size = store->size(store->ctx, name);
    void *file = size > 0 ? store->open(store->ctx, name) : NULL;
    if (!file) {
        return false;
    }
    int64_t at = 0, granule = 0, pre_skip = 0, audio_end = 0;
    for (int page = 0;; page++) {
        uint8_t head[OGG_HEADER + 255];
        if (size - at < OGG_HEADER || store->read_at(store->ctx, file, at, head, OGG_HEADER) != OGG_HEADER ||
            memcmp(head, "OggS", 4) != 0 || head[4] != 0) {
            break;
        }
        size_t segments = head[26];
        if (store->read_at(store->ctx, file, at + OGG_HEADER, head + OGG_HEADER, segments) != segments) {
            break;
        }
        int64_t body = 0;
        for (size_t i = 0; i < segments; i++) {
            body += head[OGG_HEADER + i];
        }
        int64_t end = at + OGG_HEADER + (int64_t)segments + body;
        if (end > size) {
            break; // the page the power cut fell in
        }
        if (page == 0) {
            // OpusHead: "OpusHead", version, channels, then the pre-skip
            uint8_t first[12];
            if (body >= 12 && store->read_at(store->ctx, file, at + OGG_HEADER + (int64_t)segments, first, 12) == 12 &&
                memcmp(first, "OpusHead", 8) == 0) {
                pre_skip = (int64_t)le(first + 10, 2);
            }
        }
        uint64_t position = le(head + 6, 8);
        // -1: no packet ends on this page. The two header pages have 0.
        if (position != UINT64_MAX && position > 0 && position < (uint64_t)INT64_MAX / 2) {
            granule = (int64_t)position;
            audio_end = end;
        }
        at = end;
    }
    store->close(store->ctx, file);
    if (audio_end == 0) {
        return false;
    }
    // Pages after the last one that ends a packet hold only the start of one: left out.
    *complete_bytes = audio_end;
    *duration_ms = granule > pre_skip ? (granule - pre_skip) / 48 : 0;
    return true;
}

// ---- the last note -------------------------------------------------------------------------------

bool clip_last_note(const clip_store_t *store, char id[CLIP_ID_LEN])
{
    char *text = NULL;
    size_t len = 0;
    id[0] = 0;
    if (!store->read(store->ctx, LAST_NAME, &text, &len)) {
        return false;
    }
    bool ok = len == CLIP_ID_LEN && text[CLIP_ID_LEN - 1] == '\n';
    if (ok) {
        memcpy(id, text, CLIP_ID_LEN - 1);
        id[CLIP_ID_LEN - 1] = 0;
        ok = valid_id(id);
    }
    if (!ok) {
        id[0] = 0;
    }
    free(text);
    return ok;
}

bool clip_set_last_note(const clip_store_t *store, const char *id)
{
    char text[CLIP_ID_LEN + 1];
    snprintf(text, sizeof text, "%s\n", id);
    return valid_id(id) && save(store, LAST_NAME, text, CLIP_ID_LEN);
}

// ---- the queue -----------------------------------------------------------------------------------

typedef struct {
    char **names;
    size_t count;
    bool failed;
} names_t;

static void add_name(void *arg, const char *name)
{
    names_t *names = arg;
    char **grown = realloc(names->names, (names->count + 1) * sizeof *grown);
    char *copy = grown ? malloc(strlen(name) + 1) : NULL;
    if (grown) {
        names->names = grown;
    }
    if (!copy) {
        names->failed = true;
        return;
    }
    strcpy(copy, name);
    names->names[names->count++] = copy;
}

static void names_free(names_t *names)
{
    for (size_t i = 0; i < names->count; i++) {
        free(names->names[i]);
    }
    free(names->names);
    *names = (names_t){0};
}

static bool list_names(const clip_store_t *store, names_t *names)
{
    *names = (names_t){0};
    if (!store->list(store->ctx, add_name, names) || names->failed) {
        names_free(names);
        return false;
    }
    return true;
}

// Every .meta that reads (malloc'd).
static bool load_all(const clip_store_t *store, clip_meta_t **metas, size_t *count)
{
    names_t names;
    *metas = NULL;
    *count = 0;
    if (!list_names(store, &names)) {
        return false;
    }
    clip_meta_t *list = calloc(names.count + 1, sizeof *list);
    if (!list) {
        names_free(&names);
        return false;
    }
    for (size_t i = 0; i < names.count; i++) {
        char id[CLIP_ID_LEN];
        if (id_of(names.names[i], ".meta", id) && clip_meta_load(store, id, &list[*count])) {
            (*count)++;
        }
    }
    names_free(&names);
    *metas = list;
    return true;
}

bool clip_queue_begin(const clip_store_t *store, const uint8_t random[32], const clip_clock_t *now, clip_meta_t *meta)
{
    clip_meta_t *metas;
    size_t count;
    if (!load_all(store, &metas, &count)) {
        return false;
    }
    *meta = (clip_meta_t){
        .kind = CLIP_NEW,
        .state = CLIP_REC_RECORDING,
        .seq = 1,
        .created_s = now->wall_ms / 1000,
        .boot = now->boot,
        .uptime_ms = now->uptime_ms,
        .audio_bytes = -1,
    };
    for (size_t i = 0; i < count; i++) {
        if (metas[i].seq >= meta->seq) {
            meta->seq = metas[i].seq + 1;
        }
    }
    free(metas);
    cap_new_id(random, meta->id);
    cap_new_id(random + 16, meta->addition_id);
    return clip_meta_save(store, meta);
}

bool clip_queue_mark_addition(const clip_store_t *store, clip_meta_t *meta)
{
    char last[CLIP_ID_LEN];
    if (!clip_last_note(store, last)) {
        return false;
    }
    clip_meta_t changed = *meta;
    changed.kind = CLIP_ADDITION;
    memcpy(changed.target, last, sizeof last);
    if (!clip_meta_save(store, &changed)) {
        return false; // it stays a new note, as its .meta on flash says
    }
    *meta = changed;
    return true;
}

bool clip_queue_finish(const clip_store_t *store, clip_meta_t *meta, int64_t duration_ms)
{
    // The last note first: cut between the two, the next boot's recovery comes here again. (The
    // other way round, a queued note could be left that the next hold does not add to.)
    if (meta->kind == CLIP_NEW && !clip_set_last_note(store, meta->id)) {
        return false;
    }
    meta->state = CLIP_REC_QUEUED;
    meta->duration_ms = duration_ms;
    return clip_meta_save(store, meta);
}

void clip_queue_drop(const clip_store_t *store, const char *id)
{
    // the .meta first: without it the rest is a leftover that recovery removes
    static const char *const ext[] = {".meta", ".ans", ".ogg"};
    for (size_t i = 0; i < sizeof ext / sizeof *ext; i++) {
        char name[CLIP_NAME_LEN];
        name_of(id, ext[i], name);
        if (!store->remove(store->ctx, name)) {
            return;
        }
    }
}

bool clip_queue_recover(const clip_store_t *store, clip_recovery_t *out)
{
    names_t names;
    *out = (clip_recovery_t){0};
    if (!list_names(store, &names)) {
        return false;
    }
    bool ok = true;
    for (size_t i = 0; i < names.count; i++) {
        if (ends_with(names.names[i], ".new")) {
            ok &= store->remove(store->ctx, names.names[i]);
        }
    }
    for (size_t i = 0; i < names.count && ok; i++) {
        char id[CLIP_ID_LEN], audio[CLIP_NAME_LEN];
        if (!id_of(names.names[i], ".meta", id)) {
            continue;
        }
        clip_audio_name(id, audio);
        clip_meta_t meta;
        bool reads = clip_meta_load(store, id, &meta);
        if (reads && meta.state != CLIP_REC_RECORDING) {
            continue;
        }
        int64_t bytes, duration_ms;
        if (!clip_ogg_scan(store, audio, &bytes, &duration_ms)) {
            clip_queue_drop(store, id);
            out->dropped++;
            continue;
        }
        if (!reads) {
            meta = (clip_meta_t){.kind = CLIP_NEW, .seq = 0}; // oldest: its place in the queue is not known
            memcpy(meta.id, id, sizeof id);
            memcpy(meta.addition_id, id, sizeof id);
            out->rebuilt++;
        } else {
            out->queued++;
        }
        meta.audio_bytes = bytes;
        ok &= clip_queue_finish(store, &meta, duration_ms);
    }
    for (size_t i = 0; i < names.count && ok; i++) {
        char id[CLIP_ID_LEN], meta_name[CLIP_NAME_LEN];
        if (!id_of(names.names[i], ".ogg", id) && !id_of(names.names[i], ".ans", id)) {
            continue;
        }
        name_of(id, ".meta", meta_name);
        if (!exists(store, meta_name) && exists(store, names.names[i])) {
            ok &= store->remove(store->ctx, names.names[i]);
            out->dropped++;
        }
    }
    names_free(&names);
    return ok;
}

bool clip_queue_status(const clip_store_t *store, const cap_gate_t *gate, int64_t now_ms, clip_queue_status_t *out)
{
    clip_meta_t *metas;
    size_t count;
    *out = (clip_queue_status_t){.wait_ms = -1};
    if (!load_all(store, &metas, &count)) {
        return false;
    }
    const clip_meta_t *first = NULL;
    int64_t soonest = INT64_MAX;
    for (size_t i = 0; i < count; i++) {
        const clip_meta_t *meta = &metas[i];
        out->failed += meta->state == CLIP_REC_FAILED;
        if (meta->state != CLIP_REC_QUEUED) {
            continue;
        }
        out->queued++;
        char answer[CLIP_NAME_LEN];
        name_of(meta->id, ".ans", answer);
        int64_t at = meta->not_before_ms;
        if (gate && gate->next_allowed_ms > at && !exists(store, answer)) {
            at = gate->next_allowed_ms;
        }
        bool blocked = false;
        for (size_t j = 0; j < count && meta->kind == CLIP_ADDITION; j++) {
            // its note is made first; that one's own wait is already counted
            blocked |= metas[j].state == CLIP_REC_QUEUED && strcmp(metas[j].id, meta->target) == 0;
        }
        if (blocked) {
            continue;
        }
        if (at < soonest) {
            soonest = at;
        }
        if (at <= now_ms && (!first || meta->seq < first->seq)) {
            first = meta;
        }
    }
    if (first) {
        out->due = true;
        memcpy(out->id, first->id, sizeof out->id);
        out->wait_ms = 0;
    } else if (soonest != INT64_MAX) {
        out->wait_ms = soonest - now_ms;
    }
    free(metas);
    return true;
}

int clip_queue_retry_failed(const clip_store_t *store)
{
    clip_meta_t *metas;
    size_t count;
    int retried = 0;
    if (!load_all(store, &metas, &count)) {
        return 0;
    }
    for (size_t i = 0; i < count; i++) {
        if (metas[i].state != CLIP_REC_FAILED) {
            continue;
        }
        metas[i].state = CLIP_REC_QUEUED;
        metas[i].attempts = (cap_attempts_t){0};
        metas[i].not_before_ms = 0;
        retried += clip_meta_save(store, &metas[i]);
    }
    free(metas);
    return retried;
}
