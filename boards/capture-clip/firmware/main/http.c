// HTTPS for the two services (research/2026-09-29-esp32-firmware.md §4): GitHub's contents API
// through one kept-open connection, and Gemini with the recording streamed from flash as base64
// under a fixed Content-Length, so nothing larger than one block is in memory.
//
// In the QEMU build only, the two hosts are swapped for the local mock server named by the
// provisioned `sim_base`, over plain HTTP: no real request ever leaves a simulation, and a real
// build has no such switch.
#include "app.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include "board.h"
#include "esp_app_desc.h"
#include "esp_crt_bundle.h"
#include "esp_http_client.h"
#include "provision.h"

#define GITHUB_HOST "https://api.github.com"
#define GEMINI_HOST "https://generativelanguage.googleapis.com"
#define GITHUB_TIMEOUT_MS 20000
// thinking level "high" can take tens of seconds before the first byte of the answer
#define GEMINI_TIMEOUT_MS 180000
#define ANSWER_MAX (1024 * 1024)
#define BLOCK 3072 // audio bytes per block: a multiple of 3, so base64 blocks join up

static struct {
    bool loaded;
    char repo[96], branch[48], gemini_key[96], auth[160], agent[48], sim_base[64];
    esp_http_client_handle_t github; // kept open between requests of one wake
} keys;

typedef struct {
    char *data;
    size_t len, cap, max;
    bool failed, rate_limited;
    bool (*block)(void *ctx, const uint8_t *data, size_t len);
    void *block_ctx;
} answer_t;

static bool get_key(const char *name, char *out, size_t len)
{
    return provision_get(name, out, len) == ESP_OK && out[0];
}

bool http_load_keys(char *why, size_t len)
{
    if (keys.loaded) {
        return true;
    }
    char token[128];
    const char *missing = !get_key("notes_repo", keys.repo, sizeof keys.repo)             ? "notes_repo"
                          : !get_key("gemini_api_key", keys.gemini_key, sizeof keys.gemini_key) ? "gemini_api_key"
                          : !get_key("github_token", token, sizeof token)                       ? "github_token"
                                                                                                : NULL;
    if (missing) {
        snprintf(why, len, "%s is not provisioned", missing);
        http_forget();
        return false;
    }
    snprintf(keys.auth, sizeof keys.auth, "Bearer %s", token);
    memset(token, 0, sizeof token);
    if (!get_key("notes_branch", keys.branch, sizeof keys.branch)) {
        snprintf(keys.branch, sizeof keys.branch, "main");
    }
    snprintf(keys.agent, sizeof keys.agent, "capture-clip/%.30s", esp_app_get_description()->version);
#if BOARD_IS_QEMU
    if (!get_key("sim_base", keys.sim_base, sizeof keys.sim_base)) {
        snprintf(why, len, "sim_base is not provisioned (QEMU talks only to the mock server)");
        http_forget();
        return false;
    }
#endif
    keys.loaded = true;
    return true;
}

void http_close(void)
{
    if (keys.github) {
        esp_http_client_cleanup(keys.github);
        keys.github = NULL;
    }
}

void http_forget(void)
{
    http_close();
    memset(&keys, 0, sizeof keys);
}

const char *http_repo(void)
{
    return keys.repo;
}

const char *http_branch(void)
{
    return keys.branch;
}

const char *http_sim_base(void)
{
#if BOARD_IS_QEMU
    return keys.sim_base[0] ? keys.sim_base : NULL;
#else
    return NULL;
#endif
}

// The URL to really ask (malloc'd): itself, or in QEMU the mock server's stand-in for the host.
static char *real_url(const char *url)
{
#if BOARD_IS_QEMU
    static const struct {
        const char *host, *path;
    } swap[] = {{GITHUB_HOST, "/github"}, {GEMINI_HOST, "/gemini"}};
    for (size_t i = 0; i < 2; i++) {
        size_t n = strlen(swap[i].host);
        if (strncmp(url, swap[i].host, n) == 0) {
            size_t size = strlen(keys.sim_base) + strlen(swap[i].path) + strlen(url + n) + 1;
            char *out = malloc(size);
            if (out) {
                snprintf(out, size, "%s%s%s", keys.sim_base, swap[i].path, url + n);
            }
            return out;
        }
    }
    // anything else in a simulation must already point at the mock server
    if (strncmp(url, keys.sim_base, strlen(keys.sim_base)) != 0 || !keys.sim_base[0]) {
        return NULL;
    }
#endif
    return strdup(url);
}

