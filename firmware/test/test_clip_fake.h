// Stand-ins for the clip's hardware side in the laptop tests: a flash file system in memory that
// can lose power at any operation, a made-up Ogg file, a notes repo that behaves like GitHub's
// contents API, and a Gemini that answers what it is told to.
#pragma once

#include "cap_internal.h"
#include "capture.h"
#include "clip.h"
#include "unit.h"

// ---- the file system -----------------------------------------------------------------------------

#define FAKE_FILES 48

typedef struct {
    bool used;
    char name[64];
    char *data;
    size_t len;
} fake_file_t;

typedef struct {
    fake_file_t files[FAKE_FILES];
    int ops;    // writes, renames and removes done
    int cut_at; // power is lost at this one (-1: never): a write leaves half its data, the others do nothing
    bool dead;  // after the cut: everything fails until fake_store_revive()
} fake_store_t;

static inline fake_file_t *fake_find(fake_store_t *fs, const char *name)
{
    for (int i = 0; i < FAKE_FILES; i++) {
        if (fs->files[i].used && strcmp(fs->files[i].name, name) == 0) {
            return &fs->files[i];
        }
    }
    return NULL;
}

// Sets a file directly, as the recorder would write its audio.
static inline void fake_put(fake_store_t *fs, const char *name, const void *data, size_t len)
{
    fake_file_t *file = fake_find(fs, name);
    for (int i = 0; i < FAKE_FILES && !file; i++) {
        if (!fs->files[i].used) {
            file = &fs->files[i];
            *file = (fake_file_t){.used = true};
            snprintf(file->name, sizeof file->name, "%s", name);
        }
    }
    if (!file) {
        fprintf(stderr, "fake store is full\n");
        abort();
    }
    free(file->data);
    file->data = calloc(1, len + 1);
    memcpy(file->data, data, len);
    file->len = len;
}

static inline void fake_delete(fake_store_t *fs, const char *name)
{
    fake_file_t *file = fake_find(fs, name);
    if (file) {
        free(file->data);
        *file = (fake_file_t){0};
    }
}

static inline int fake_count(fake_store_t *fs)
{
    int count = 0;
    for (int i = 0; i < FAKE_FILES; i++) {
        count += fs->files[i].used;
    }
    return count;
}

// True when the operation may happen; at the cut, power is lost instead.
static inline bool fake_op(fake_store_t *fs)
{
    if (fs->dead) {
        return false;
    }
    if (fs->ops == fs->cut_at) {
        fs->dead = true;
        return false;
    }
    fs->ops++;
    return true;
}

static inline bool fake_list(void *ctx, void (*each)(void *arg, const char *name), void *arg)
{
    fake_store_t *fs = ctx;
    if (fs->dead) {
        return false;
    }
    for (int i = 0; i < FAKE_FILES; i++) {
        if (fs->files[i].used) {
            each(arg, fs->files[i].name);
        }
    }
    return true;
}

static inline bool fake_read(void *ctx, const char *name, char **data, size_t *len)
{
    fake_store_t *fs = ctx;
    fake_file_t *file = fs->dead ? NULL : fake_find(fs, name);
    if (!file) {
        return false;
    }
    *data = calloc(1, file->len + 1);
    memcpy(*data, file->data, file->len);
    *len = file->len;
    return true;
}

static inline bool fake_write(void *ctx, const char *name, const char *data, size_t len)
{
    fake_store_t *fs = ctx;
    bool was_dead = fs->dead;
    if (!fake_op(fs)) {
        if (!was_dead) {
            fake_put(fs, name, data, len / 2); // torn
        }
        return false;
    }
    fake_put(fs, name, data, len);
    return true;
}

static inline bool fake_rename(void *ctx, const char *from, const char *to)
{
    fake_store_t *fs = ctx;
    if (!fake_op(fs)) {
        return false;
    }
    fake_file_t *file = fake_find(fs, from);
    if (!file) {
        return false;
    }
    fake_delete(fs, to);
    snprintf(file->name, sizeof file->name, "%s", to);
    return true;
}

static inline bool fake_remove(void *ctx, const char *name)
{
    fake_store_t *fs = ctx;
    if (!fake_op(fs)) {
        return false;
    }
    fake_delete(fs, name);
    return true;
}

