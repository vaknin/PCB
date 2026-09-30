// The notes repo over GitHub's REST API (Capture's GitHub.kt and GitHubClient.kt, SPEC §9). The
// clip only adds: it takes a number, creates a note, or reads a note and puts it back with an
// addition. Every write is safe to repeat, because an answer can be lost on the way back.
#include "cap_internal.h"
#include "capture.h"

#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

__attribute__((format(printf, 2, 3))) static cap_result_t result(cap_status_t status, const char *format, ...)
{
    cap_result_t out = {.status = status};
    va_list args;
    va_start(args, format);
    vsnprintf(out.message, sizeof out.message, format, args);
    va_end(args);
    cap_utf8_whole(out.message);
    return out;
}

static cap_result_t ok(void)
{
    return (cap_result_t){.status = CAP_OK};
}

static cap_result_t unreadable(void)
{
    return result(CAP_RETRYABLE, "Unreadable answer from GitHub");
}

// URLEncoder.encode: letters, digits and .-*_ stay, a space becomes +, the rest %XX.
static void add_url_encoded(cap_buf_t *out, const char *text)
{
    for (; *text; text++) {
        unsigned char c = (unsigned char)*text;
        if ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || (c && strchr(".-*_", c))) {
            cap_buf_add(out, text, 1);
        } else if (c == ' ') {
            cap_buf_str(out, "+");
        } else {
            char escape[4];
            snprintf(escape, sizeof escape, "%%%02X", c);
            cap_buf_str(out, escape);
        }
    }
}

char *cap_github_contents_url(const char *repo, const char *path, const char *ref_branch)
{
    cap_buf_t out = {0};
    cap_buf_str(&out, "https://api.github.com/repos/");
    cap_buf_str(&out, repo);
    cap_buf_str(&out, "/contents/");
    cap_buf_str(&out, path);
    if (ref_branch) {
        cap_buf_str(&out, "?ref=");
        add_url_encoded(&out, ref_branch);
    }
    return cap_buf_take(&out);
}

char *cap_github_put_body(const char *message, const char *text, const char *branch, const char *sha)
{
    cap_buf_t out = {0};
    cap_buf_str(&out, "{\"message\":");
    cap_buf_json(&out, message);
    cap_buf_str(&out, ",\"content\":\"");
    cap_buf_base64(&out, (const uint8_t *)text, strlen(text));
    cap_buf_str(&out, "\",\"branch\":");
    cap_buf_json(&out, branch);
    if (sha) {
        cap_buf_str(&out, ",\"sha\":");
        cap_buf_json(&out, sha);
    }
    cap_buf_str(&out, "}");
    return cap_buf_take(&out);
}

// Whether `word` (lower case) is in text, without regard to case.
static bool contains_nocase(const char *text, const char *word)
{
    for (; *text; text++) {
        size_t i = 0;
        while (word[i] && (text[i] >= 'A' && text[i] <= 'Z' ? text[i] - 'A' + 'a' : text[i]) == word[i]) {
            i++;
        }
        if (!word[i]) {
            return true;
        }
    }
    return false;
}

cap_result_t cap_github_classify(int code, const char *body, bool rate_limited)
{
    if (code >= 200 && code <= 299) {
        return ok();
    }
    cap_json_t root;
    char *message = cap_json_parse(body, &root) ? cap_json_member_string(root, "message") : NULL;
    cap_result_t out;
    if (code == 401) {
        out = result(CAP_TERMINAL, "GitHub token rejected (expired or revoked?)");
    } else if (code == 403 || code == 429) {
        if (rate_limited || code == 429 || (message && contains_nocase(message, "rate limit"))) {
            out = result(CAP_RETRYABLE, "GitHub rate limit; will retry");
        } else {
            out = result(CAP_TERMINAL, "GitHub token has no access to the notes repo (HTTP 403)");
        }
    } else if (code == 404) {
        out = result(CAP_TERMINAL, "Notes repo or branch not found, or the token cannot see it (HTTP 404)");
    } else if (code == 409 || code == 422) {
        // a write against a tree that moved on: read it again
        out = result(CAP_RETRYABLE, "The notes repo changed during sync; will retry");
    } else if (code == 408 || code >= 500) {
        out = result(CAP_RETRYABLE, "GitHub HTTP %d; will retry", code);
    } else if (message) {
        out = result(CAP_TERMINAL, "GitHub HTTP %d: %s", code, message);
    } else {
        out = result(CAP_TERMINAL, "GitHub HTTP %d", code);
    }
    free(message);
    return out;
}

static bool member_sha(cap_json_t object, char sha[CAP_SHA_LEN])
{
    char *text = cap_json_member_string(object, "sha");
    bool fits = text && *text && strlen(text) < CAP_SHA_LEN;
    if (fits) {
        strcpy(sha, text);
    }
    free(text);
    return fits;
}