static bool add(answer_t *answer, const char *data, size_t len)
{
    if (answer->block) {
        if (!answer->block(answer->block_ctx, (const uint8_t *)data, len)) {
            answer->failed = true;
        }
        return !answer->failed;
    }
    if (answer->failed || answer->len + len > answer->max) {
        answer->failed = true;
        return false;
    }
    if (answer->len + len + 1 > answer->cap) {
        size_t cap = answer->cap ? answer->cap * 2 : 2048;
        while (cap < answer->len + len + 1) {
            cap *= 2;
        }
        char *grown = realloc(answer->data, cap);
        if (!grown) {
            answer->failed = true;
            return false;
        }
        answer->data = grown;
        answer->cap = cap;
    }
    memcpy(answer->data + answer->len, data, len);
    answer->len += len;
    answer->data[answer->len] = 0;
    return true;
}

static esp_err_t on_event(esp_http_client_event_t *event)
{
    answer_t *answer = event->user_data;
    if (event->event_id == HTTP_EVENT_ON_HEADER) {
        // GitHub's rate limits (GitHubClient.kt): a retry-after header, or no requests left
        if (strcasecmp(event->header_key, "retry-after") == 0 ||
            (strcasecmp(event->header_key, "x-ratelimit-remaining") == 0 && strcmp(event->header_value, "0") == 0)) {
            answer->rate_limited = true;
        }
    } else if (event->event_id == HTTP_EVENT_ON_DATA) {
        add(answer, event->data, (size_t)event->data_len);
    }
    return ESP_OK;
}

static esp_http_client_handle_t client_for(const char *url, int timeout_ms, answer_t *answer)
{
    esp_http_client_config_t config = {
        .url = url,
        .timeout_ms = timeout_ms,
        .event_handler = on_event,
        .user_data = answer,
        .crt_bundle_attach = esp_crt_bundle_attach,
        .buffer_size_tx = 1024,
        .keep_alive_enable = true,
    };
    return esp_http_client_init(&config);
}

bool http_github(void *ctx, const char *method, const char *url, const char *body, cap_http_response_t *response)
{
    (void)ctx;
    *response = (cap_http_response_t){0};
    char *real = real_url(url);
    answer_t answer = {.max = ANSWER_MAX};
    if (!real || !keys.loaded) {
        free(real);
        return false;
    }
    bool ok = false;
    // A kept-open connection may have been closed by the other side: one more try on a new one.
    for (int attempt = 0; attempt < 2 && !ok; attempt++) {
        free(answer.data);
        answer = (answer_t){.max = ANSWER_MAX};
        bool reused = keys.github != NULL;
        if (!keys.github) {
            keys.github = client_for(real, GITHUB_TIMEOUT_MS, &answer);
            if (!keys.github) {
                break;
            }
        }
        esp_http_client_handle_t client = keys.github;
        esp_http_client_set_url(client, real);
        esp_http_client_set_user_data(client, &answer);
        esp_http_client_set_method(client, strcmp(method, "PUT") == 0 ? HTTP_METHOD_PUT : HTTP_METHOD_GET);
        esp_http_client_set_header(client, "Authorization", keys.auth);
        esp_http_client_set_header(client, "Accept", "application/vnd.github+json");
        esp_http_client_set_header(client, "X-GitHub-Api-Version", "2022-11-28"); // as the phone
        esp_http_client_set_header(client, "User-Agent", keys.agent);
        if (body) {
            esp_http_client_set_header(client, "Content-Type", "application/json");
            esp_http_client_set_post_field(client, body, (int)strlen(body));
        } else {
            esp_http_client_delete_header(client, "Content-Type");
            esp_http_client_set_post_field(client, NULL, 0);
        }
        ok = esp_http_client_perform(client) == ESP_OK && !answer.failed;
        if (ok) {
            response->status = esp_http_client_get_status_code(client);
        } else {
            http_close();
            if (!reused) {
                break;
            }
        }
    }
    free(real);
    if (!ok) {
        free(answer.data);
        return false;
    }
    response->body = answer.data ? answer.data : strdup("");
    response->rate_limited = answer.rate_limited;
    return response->body != NULL;
}

// Reads the whole answer of a request made with open/write. The bytes reach `answer` through
// on_event (the client reports body data there for this kind of request too); the buffer here
// only drives the reading.
static bool read_answer(esp_http_client_handle_t client, answer_t *answer)
{
    char block[1024];
    for (;;) {
        int got = esp_http_client_read(client, block, sizeof block);
        if (got < 0 || answer->failed) {
            return false;
        }
        if (got == 0) {
            return esp_http_client_is_complete_data_received(client);
        }
    }
}