static inline int64_t fake_size(void *ctx, const char *name)
{
    fake_store_t *fs = ctx;
    fake_file_t *file = fs->dead ? NULL : fake_find(fs, name);
    return file ? (int64_t)file->len : -1;
}

static inline void *fake_open(void *ctx, const char *name)
{
    fake_store_t *fs = ctx;
    return fs->dead ? NULL : fake_find(fs, name);
}

static inline size_t fake_read_at(void *ctx, void *handle, int64_t offset, uint8_t *buf, size_t cap)
{
    fake_store_t *fs = ctx;
    fake_file_t *file = handle;
    if (fs->dead || offset < 0 || (size_t)offset >= file->len) {
        return 0;
    }
    size_t n = file->len - (size_t)offset < cap ? file->len - (size_t)offset : cap;
    memcpy(buf, file->data + offset, n);
    return n;
}

static inline void fake_close(void *ctx, void *handle)
{
    (void)ctx;
    (void)handle;
}

static inline clip_store_t fake_store(fake_store_t *fs)
{
    fs->cut_at = -1;
    return (clip_store_t){
        .ctx = fs,
        .list = fake_list,
        .read = fake_read,
        .write = fake_write,
        .rename = fake_rename,
        .remove = fake_remove,
        .size = fake_size,
        .open = fake_open,
        .read_at = fake_read_at,
        .close = fake_close,
    };
}

// Power comes back.
static inline void fake_store_revive(fake_store_t *fs)
{
    fs->dead = false;
    fs->cut_at = -1;
}

static inline void fake_store_free(fake_store_t *fs)
{
    for (int i = 0; i < FAKE_FILES; i++) {
        free(fs->files[i].data);
    }
    *fs = (fake_store_t){0};
}

// 32 "random" bytes per seed, so every recording in a test has its own ids.
static inline const uint8_t *fake_random(uint8_t seed)
{
    static uint8_t bytes[32];
    for (int i = 0; i < 32; i++) {
        bytes[i] = (uint8_t)(seed * 31 + i * 7 + 1);
    }
    return bytes;
}

// ---- an Ogg/Opus file ----------------------------------------------------------------------------

#define OGG_PAGE_PACKETS 50 // a second of 20 ms packets per page, as the muxer is set up (Phase D.1)
#define OGG_PACKET 80
#define OGG_AUDIO_PAGE (27 + OGG_PAGE_PACKETS + OGG_PAGE_PACKETS * OGG_PACKET)

static inline size_t ogg_page(uint8_t *out, uint64_t granule, uint32_t sequence, const uint8_t *packet, size_t packet_len,
                       int packets)
{
    memcpy(out, "OggS", 4);
    out[4] = 0;
    out[5] = sequence == 0 ? 2 : 0;
    for (int i = 0; i < 8; i++) {
        out[6 + i] = (uint8_t)(granule >> (8 * i));
    }
    memset(out + 14, 0, 4);
    for (int i = 0; i < 4; i++) {
        out[18 + i] = (uint8_t)(sequence >> (8 * i));
    }
    memset(out + 22, 0, 4); // the page CRC, which nothing here reads
    out[26] = (uint8_t)packets;
    size_t at = 27;
    for (int i = 0; i < packets; i++) {
        out[at++] = (uint8_t)packet_len;
    }
    for (int i = 0; i < packets; i++) {
        memcpy(out + at, packet, packet_len);
        at += packet_len;
    }
    return at;
}

// OpusHead, OpusTags, then `seconds` audio pages of one second each. malloc'd.
static inline uint8_t *fake_ogg(int seconds, int pre_skip, size_t *len)
{
    uint8_t head[19] = "OpusHead\x01\x01", tags[16] = "OpusTags", packet[OGG_PACKET];
    head[10] = (uint8_t)pre_skip;
    head[11] = (uint8_t)(pre_skip >> 8);
    memset(packet, 0x5a, sizeof packet);
    uint8_t *out = malloc(200 + (size_t)seconds * OGG_AUDIO_PAGE);
    size_t at = ogg_page(out, 0, 0, head, sizeof head, 1);
    at += ogg_page(out + at, 0, 1, tags, sizeof tags, 1);
    for (int i = 0; i < seconds; i++) {
        at += ogg_page(out + at, (uint64_t)48000 * (uint64_t)(i + 1), (uint32_t)(2 + i), packet, sizeof packet,
                       OGG_PAGE_PACKETS);
    }
    *len = at;
    return out;
}