bool cap_github_parse_put_sha(const char *body, char sha[CAP_SHA_LEN])
{
    cap_json_t root, content;
    return cap_json_parse(body, &root) && cap_json_member(root, "content", &content) &&
           cap_json_type(content) == '{' && member_sha(content, sha);
}

cap_result_t cap_github_parse_file(const char *body, char **text, char sha[CAP_SHA_LEN])
{
    cap_json_t root;
    *text = NULL;
    if (!cap_json_parse(body, &root) || !member_sha(root, sha)) {
        return unreadable();
    }
    char *content = cap_json_member_string(root, "content");
    if (content) {
        *text = cap_base64_decode(content, NULL);
    }
    free(content);
    return *text ? ok() : unreadable();
}

cap_result_t cap_github_parse_counter(const char *body, int *next, char sha[CAP_SHA_LEN])
{
    char *text;
    cap_result_t read = cap_github_parse_file(body, &text, sha);
    if (read.status != CAP_OK) {
        return read;
    }
    const char *number = text;
    size_t len = strlen(text);
    cap_trim(&number, &len);
    long long value;
    cap_result_t out = ok();
    // Anything but a whole number that can still move on is a hand edit waiting cannot fix.
    if (cap_parse_long(number, len, &value) && value >= 1 && value < 2147483647) {
        *next = (int)value;
    } else {
        size_t shown = 0;
        for (int chars = 0; shown < len && chars < 20; chars++) {
            shown++;
            while (shown < len && ((unsigned char)number[shown] & 0xc0) == 0x80) {
                shown++;
            }
        }
        out = result(CAP_TERMINAL, CAP_COUNTER_PATH " in the notes repo is not a number: \"%.*s\"", (int)shown, number);
    }
    free(text);
    return out;
}

// ---- the calls -----------------------------------------------------------------------------------

// One request. CAP_OK means an answer came, whatever its status; response->body is then the
// caller's to free. A rate limit (403 or 429 with the headers that say so) is reported here.
static cap_result_t call(const cap_github_t *github, const char *method, const char *url, const char *body,
                         cap_http_response_t *response)
{
    *response = (cap_http_response_t){0};
    if (!url || (!body && strcmp(method, "GET") != 0)) {
        return result(CAP_RETRYABLE, "Out of memory");
    }
    if (!github->send(github->ctx, method, url, body, response)) {
        free(response->body);
        response->body = NULL;
        return result(CAP_RETRYABLE, "Network error");
    }
    if (!response->body) {
        response->body = cap_strndup("", 0);
        if (!response->body) {
            return result(CAP_RETRYABLE, "Out of memory");
        }
    }
    if ((response->status == 403 || response->status == 429) && response->rate_limited) {
        cap_result_t limited = cap_github_classify(response->status, response->body, true);
        free(response->body);
        response->body = NULL;
        return limited;
    }
    return ok();
}

// A status that should have been a success and was not.
static cap_result_t failure(const cap_http_response_t *response)
{
    cap_result_t out = cap_github_classify(response->status, response->body, false);
    return out.status == CAP_OK ? result(CAP_RETRYABLE, "Unexpected answer from GitHub") : out;
}

static cap_result_t put_file(const cap_github_t *github, const char *path, const char *message, const char *text,
                             const char *sha, cap_http_response_t *response)
{
    char *url = cap_github_contents_url(github->repo, path, NULL);
    char *body = text ? cap_github_put_body(message, text, github->branch, sha) : NULL;
    cap_result_t out = call(github, "PUT", url, body, response);
    free(url);
    free(body);
    return out;
}

cap_result_t cap_github_reserve_number(const cap_github_t *github, int *number)
{
    for (int round = 0; round < CAP_RESERVE_TRIES; round++) {
        cap_http_response_t response;
        char *url = cap_github_contents_url(github->repo, CAP_COUNTER_PATH, github->branch);
        cap_result_t out = call(github, "GET", url, NULL, &response);
        free(url);
        if (out.status != CAP_OK) {
            return out;
        }
        int next = 1;
        char sha[CAP_SHA_LEN] = "";
        // No file yet is a fresh counter. (A missing repo also answers 404; the PUT then says so.)
        if (response.status == 200) {
            out = cap_github_parse_counter(response.body, &next, sha);
        } else if (response.status != 404) {
            out = failure(&response);
        }
        free(response.body);
        if (out.status != CAP_OK) {
            return out;
        }
        char message[48], text[16];
        snprintf(message, sizeof message, CAP_SOURCE ": next number %d", next + 1);
        snprintf(text, sizeof text, "%d\n", next + 1);
        out = put_file(github, CAP_COUNTER_PATH, message, text, *sha ? sha : NULL, &response);
        if (out.status != CAP_OK) {
            return out;
        }
        int status = response.status;
        if (status != 200 && status != 201 && status != 409 && status != 422) {
            out = failure(&response);
        }
        free(response.body);
        if (out.status != CAP_OK) {
            return out;
        }
        if (status == 200 || status == 201) {
            *number = next;
            return ok();
        }
        // another device moved or created it first: start over
    }
    return result(CAP_RETRYABLE, "The note counter kept changing; will retry");
}

