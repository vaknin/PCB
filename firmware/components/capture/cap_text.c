// Text helpers for the Capture client: a growing string, Kotlin's trim, base64 and a small strict
// JSON reader. ESP-IDF v6.1 ships no JSON component (cJSON is a managed one) and the laptop tests
// build without IDF, so the reader is our own.
#include "cap_internal.h"
#include "capture.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// ---- growing string ------------------------------------------------------------------------------

static bool buf_room(cap_buf_t *buf, size_t extra)
{
    if (buf->failed) {
        return false;
    }
    size_t need = buf->len + extra + 1;
    if (need <= buf->cap) {
        return true;
    }
    size_t cap = buf->cap ? buf->cap : 64;
    while (cap < need) {
        cap *= 2;
    }
    char *data = realloc(buf->data, cap);
    if (!data) {
        free(buf->data);
        *buf = (cap_buf_t){.failed = true};
        return false;
    }
    buf->data = data;
    buf->cap = cap;
    return true;
}

void cap_buf_add(cap_buf_t *buf, const char *text, size_t len)
{
    if (!buf_room(buf, len)) {
        return;
    }
    if (len) {
        memcpy(buf->data + buf->len, text, len);
    }
    buf->len += len;
    buf->data[buf->len] = 0;
}

void cap_buf_str(cap_buf_t *buf, const char *text)
{
    cap_buf_add(buf, text, strlen(text));
}

void cap_buf_int(cap_buf_t *buf, long long value)
{
    char digits[24];
    int n = snprintf(digits, sizeof digits, "%lld", value);
    cap_buf_add(buf, digits, (size_t)n);
}

// As kotlinx.serialization writes a string: the short escapes, \u00xx for other control
// characters, everything else as it is.
void cap_buf_json(cap_buf_t *buf, const char *text)
{
    cap_buf_add(buf, "\"", 1);
    const char *run = text;
    for (const char *p = text;; p++) {
        unsigned char c = (unsigned char)*p;
        if (c >= 0x20 && c != '"' && c != '\\') {
            continue;
        }
        cap_buf_add(buf, run, (size_t)(p - run));
        if (!c) {
            break;
        }
        run = p + 1;
        char escape[8];
        switch (c) {
        case '"': cap_buf_str(buf, "\\\""); break;
        case '\\': cap_buf_str(buf, "\\\\"); break;
        case '\n': cap_buf_str(buf, "\\n"); break;
        case '\r': cap_buf_str(buf, "\\r"); break;
        case '\t': cap_buf_str(buf, "\\t"); break;
        case '\b': cap_buf_str(buf, "\\b"); break;
        case '\f': cap_buf_str(buf, "\\f"); break;
        default:
            snprintf(escape, sizeof escape, "\\u%04x", c);
            cap_buf_str(buf, escape);
        }
    }
    cap_buf_add(buf, "\"", 1);
}

void cap_buf_base64(cap_buf_t *buf, const uint8_t *data, size_t len)
{
    size_t n = cap_base64_len(len);
    if (!buf_room(buf, n)) {
        return;
    }
    cap_base64_encode(data, len, buf->data + buf->len);
    buf->len += n;
    buf->data[buf->len] = 0;
}

char *cap_buf_take(cap_buf_t *buf)
{
    if (!buf->failed && !buf->data) {
        buf_room(buf, 0); // an empty string, not NULL
        if (buf->data) {
            buf->data[0] = 0;
        }
    }
    char *data = buf->failed ? NULL : buf->data;
    *buf = (cap_buf_t){0};
    return data;
}

char *cap_strndup(const char *text, size_t len)
{
    char *copy = malloc(len + 1);
    if (copy) {
        if (len) {
            memcpy(copy, text, len);
        }
        copy[len] = 0;
    }
    return copy;
}

// ---- Kotlin's trim -------------------------------------------------------------------------------

// The bytes of the whitespace character at the start of text, or 0. Kotlin's Char.isWhitespace:
// U+0009-000D, U+001C-0020, U+00A0, U+1680, U+2000-200A, U+2028, U+2029, U+202F, U+205F, U+3000.
static size_t space_at(const char *text, size_t len)
{
    const unsigned char *p = (const unsigned char *)text;
    if (!len) {
        return 0;
    }
    if ((p[0] >= 0x09 && p[0] <= 0x0d) || (p[0] >= 0x1c && p[0] <= 0x20)) {
        return 1;
    }
    if (len >= 2 && p[0] == 0xc2 && p[1] == 0xa0) {
        return 2;
    }
    if (len < 3) {
        return 0;
    }
    if (p[0] == 0xe1 && p[1] == 0x9a && p[2] == 0x80) {
        return 3;
    }
    if (p[0] == 0xe2 && p[1] == 0x80 && ((p[2] >= 0x80 && p[2] <= 0x8a) || p[2] == 0xa8 || p[2] == 0xa9 || p[2] == 0xaf)) {
        return 3;
    }
    if ((p[0] == 0xe2 && p[1] == 0x81 && p[2] == 0x9f) || (p[0] == 0xe3 && p[1] == 0x80 && p[2] == 0x80)) {
        return 3;
    }
    return 0;
}