#define OGG_HEADERS (27 + 1 + 19 + 27 + 1 + 16) // the two header pages of fake_ogg()

// ---- GitHub --------------------------------------------------------------------------------------

#define GH_FILES 12

typedef struct {
    bool used;
    char path[96];
    char sha[CAP_SHA_LEN];
    char *text;
} gh_file_t;

typedef struct {
    gh_file_t files[GH_FILES];
    int requests, shas;
    char log[4096]; // "GET next-number\nPUT notes/<id>.md\n..."
    // What happens at request number n (-1: never):
    int fail_at;    // the network fails before the request arrives
    int lose_at;    // the request is carried out and its answer is lost
    int status_at;  // it is answered with `status` and not carried out
    int status;
    bool status_always; // every request from status_at on
    int edit_at;    // just before it, another device replaces `edit_path` with `edit_text` (NULL: deletes it)
    const char *edit_path, *edit_text;
} fake_github_t;

static inline void gh_init(fake_github_t *gh)
{
    *gh = (fake_github_t){.fail_at = -1, .lose_at = -1, .status_at = -1, .edit_at = -1};
}

static inline void gh_free(fake_github_t *gh)
{
    for (int i = 0; i < GH_FILES; i++) {
        free(gh->files[i].text);
    }
    gh_init(gh);
}

static inline gh_file_t *gh_find(fake_github_t *gh, const char *path)
{
    for (int i = 0; i < GH_FILES; i++) {
        if (gh->files[i].used && strcmp(gh->files[i].path, path) == 0) {
            return &gh->files[i];
        }
    }
    return NULL;
}

static inline void gh_set(fake_github_t *gh, const char *path, const char *text)
{
    gh_file_t *file = gh_find(gh, path);
    if (!text) {
        if (file) {
            free(file->text);
            *file = (gh_file_t){0};
        }
        return;
    }
    for (int i = 0; i < GH_FILES && !file; i++) {
        if (!gh->files[i].used) {
            file = &gh->files[i];
            *file = (gh_file_t){.used = true};
            snprintf(file->path, sizeof file->path, "%s", path);
        }
    }
    if (!file) {
        fprintf(stderr, "fake GitHub is full\n");
        abort();
    }
    free(file->text);
    file->text = strdup(text);
    snprintf(file->sha, sizeof file->sha, "sha%04d", ++gh->shas);
}

static inline int gh_count(fake_github_t *gh, const char *prefix)
{
    int count = 0;
    for (int i = 0; i < GH_FILES; i++) {
        count += gh->files[i].used && strncmp(gh->files[i].path, prefix, strlen(prefix)) == 0;
    }
    return count;
}

