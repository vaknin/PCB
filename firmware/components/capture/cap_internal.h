// Shared by the capture component's files and its tests: a growing string, JSON and base64.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// A growing, always NUL-terminated string. After a failed allocation every call is a no-op and
// cap_buf_take() returns NULL.
typedef struct {
    char *data;
    size_t len, cap;
    bool failed;
} cap_buf_t;

void cap_buf_add(cap_buf_t *buf, const char *text, size_t len);
void cap_buf_str(cap_buf_t *buf, const char *text);
void cap_buf_int(cap_buf_t *buf, long long value);
void cap_buf_json(cap_buf_t *buf, const char *text); // a JSON string, with its quotes
void cap_buf_base64(cap_buf_t *buf, const uint8_t *data, size_t len);
char *cap_buf_take(cap_buf_t *buf);

char *cap_strndup(const char *text, size_t len);

// Kotlin's trim(): Unicode whitespace, not only ASCII.
void cap_trim(const char **text, size_t *len);
void cap_trim_end(const char *text, size_t *len);
char *cap_trimmed(const char *text); // malloc'd; NULL reads as ""
bool cap_is_blank(const char *text);

// Base64 as GitHub sends it, wrapped in lines: anything outside the alphabet is skipped
// (Base64.getMimeDecoder). malloc'd, NUL-terminated.
char *cap_base64_decode(const char *text, size_t *len);

// A JSON value inside a checked document.
typedef struct {
    const char *at;
    size_t len;
} cap_json_t;

// Checks the whole text is one JSON value. Everything below works on the spans it hands out.
bool cap_json_parse(const char *text, cap_json_t *root);
// '{', '[', '"', 't', 'f', 'n', or '0' for a number.
char cap_json_type(cap_json_t value);
bool cap_json_member(cap_json_t object, const char *key, cap_json_t *value);
// Walks an object's members in the file's order: *cursor starts as NULL. `key` is a JSON string.
bool cap_json_each_member(cap_json_t object, const char **cursor, cap_json_t *key, cap_json_t *value);
// Walks an array: *cursor starts as NULL.
bool cap_json_each(cap_json_t array, const char **cursor, cap_json_t *item);
// The text of a JSON string (malloc'd); NULL when the value is not a string.
char *cap_json_string(cap_json_t value);
char *cap_json_member_string(cap_json_t object, const char *key);
bool cap_json_integer(cap_json_t value, long long *out);

// Kotlin's String.toLongOrNull: an optional sign and decimal digits, within a long.
bool cap_parse_long(const char *text, size_t len, long long *out);
// Cuts a string that snprintf truncated inside a UTF-8 character back to a whole one.
void cap_utf8_whole(char *text);