void cap_trim_end(const char *text, size_t *len)
{
    for (;;) {
        size_t n = 0;
        for (size_t width = 1; width <= 3 && width <= *len; width++) {
            if (space_at(text + *len - width, width) == width) {
                n = width;
                break;
            }
        }
        if (!n) {
            return;
        }
        *len -= n;
    }
}

void cap_trim(const char **text, size_t *len)
{
    size_t n;
    while ((n = space_at(*text, *len)) != 0) {
        *text += n;
        *len -= n;
    }
    cap_trim_end(*text, len);
}

char *cap_trimmed(const char *text)
{
    if (!text) {
        text = "";
    }
    size_t len = strlen(text);
    cap_trim(&text, &len);
    return cap_strndup(text, len);
}

bool cap_is_blank(const char *text)
{
    if (!text) {
        return true;
    }
    size_t len = strlen(text);
    cap_trim(&text, &len);
    return len == 0;
}

// ---- base64 --------------------------------------------------------------------------------------

static const char BASE64[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

size_t cap_base64_len(size_t n)
{
    return (n + 2) / 3 * 4;
}

size_t cap_base64_encode(const uint8_t *in, size_t n, char *out)
{
    size_t o = 0;
    for (size_t i = 0; i < n; i += 3) {
        uint32_t v = (uint32_t)in[i] << 16;
        if (i + 1 < n) {
            v |= (uint32_t)in[i + 1] << 8;
        }
        if (i + 2 < n) {
            v |= in[i + 2];
        }
        out[o++] = BASE64[v >> 18];
        out[o++] = BASE64[(v >> 12) & 63];
        out[o++] = i + 1 < n ? BASE64[(v >> 6) & 63] : '=';
        out[o++] = i + 2 < n ? BASE64[v & 63] : '=';
    }
    return o;
}

char *cap_base64_decode(const char *text, size_t *len)
{
    char *out = malloc(strlen(text) / 4 * 3 + 4);
    if (!out) {
        return NULL;
    }
    size_t n = 0;
    uint32_t bits = 0;
    int count = 0;
    for (const char *p = text; *p && *p != '='; p++) {
        const char *at = strchr(BASE64, *p);
        if (!at) {
            continue;
        }
        bits = bits << 6 | (uint32_t)(at - BASE64);
        if (++count == 4) {
            out[n++] = (char)(bits >> 16);
            out[n++] = (char)(bits >> 8);
            out[n++] = (char)bits;
            bits = 0;
            count = 0;
        }
    }
    if (count == 1) { // six bits are not a byte
        free(out);
        return NULL;
    }
    if (count == 3) {
        out[n++] = (char)(bits >> 10);
        out[n++] = (char)(bits >> 2);
    } else if (count == 2) {
        out[n++] = (char)(bits >> 4);
    }
    out[n] = 0;
    if (len) {
        *len = n;
    }
    return out;
}

// ---- JSON ----------------------------------------------------------------------------------------

#define JSON_MAX_DEPTH 32

static const char *json_space(const char *p, const char *end)
{
    while (p < end && (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r')) {
        p++;
    }
    return p;
}

static bool is_digit(char c)
{
    return c >= '0' && c <= '9';
}

static int hex_value(char c)
{
    if (is_digit(c)) {
        return c - '0';
    }
    if (c >= 'a' && c <= 'f') {
        return c - 'a' + 10;
    }
    if (c >= 'A' && c <= 'F') {
        return c - 'A' + 10;
    }
    return -1;
}

// p is at the opening quote; returns just past the closing one, or NULL.
static const char *json_skip_string(const char *p, const char *end)
{
    for (p++; p < end; p++) {
        unsigned char c = (unsigned char)*p;
        if (c == '"') {
            return p + 1;
        }
        if (c < 0x20) {
            return NULL;
        }
        if (c != '\\') {
            continue;
        }
        if (++p >= end) {
            return NULL;
        }
        if (*p == 'u') {
            if (end - p < 5 || hex_value(p[1]) < 0 || hex_value(p[2]) < 0 || hex_value(p[3]) < 0 || hex_value(p[4]) < 0) {
                return NULL;
            }
            p += 4;
        } else if (!strchr("\"\\/bfnrt", *p)) {
            return NULL;
        }
    }
    return NULL;
}

static const char *json_skip_number(const char *p, const char *end)
{
    if (p < end && *p == '-') {
        p++;
    }
    if (p >= end || !is_digit(*p)) {
        return NULL;
    }
    if (*p == '0') {
        p++;
    } else {
        while (p < end && is_digit(*p)) {
            p++;
        }
    }
    if (p < end && *p == '.') {
        p++;
        if (p >= end || !is_digit(*p)) {
            return NULL;
        }
        while (p < end && is_digit(*p)) {
            p++;
        }
    }
    if (p < end && (*p == 'e' || *p == 'E')) {
        p++;
        if (p < end && (*p == '+' || *p == '-')) {
            p++;
        }
        if (p >= end || !is_digit(*p)) {
            return NULL;
        }
        while (p < end && is_digit(*p)) {
            p++;
        }
    }
    return p;
}

// p is at a value's first character; returns just past the value, or NULL when it is not JSON.
static const char *json_skip_value(const char *p, const char *end, int depth)
{
    if (p >= end || depth > JSON_MAX_DEPTH) {
        return NULL;
    }
    if (*p == '"') {
        return json_skip_string(p, end);
    }
    if (*p == '{' || *p == '[') {
        bool object = *p == '{';
        char close = object ? '}' : ']';
        p = json_space(p + 1, end);
        if (p < end && *p == close) {
            return p + 1;
        }
        for (;;) {
            if (object) {
                if (p >= end || *p != '"' || !(p = json_skip_string(p, end))) {
                    return NULL;
                }
                p = json_space(p, end);
                if (p >= end || *p != ':') {
                    return NULL;
                }
                p = json_space(p + 1, end);
            }
            if (!(p = json_skip_value(p, end, depth + 1))) {
                return NULL;
            }
            p = json_space(p, end);
            if (p >= end) {
                return NULL;
            }
            if (*p == close) {
                return p + 1;
            }
            if (*p != ',') {
                return NULL;
            }
            p = json_space(p + 1, end);
        }
    }
    static const char *const words[] = {"true", "false", "null"};
    for (size_t i = 0; i < 3; i++) {
        size_t n = strlen(words[i]);
        if ((size_t)(end - p) >= n && memcmp(p, words[i], n) == 0) {
            return p + n;
        }
    }
    return json_skip_number(p, end);
}

bool cap_json_parse(const char *text, cap_json_t *root)
{
    if (!text) {
        return false;
    }
    const char *end = text + strlen(text);
    const char *start = json_space(text, end);
    const char *after = json_skip_value(start, end, 1);
    if (!after || json_space(after, end) != end) {
        return false;
    }
    *root = (cap_json_t){start, (size_t)(after - start)};
    return true;
}

char cap_json_type(cap_json_t value)
{
    if (!value.len) {
        return 0;
    }
    return strchr("{[\"tfn", *value.at) ? *value.at : '0';
}

static void utf8_add(cap_buf_t *out, uint32_t c)
{
    char b[4];
    if (c < 0x80) {
        b[0] = (char)c;
        cap_buf_add(out, b, 1);
    } else if (c < 0x800) {
        b[0] = (char)(0xc0 | c >> 6);
        b[1] = (char)(0x80 | (c & 0x3f));
        cap_buf_add(out, b, 2);
    } else if (c < 0x10000) {
        b[0] = (char)(0xe0 | c >> 12);
        b[1] = (char)(0x80 | (c >> 6 & 0x3f));
        b[2] = (char)(0x80 | (c & 0x3f));
        cap_buf_add(out, b, 3);
    } else {
        b[0] = (char)(0xf0 | c >> 18);
        b[1] = (char)(0x80 | (c >> 12 & 0x3f));
        b[2] = (char)(0x80 | (c >> 6 & 0x3f));
        b[3] = (char)(0x80 | (c & 0x3f));
        cap_buf_add(out, b, 4);
    }
}

static uint32_t hex4(const char *p)
{
    return (uint32_t)(hex_value(p[0]) << 12 | hex_value(p[1]) << 8 | hex_value(p[2]) << 4 | hex_value(p[3]));
}

char *cap_json_string(cap_json_t value)
{
    if (cap_json_type(value) != '"') {
        return NULL;
    }
    cap_buf_t out = {0};
    const char *end = value.at + value.len - 1;
    for (const char *p = value.at + 1; p < end; p++) {
        if (*p != '\\') {
            cap_buf_add(&out, p, 1);
            continue;
        }
        p++;
        switch (*p) {
        case 'b': cap_buf_add(&out, "\b", 1); break;
        case 'f': cap_buf_add(&out, "\f", 1); break;
        case 'n': cap_buf_add(&out, "\n", 1); break;
        case 'r': cap_buf_add(&out, "\r", 1); break;
        case 't': cap_buf_add(&out, "\t", 1); break;
        case 'u': {
            uint32_t c = hex4(p + 1);
            p += 4;
            if (c >= 0xd800 && c <= 0xdbff && end - p > 6 && p[1] == '\\' && p[2] == 'u') {
                uint32_t low = hex4(p + 3);
                if (low >= 0xdc00 && low <= 0xdfff) {
                    c = 0x10000 + ((c - 0xd800) << 10) + (low - 0xdc00);
                    p += 6;
                }
            }
            // A C string cannot hold U+0000, and UTF-8 cannot hold half a pair.
            if (c == 0 || (c >= 0xd800 && c <= 0xdfff)) {
                c = 0xfffd;
            }
            utf8_add(&out, c);
            break;
        }
        default: cap_buf_add(&out, p, 1); // " \ /
        }
    }
    return cap_buf_take(&out);
}

static bool key_is(cap_json_t key, const char *name)
{
    if (!memchr(key.at, '\\', key.len)) {
        return key.len == strlen(name) + 2 && memcmp(key.at + 1, name, key.len - 2) == 0;
    }
    char *text = cap_json_string(key);
    bool same = text && strcmp(text, name) == 0;
    free(text);
    return same;
}

// Of two members with one name the last counts, as in kotlinx.serialization's JsonObject.
bool cap_json_member(cap_json_t object, const char *key, cap_json_t *value)
{
    if (cap_json_type(object) != '{') {
        return false;
    }
    const char *cursor = NULL;
    cap_json_t name, member;
    bool found = false;
    while (cap_json_each_member(object, &cursor, &name, &member)) {
        if (key_is(name, key)) {
            *value = member;
            found = true;
        }
    }
    return found;
}

bool cap_json_each_member(cap_json_t object, const char **cursor, cap_json_t *key, cap_json_t *value)
{
    if (cap_json_type(object) != '{') {
        return false;
    }
    const char *end = object.at + object.len;
    const char *p = json_space(*cursor ? *cursor : object.at + 1, end);
    if (*p == ',') {
        p = json_space(p + 1, end);
    }
    if (*p != '"') {
        return false;
    }
    const char *key_end = json_skip_string(p, end);
    *key = (cap_json_t){p, (size_t)(key_end - p)};
    p = json_space(json_space(key_end, end) + 1, end);
    *cursor = json_skip_value(p, end, 1);
    *value = (cap_json_t){p, (size_t)(*cursor - p)};
    return true;
}

bool cap_json_each(cap_json_t array, const char **cursor, cap_json_t *item)
{
    if (cap_json_type(array) != '[') {
        return false;
    }
    const char *end = array.at + array.len;
    const char *p = json_space(*cursor ? *cursor : array.at + 1, end);
    if (*p == ',') {
        p = json_space(p + 1, end);
    }
    if (*p == ']') {
        return false;
    }
    const char *value_end = json_skip_value(p, end, 1);
    *item = (cap_json_t){p, (size_t)(value_end - p)};
    *cursor = value_end;
    return true;
}

char *cap_json_member_string(cap_json_t object, const char *key)
{
    cap_json_t value;
    return cap_json_member(object, key, &value) ? cap_json_string(value) : NULL;
}

bool cap_json_integer(cap_json_t value, long long *out)
{
    if (cap_json_type(value) != '0') {
        return false;
    }
    return cap_parse_long(value.at, value.len, out);
}

// String.toLongOrNull: an optional sign and decimal digits, within a long.
bool cap_parse_long(const char *text, size_t len, long long *out)
{
    size_t i = 0;
    bool negative = len && text[0] == '-';
    if (len && (text[0] == '-' || text[0] == '+')) {
        i = 1;
    }
    if (i == len) {
        return false;
    }
    unsigned long long limit = negative ? 9223372036854775808ULL : 9223372036854775807ULL;
    unsigned long long v = 0;
    for (; i < len; i++) {
        if (!is_digit(text[i])) {
            return false;
        }
        unsigned d = (unsigned)(text[i] - '0');
        if (v > (limit - d) / 10) {
            return false;
        }
        v = v * 10 + d;
    }
    *out = negative ? (long long)(0 - v) : (long long)v;
    return true;
}

// Cuts a string that snprintf truncated in the middle of a UTF-8 character back to a whole one.
void cap_utf8_whole(char *text)
{
    size_t len = strlen(text);
    size_t start = len;
    while (start > 0 && ((unsigned char)text[start - 1] & 0xc0) == 0x80) {
        start--;
    }
    if (start == 0) {
        return;
    }
    unsigned char lead = (unsigned char)text[start - 1];
    size_t want = lead >= 0xf0 ? 4 : lead >= 0xe0 ? 3 : lead >= 0xc0 ? 2 : 1;
    if (want > 1 && len - (start - 1) < want) {
        text[start - 1] = 0;
    }
}