static inline bool gh_send(void *ctx, const char *method, const char *url, const char *body, cap_http_response_t *response)
{
    fake_github_t *gh = ctx;
    int index = gh->requests++;
    const char *at = strstr(url, "/contents/");
    char path[96] = "";
    if (at) {
        snprintf(path, sizeof path, "%.*s", (int)strcspn(at + 10, "?"), at + 10);
    }
    size_t used = strlen(gh->log);
    snprintf(gh->log + used, sizeof gh->log - used, "%s %s\n", method, path);
    if (index == gh->edit_at) {
        gh_set(gh, gh->edit_path, gh->edit_text);
    }
    if (index == gh->fail_at) {
        return false;
    }
    if (index == gh->status_at || (gh->status_always && gh->status_at >= 0 && index > gh->status_at)) {
        response->status = gh->status;
        response->body = strdup("{\"message\":\"Injected\"}");
        return true;
    }
    gh_file_t *file = gh_find(gh, path);
    cap_buf_t out = {0};
    if (strcmp(method, "GET") == 0) {
        CHECK(body == NULL);
        if (!file) {
            response->status = 404;
            cap_buf_str(&out, "{\"message\":\"Not Found\"}");
        } else {
            // as GitHub sends it: the base64 wrapped in lines of 60
            cap_buf_t base64 = {0};
            cap_buf_base64(&base64, (const uint8_t *)file->text, strlen(file->text));
            response->status = 200;
            cap_buf_str(&out, "{\"name\":\"x\",\"sha\":\"");
            cap_buf_str(&out, file->sha);
            cap_buf_str(&out, "\",\"type\":\"file\",\"content\":\"");
            for (size_t i = 0; i < base64.len; i += 60) {
                cap_buf_add(&out, base64.data + i, base64.len - i < 60 ? base64.len - i : 60);
                cap_buf_str(&out, "\\n");
            }
            cap_buf_str(&out, "\",\"encoding\":\"base64\"}");
            free(cap_buf_take(&base64));
        }
    } else {
        cap_json_t root;
        CHECK(body && cap_json_parse(body, &root));
        char *content = cap_json_member_string(root, "content");
        char *sha = cap_json_member_string(root, "sha");
        char *branch = cap_json_member_string(root, "branch");
        char *text = content ? cap_base64_decode(content, NULL) : NULL;
        CHECK(text != NULL);
        CHECK_STR(branch, "main");
        if (file && !sha) {
            response->status = 422; // it exists: a sha is needed
        } else if (file && strcmp(sha, file->sha) != 0) {
            response->status = 409;
        } else if (!file && sha) {
            response->status = 404;
        } else {
            response->status = file ? 200 : 201;
            gh_set(gh, path, text);
        }
        if (response->status < 300) {
            cap_buf_str(&out, "{\"content\":{\"name\":\"x\",\"sha\":\"");
            cap_buf_str(&out, gh_find(gh, path)->sha);
            cap_buf_str(&out, "\"},\"commit\":{\"sha\":\"c\"}}");
        } else {
            cap_buf_str(&out, "{\"message\":\"Refused\"}");
        }
        free(content);
        free(sha);
        free(branch);
        free(text);
    }
    response->body = cap_buf_take(&out);
    if (index == gh->lose_at) {
        free(response->body);
        response->body = NULL;
        return false;
    }
    return true;
}

// ---- Gemini --------------------------------------------------------------------------------------

typedef struct {
    int calls;
    int status;       // what it answers
    char *body;       // malloc'd; kept for every call
    bool offline;     // the network fails
    size_t audio_bytes; // what the last call read
    bool size_matched;  // and that was what it said it would send
    char *prefix;     // the last call's
} fake_gemini_t;

// An interaction answering with this text (GeminiTest.kt's interaction()).
static inline char *fake_interaction(const char *text, const char *status)
{
    cap_buf_t b = {0};
    cap_buf_str(&b, "{\"status\":");
    cap_buf_json(&b, status);
    cap_buf_str(&b, ",\"steps\":[{\"type\":\"user_input\"},{\"type\":\"model_output\",\"content\":[{\"type\":\"text\",\"text\":");
    cap_buf_json(&b, text);
    cap_buf_str(&b, "}]}],\"usage\":{\"total_input_tokens\":900,\"total_output_tokens\":40}}");
    return cap_buf_take(&b);
}

static inline void gemini_answers(fake_gemini_t *gm, int status, char *body)
{
    free(gm->body);
    gm->status = status;
    gm->body = body;
}

static inline void gemini_free(fake_gemini_t *gm)
{
    free(gm->body);
    free(gm->prefix);
    *gm = (fake_gemini_t){0};
}

static inline bool gemini_send(void *ctx, const char *prefix, const clip_audio_t *audio, const char *suffix, int *status,
                        char **body)
{
    fake_gemini_t *gm = ctx;
    gm->calls++;
    free(gm->prefix);
    gm->prefix = strdup(prefix);
    CHECK(strstr(suffix, "\"store\":false}") != NULL);
    // in blocks of a multiple of 3 bytes, as the device base64-encodes them
    uint8_t block[3000];
    size_t n;
    gm->audio_bytes = 0;
    while ((n = audio->read(audio->ctx, block, sizeof block)) > 0) {
        if (gm->audio_bytes == 0) {
            CHECK(n >= 4 && memcmp(block, "OggS", 4) == 0);
        }
        gm->audio_bytes += n;
    }
    gm->size_matched = gm->audio_bytes == audio->size;
    CHECK(gm->size_matched);
    if (gm->offline) {
        return false;
    }
    *status = gm->status;
    *body = strdup(gm->body ? gm->body : "");
    return true;
}
