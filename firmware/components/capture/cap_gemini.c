// The Gemini call (Capture's Gemini.kt): the request body, reading the answer, the rate gate
// (RateGate.kt) and what a failed try does (ProcessWorker.answered, Repository.processingFailed).
#include "cap_internal.h"
#include "capture.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// ---- request -------------------------------------------------------------------------------------

// The value again without whitespace, as kotlinx.serialization prints a parsed element.
static void add_compact(cap_buf_t *out, cap_json_t value)
{
    char type = cap_json_type(value);
    if (type == '"') {
        char *text = cap_json_string(value);
        if (text) {
            cap_buf_json(out, text);
        } else {
            out->failed = true;
        }
        free(text);
    } else if (type == '[') {
        const char *cursor = NULL;
        cap_json_t item;
        cap_buf_str(out, "[");
        for (bool first = true; cap_json_each(value, &cursor, &item); first = false) {
            if (!first) {
                cap_buf_str(out, ",");
            }
            add_compact(out, item);
        }
        cap_buf_str(out, "]");
    } else if (type == '{') {
        const char *cursor = NULL;
        cap_json_t key, member;
        cap_buf_str(out, "{");
        for (bool first = true; cap_json_each_member(value, &cursor, &key, &member); first = false) {
            if (!first) {
                cap_buf_str(out, ",");
            }
            add_compact(out, key);
            cap_buf_str(out, ":");
            add_compact(out, member);
        }
        cap_buf_str(out, "}");
    } else {
        cap_buf_add(out, value.at, value.len);
    }
}

static void add_text_part(cap_buf_t *out, const char *text)
{
    cap_buf_str(out, "{\"type\":\"text\",\"text\":");
    cap_buf_json(out, text);
    cap_buf_str(out, "},");
}

bool cap_gemini_request(const char *prompt, const char *schema, const char *note_text, const char *mime_type,
                        cap_gemini_request_t *out)
{
    *out = (cap_gemini_request_t){0};
    cap_json_t root;
    if (!cap_json_parse(schema, &root)) {
        return false;
    }
    size_t prompt_len = strlen(prompt);
    cap_trim_end(prompt, &prompt_len);
    char *system = cap_strndup(prompt, prompt_len);
    if (!system) {
        return false;
    }
    cap_buf_t prefix = {0}, suffix = {0};
    cap_buf_str(&prefix, "{\"model\":\"" CAP_GEMINI_MODEL "\",\"system_instruction\":");
    cap_buf_json(&prefix, system);
    free(system);
    cap_buf_str(&prefix, ",\"input\":[");
    if (note_text) {
        add_text_part(&prefix, note_text);
        add_text_part(&prefix, CAP_GEMINI_USER_TEXT_APPEND);
    } else {
        add_text_part(&prefix, CAP_GEMINI_USER_TEXT);
    }
    cap_buf_str(&prefix, "{\"type\":\"audio\",\"mime_type\":");
    cap_buf_json(&prefix, mime_type);
    cap_buf_str(&prefix, ",\"data\":\"");

    cap_buf_str(&suffix, "\"}],\"generation_config\":{\"temperature\":" CAP_GEMINI_TEMPERATURE
                         ",\"thinking_level\":\"" CAP_GEMINI_THINKING_LEVEL "\"},"
                         "\"response_format\":{\"type\":\"text\",\"mime_type\":\"application/json\",\"schema\":");
    add_compact(&suffix, root);
    cap_buf_str(&suffix, "},\"store\":false}");
    out->prefix = cap_buf_take(&prefix);
    out->suffix = cap_buf_take(&suffix);
    if (!out->prefix || !out->suffix) {
        cap_gemini_request_free(out);
        return false;
    }
    return true;
}

void cap_gemini_request_free(cap_gemini_request_t *request)
{
    free(request->prefix);
    free(request->suffix);
    *request = (cap_gemini_request_t){0};
}

size_t cap_gemini_content_length(const cap_gemini_request_t *request, size_t audio_bytes)
{
    return strlen(request->prefix) + cap_base64_len(audio_bytes) + strlen(request->suffix);
}

// ---- reading the answer --------------------------------------------------------------------------

static char lower(char c)
{
    return c >= 'A' && c <= 'Z' ? (char)(c - 'A' + 'a') : c;
}

// The first place `word` (lower case) is in text, without regard to case; NULL when it is not.
static const char *find_nocase(const char *text, const char *word)
{
    for (; *text; text++) {
        size_t i = 0;
        while (word[i] && lower(text[i]) == word[i]) {
            i++;
        }
        if (!word[i]) {
            return text;
        }
    }
    return NULL;
}

static bool digit(char c)
{
    return c >= '0' && c <= '9';
}

bool cap_gemini_is_daily_quota(const char *body)
{
    if (find_nocase(body, "per minute")) {
        return false;
    }
    if (strstr(body, "PerDay") || find_nocase(body, "per day")) {
        return true;
    }
    // the older wording named no window: a limit bigger than a minute's is a day's
    for (const char *p = body; (p = strstr(p, "limit: ")) != NULL; p += 7) {
        const char *number = p + 7;
        size_t n = 0;
        while (digit(number[n])) {
            n++;
        }
        long long limit;
        if (n) {
            return cap_parse_long(number, n, &limit) && limit <= 2147483647 && limit > CAP_GEMINI_REQUESTS_PER_MINUTE;
        }
    }
    return false;
}