cap_result_t cap_github_get_note(const cap_github_t *github, const char *id, cap_note_t *note, char sha[CAP_SHA_LEN])
{
    *note = (cap_note_t){.duration_ms = CAP_NO_DURATION};
    cap_http_response_t response;
    char *path = cap_note_path(id);
    char *url = path ? cap_github_contents_url(github->repo, path, github->branch) : NULL;
    cap_result_t out = call(github, "GET", url, NULL, &response);
    free(path);
    free(url);
    if (out.status != CAP_OK) {
        return out;
    }
    char *text = NULL;
    if (response.status == 404) {
        out = result(CAP_GONE, "The note is not on GitHub");
    } else if (response.status != 200) {
        out = failure(&response);
    } else {
        out = cap_github_parse_file(response.body, &text, sha);
    }
    free(response.body);
    if (out.status == CAP_OK && !cap_note_parse(text, note)) {
        out = result(CAP_RETRYABLE, "Out of memory");
    }
    free(text);
    return out;
}

// PUTs the note. *status is the HTTP status when CAP_OK; on 200/201 sha is the new one.
static cap_result_t put_note(const cap_github_t *github, const char *id, const cap_note_t *note, const char *verb,
                             const char *old_sha, char sha[CAP_SHA_LEN], int *status)
{
    if (!note->has_created) {
        return result(CAP_TERMINAL, "The note has no time");
    }
    cap_http_response_t response;
    char *path = cap_note_path(id);
    char *text = cap_note_render(id, note);
    cap_buf_t message = {0};
    cap_buf_str(&message, verb);
    // the title as the file has it: one line
    const char *title = text ? strstr(text, "\n# ") : NULL;
    if (title) {
        cap_buf_add(&message, title + 3, strcspn(title + 3, "\n"));
    }
    char *subject = cap_buf_take(&message);
    cap_result_t out = path && subject ? put_file(github, path, subject, text, old_sha, &response)
                                       : result(CAP_RETRYABLE, "Out of memory");
    free(path);
    free(text);
    free(subject);
    if (out.status != CAP_OK) {
        return out;
    }
    *status = response.status;
    if ((response.status == 200 || response.status == 201) && !cap_github_parse_put_sha(response.body, sha)) {
        out = unreadable();
    } else if (response.status != 200 && response.status != 201 && response.status != 409 && response.status != 422 &&
               response.status != 404) {
        out = failure(&response);
    }
    free(response.body);
    return out;
}

cap_result_t cap_github_create_note(const cap_github_t *github, const char *id, const cap_note_t *note,
                                    char sha[CAP_SHA_LEN])
{
    int status = 0;
    cap_result_t out = put_note(github, id, note, CAP_SOURCE ": add ", NULL, sha, &status);
    if (out.status != CAP_OK || status == 200 || status == 201) {
        return out;
    }
    if (status == 404) {
        return cap_github_classify(404, "", false);
    }
    // 409 or 422: the file may be there already, from an earlier try whose answer was lost
    cap_note_t there;
    out = cap_github_get_note(github, id, &there, sha);
    cap_note_free(&there);
    if (out.status == CAP_GONE) {
        return cap_github_classify(status, "", false);
    }
    return out;
}

cap_result_t cap_github_update_note(const cap_github_t *github, const char *id, cap_note_t *note,
                                    char sha[CAP_SHA_LEN])
{
    for (int round = 0; round < CAP_UPDATE_TRIES; round++) {
        int status = 0;
        char new_sha[CAP_SHA_LEN];
        cap_result_t out = put_note(github, id, note, CAP_SOURCE ": add to ", sha, new_sha, &status);
        if (out.status != CAP_OK) {
            return out;
        }
        if (status == 200 || status == 201) {
            strcpy(sha, new_sha);
            return ok();
        }
        if (status == 404) {
            return result(CAP_GONE, "The note is not on GitHub");
        }
        // 409 or 422: the file changed meanwhile. Read it and merge.
        cap_note_t remote;
        out = cap_github_get_note(github, id, &remote, new_sha);
        if (out.status != CAP_OK) {
            return out;
        }
        cap_merge_t merged = cap_note_merge(&remote, note);
        if (merged == CAP_MERGE_NO_MEMORY) {
            cap_note_free(&remote);
            return result(CAP_RETRYABLE, "Out of memory");
        }
        cap_note_free(note);
        *note = remote;
        strcpy(sha, new_sha);
        if (merged == CAP_REMOTE_HAS_ALL) {
            return ok(); // an earlier try got through, or another device merged ours
        }
    }
    return result(CAP_RETRYABLE, "The notes repo kept changing during sync; will retry");
}