static bool write_all(esp_http_client_handle_t client, const char *data, size_t len)
{
    while (len) {
        int sent = esp_http_client_write(client, data, (int)len);
        if (sent <= 0) {
            return false;
        }
        data += sent;
        len -= (size_t)sent;
    }
    return true;
}

bool http_gemini(void *ctx, const char *prefix, const clip_audio_t *audio, const char *suffix, int *status, char **body)
{
    (void)ctx;
    *status = 0;
    *body = NULL;
    char *real = real_url(CAP_GEMINI_ENDPOINT);
    answer_t answer = {.max = ANSWER_MAX};
    esp_http_client_handle_t client = real && keys.loaded ? client_for(real, GEMINI_TIMEOUT_MS, &answer) : NULL;
    uint8_t *raw = malloc(BLOCK);
    char *text = malloc(cap_base64_len(BLOCK));
    bool ok = false;
    if (!client || !raw || !text) {
        goto end;
    }
    cap_gemini_request_t request = {.prefix = (char *)prefix, .suffix = (char *)suffix};
    size_t total = cap_gemini_content_length(&request, audio->size);
    esp_http_client_set_method(client, HTTP_METHOD_POST);
    esp_http_client_set_header(client, "x-goog-api-key", keys.gemini_key);
    esp_http_client_set_header(client, "Content-Type", "application/json");
    esp_http_client_set_header(client, "User-Agent", keys.agent);
    if (esp_http_client_open(client, (int)total) != ESP_OK || !write_all(client, prefix, strlen(prefix))) {
        goto end;
    }
    size_t left = audio->size;
    while (left) {
        // full blocks until the last, or the base64 of one block would be padded mid-stream
        size_t want = left < BLOCK ? left : BLOCK, have = 0;
        while (have < want) {
            size_t got = audio->read(audio->ctx, raw + have, want - have);
            if (got == 0) {
                goto end; // the file is shorter than it said: the length sent would be wrong
            }
            have += got;
        }
        size_t chars = cap_base64_encode(raw, have, text);
        if (!write_all(client, text, chars)) {
            goto end;
        }
        left -= have;
    }
    if (!write_all(client, suffix, strlen(suffix)) || esp_http_client_fetch_headers(client) < 0) {
        goto end;
    }
    *status = esp_http_client_get_status_code(client);
    ok = read_answer(client, &answer) && *status > 0;
end:
    if (client) {
        esp_http_client_close(client);
        esp_http_client_cleanup(client);
    }
    free(real);
    free(raw);
    free(text);
    if (!ok) {
        free(answer.data);
        return false;
    }
    *body = answer.data ? answer.data : strdup("");
    return *body != NULL;
}

static bool get(const char *url, bool github_auth, answer_t *answer, int *status)
{
    char *real = real_url(url);
    esp_http_client_handle_t client = real ? client_for(real, GITHUB_TIMEOUT_MS, answer) : NULL;
    bool ok = false;
    *status = 0;
    if (client) {
        if (keys.agent[0]) {
            esp_http_client_set_header(client, "User-Agent", keys.agent);
        }
        if (github_auth && keys.loaded) {
            // a file of a private repo, as it is, of any size
            esp_http_client_set_header(client, "Authorization", keys.auth);
            esp_http_client_set_header(client, "Accept", "application/vnd.github.raw+json");
            esp_http_client_set_header(client, "X-GitHub-Api-Version", "2022-11-28");
        }
        ok = esp_http_client_perform(client) == ESP_OK && !answer->failed &&
             esp_http_client_is_complete_data_received(client);
        *status = esp_http_client_get_status_code(client);
        esp_http_client_cleanup(client);
    }
    free(real);
    return ok;
}

bool http_get(const char *url, bool github_auth, size_t max, int *status, char **body, size_t *len)
{
    answer_t answer = {.max = max};
    *body = NULL;
    if (!get(url, github_auth, &answer, status)) {
        free(answer.data);
        return false;
    }
    *body = answer.data ? answer.data : strdup("");
    if (len) {
        *len = answer.len;
    }
    return *body != NULL;
}

bool http_download(const char *url, bool github_auth, int *status, bool (*block)(void *ctx, const uint8_t *data, size_t len),
                   void *ctx)
{
    answer_t answer = {.block = block, .block_ctx = ctx};
    return get(url, github_auth, &answer, status);
}