// (\d+(?:\.\d+)?)s at p, as milliseconds (rounded down); -1 when it is not there.
static int64_t seconds_at(const char *p, bool any_case)
{
    if (!digit(*p)) {
        return -1;
    }
    int64_t ms = 0;
    for (; digit(*p); p++) {
        if (ms > INT64_MAX / 10000) {
            return -1;
        }
        ms = ms * 10 + (*p - '0') * 1000;
    }
    if (*p == '.' && digit(p[1])) {
        int scale = 100;
        for (p++; digit(*p); p++) {
            ms += (*p - '0') * scale;
            scale /= 10;
        }
    }
    return *p == 's' || (any_case && *p == 'S') ? ms : -1;
}

int64_t cap_gemini_retry_delay_ms(const char *body)
{
    // "retryDelay"\s*:\s*"(\d+(?:\.\d+)?)s"
    for (const char *p = body; (p = strstr(p, "\"retryDelay\"")) != NULL;) {
        p += 12;
        const char *q = p;
        while (*q && strchr(" \t\n\v\f\r", *q)) {
            q++;
        }
        if (*q != ':') {
            continue;
        }
        q++;
        while (*q && strchr(" \t\n\v\f\r", *q)) {
            q++;
        }
        if (*q != '"') {
            continue;
        }
        const char *end = q + 1;
        while (digit(*end) || *end == '.') {
            end++;
        }
        int64_t ms = seconds_at(q + 1, false);
        if (ms >= 0 && end[0] == 's' && end[1] == '"') {
            return ms;
        }
    }
    // "Please retry in 14.9s." (the free tier's form)
    for (const char *p = body; (p = find_nocase(p, "retry in ")) != NULL; p += 9) {
        int64_t ms = seconds_at(p + 9, true);
        if (ms >= 0) {
            return ms;
        }
    }
    return -1;
}

// The first line of Google's {"error": {"message": ...}}, or of the body; at most 200 characters.
static void error_message(const char *body, char *out, size_t size)
{
    cap_json_t root, error;
    char *message = NULL;
    if (cap_json_parse(body, &root) && cap_json_member(root, "error", &error)) {
        message = cap_json_member_string(error, "message");
    }
    const char *text = message ? message : body;
    size_t len = strlen(text);
    cap_trim(&text, &len);
    size_t line = 0;
    while (line < len && text[line] != '\n' && text[line] != '\r') {
        line++;
    }
    cap_trim_end(text, &line);
    size_t n = 0;
    for (int chars = 0; n < line && chars < 200; chars++) {
        n++;
        while (n < line && ((unsigned char)text[n] & 0xc0) == 0x80) {
            n++;
        }
    }
    if (!n) {
        text = "no details";
        n = strlen(text);
    }
    snprintf(out, size, "%.*s", (int)n, text);
    cap_utf8_whole(out);
    free(message);
}

static void failed(cap_gemini_result_t *out, const char *message, bool terminal, bool unreadable)
{
    out->kind = CAP_GEMINI_FAILED;
    snprintf(out->message, sizeof out->message, "%s", message);
    out->terminal = terminal;
    out->unreadable = unreadable;
}

static bool string_is(cap_json_t object, const char *key, const char *expected)
{
    char *text = cap_json_member_string(object, key);
    bool same = text && strcmp(text, expected) == 0;
    free(text);
    return same;
}

// The text of every model_output step, joined; NULL when there is none (or no memory).
static char *output_text(cap_json_t root)
{
    cap_json_t steps, step, content, part, text;
    cap_buf_t out = {0};
    bool any = false;
    if (!cap_json_member(root, "steps", &steps)) {
        return NULL;
    }
    for (const char *s = NULL; cap_json_each(steps, &s, &step);) {
        if (!string_is(step, "type", "model_output") || !cap_json_member(step, "content", &content)) {
            continue;
        }
        for (const char *c = NULL; cap_json_each(content, &c, &part);) {
            if (!string_is(part, "type", "text") || !cap_json_member(part, "text", &text)) {
                continue;
            }
            char *piece = cap_json_string(text);
            if (piece) {
                cap_buf_str(&out, piece);
                any = true;
            }
            free(piece);
        }
    }
    char *joined = cap_buf_take(&out);
    if (!any) {
        free(joined);
        return NULL;
    }
    return joined;
}

void cap_gemini_interpret(int code, const char *body, int64_t now_ms, cap_gemini_result_t *out)
{
    *out = (cap_gemini_result_t){.input_tokens = -1, .output_tokens = -1, .retry_after_ms = -1};
    if (!body) {
        body = "";
    }
    if (code == 0) {
        failed(out, "Network error", false, false);
        return;
    }
    if (code < 200 || code > 299) {
        if (code == 429) {
            // The free tier's 429 text is a long paragraph about billing; one line is enough.
            if (cap_gemini_is_daily_quota(body)) {
                failed(out, "HTTP 429: daily free-tier limit reached, will retry after it resets", false, false);
                out->retry_after_ms = cap_until_quota_reset_ms(now_ms);
                return;
            }
            failed(out, "HTTP 429: rate limit reached (free tier), will retry", false, false);
            out->retry_after_ms = cap_gemini_retry_delay_ms(body);
            if (out->retry_after_ms < 0) {
                out->retry_after_ms = 60000; // a 429 that names no wait gets a full window
            }
            return;
        }
        char detail[220];
        error_message(body, detail, sizeof detail);
        out->kind = CAP_GEMINI_FAILED;
        snprintf(out->message, sizeof out->message, "HTTP %d: %s", code, detail);
        out->terminal = code >= 400 && code <= 499 && code != 408;
        return;
    }
    cap_json_t root, value;
    if (!cap_json_parse(body, &root) || cap_json_type(root) != '{') {
        failed(out, "Unreadable response from Gemini", false, true);
        return;
    }
    char *status = cap_json_member_string(root, "status");
    if (!status) {
        failed(out, "Response has no status", false, true);
        return;
    }
    if (strcmp(status, "completed") != 0) {
        out->kind = CAP_GEMINI_NOT_COMPLETED;
        snprintf(out->status, sizeof out->status, "%s", status);
        free(status);
        return;
    }
    free(status);
    char *text = output_text(root);
    if (!text) {
        failed(out, "Gemini returned no text", false, true);
        return;
    }
    // Every field is required: an answer missing one retries, never a note with a field emptied.
    cap_json_t answer;
    if (cap_json_parse(text, &answer)) {
        out->answer.title = cap_json_member_string(answer, "title");
        out->answer.summary = cap_json_member_string(answer, "summary");
        out->answer.transcript = cap_json_member_string(answer, "transcript");
    }
    free(text);
    if (!out->answer.title || !out->answer.summary || !out->answer.transcript) {
        cap_answer_free(&out->answer);
        failed(out, "Gemini's answer did not match the schema", false, true);
        return;
    }
    out->kind = CAP_GEMINI_PARSED;
    cap_json_t usage;
    if (cap_json_member(root, "usage", &usage)) {
        if (!cap_json_member(usage, "total_input_tokens", &value) || !cap_json_integer(value, &out->input_tokens)) {
            out->input_tokens = -1;
        }
        if (!cap_json_member(usage, "total_output_tokens", &value) || !cap_json_integer(value, &out->output_tokens)) {
            out->output_tokens = -1;
        }
    }
}

void cap_gemini_result_free(cap_gemini_result_t *result)
{
    cap_answer_free(&result->answer);
}

// ---- the gate and retries ------------------------------------------------------------------------

int64_t cap_gate_wait_ms(const cap_gate_t *gate, int64_t now_ms)
{
    return gate->next_allowed_ms > now_ms ? gate->next_allowed_ms - now_ms : 0;
}

void cap_gate_finished(cap_gate_t *gate, int64_t now_ms)
{
    cap_gate_hold(gate, now_ms, gate->min_interval_ms);
}

void cap_gate_hold(cap_gate_t *gate, int64_t now_ms, int64_t delay_ms)
{
    if (now_ms + delay_ms > gate->next_allowed_ms) {
        gate->next_allowed_ms = now_ms + delay_ms;
    }
}

cap_retry_t cap_gemini_failed(cap_attempts_t *state, const cap_gemini_result_t *result, cap_gate_t *gate,
                              int64_t now_ms)
{
    char error[sizeof state->last_error];
    bool terminal = false, counts = true;
    if (result->kind == CAP_GEMINI_NOT_COMPLETED) {
        // failed, cancelled, incomplete or budget_exceeded is terminal only when the attempt
        // before gave the same status; other statuses always retry
        static const char *const bad[] = {"failed", "cancelled", "incomplete", "budget_exceeded"};
        snprintf(error, sizeof error, "Gemini returned status %s", result->status);
        for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
            terminal |= strcmp(result->status, bad[i]) == 0 && strcmp(state->last_error, error) == 0;
        }
    } else {
        snprintf(error, sizeof error, "%s", result->message);
        terminal = result->terminal;
        if (result->retry_after_ms >= 0) {
            // a rate limit holds every recording back, and does not use up this one's attempts
            cap_gate_hold(gate, now_ms, result->retry_after_ms);
            counts = false;
        }
    }
    state->attempts += counts;
    memcpy(state->last_error, error, sizeof error);
    return terminal || state->attempts >= CAP_GEMINI_MAX_ATTEMPTS ? CAP_GIVE_UP : CAP_TRY_AGAIN;
}

int64_t cap_backoff_ms(int attempts)
{
    const int64_t max = 5 * 60 * 60 * 1000;
    if (attempts <= 0) {
        return 0;
    }
    return attempts > 11 ? max : (INT64_C(30000) << (attempts - 1) > max ? max : INT64_C(30000) << (attempts - 1));
}
