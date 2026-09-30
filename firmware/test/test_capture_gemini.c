// SOURCES: firmware/components/capture/cap_text.c firmware/components/capture/cap_time.c firmware/components/capture/cap_note.c firmware/components/capture/cap_gemini.c
// The Gemini half of the Capture client, against Capture's own GeminiTest.kt and RateGateTest.kt
// (cap_note.c is linked only for cap_answer_free). No request is ever sent: every body is made here.
#include "cap_internal.h"
#include "capture.h"
#include "unit.h"

#define PREFIX_HEAD "{\"model\":\"gemini-3.5-flash-lite\",\"system_instruction\":\"PROMPT\",\"input\":["
#define AUDIO_PART "{\"type\":\"audio\",\"mime_type\":\"audio/ogg\",\"data\":\""
#define SUFFIX                                                                                           \
    "\"}],\"generation_config\":{\"temperature\":0.2,\"thinking_level\":\"high\"},"                      \
    "\"response_format\":{\"type\":\"text\",\"mime_type\":\"application/json\",\"schema\":"              \
    "{\"type\":\"object\",\"required\":[\"transcript\",\"title\",\"summary\"],\"additionalProperties\":false," \
    "\"properties\":{\"transcript\":{\"type\":\"string\"},\"title\":{\"type\":\"string\"},"              \
    "\"summary\":{\"type\":\"string\"}}}},\"store\":false}"

#define RATE_MESSAGE "HTTP 429: rate limit reached (free tier), will retry"
#define DAILY_MESSAGE "HTTP 429: daily free-tier limit reached, will retry after it resets"

// ---- helpers -------------------------------------------------------------------------------------

// A whole file (malloc'd, NUL-terminated); NULL when it cannot be read.
static char *slurp(const char *path, size_t *len)
{
    FILE *f = fopen(path, "rb");
    if (!f) {
        return NULL;
    }
    cap_buf_t buf = {0};
    char block[4096];
    size_t n;
    while ((n = fread(block, 1, sizeof block, f)) > 0) {
        cap_buf_add(&buf, block, n);
    }
    fclose(f);
    if (len) {
        *len = buf.len;
    }
    return cap_buf_take(&buf);
}

// firmware/components/capture/prompts/<name>, found from this file's own directory.
static void prompt_path(const char *name, char *out, size_t size)
{
    const char *file = __FILE__;
    const char *slash = strrchr(file, '/');
    snprintf(out, size, "%.*s/../components/capture/prompts/%s", slash ? (int)(slash - file) : 1, slash ? file : ".",
             name);
}

static char *shipped(const char *name, size_t *len)
{
    char path[1024];
    prompt_path(name, path, sizeof path);
    return slurp(path, len);
}

// GeminiTest.kt's interaction(): one model_output step holding `text`, and a usage block.
static char *interaction(const char *text, const char *status)
{
    cap_buf_t b = {0};
    cap_buf_str(&b, "{\"status\":");
    cap_buf_json(&b, status);
    cap_buf_str(&b, ",\"steps\":[{\"type\":\"user_input\"},{\"type\":\"model_output\",\"content\":[{\"type\":\"text\",\"text\":");
    cap_buf_json(&b, text);
    cap_buf_str(&b, "}]}],\"usage\":{\"total_input_tokens\":900,\"total_output_tokens\":40}}");
    return cap_buf_take(&b);
}

static int64_t at_ms(const char *iso)
{
    int64_t s = 0;
    CHECK(cap_iso_parse(iso, &s));
    return s * 1000;
}

static cap_gemini_result_t read_answer(int code, const char *body)
{
    cap_gemini_result_t r;
    cap_gemini_interpret(code, body, 0, &r);
    return r;
}

static cap_gemini_result_t status_result(const char *status)
{
    cap_gemini_result_t r = {.kind = CAP_GEMINI_NOT_COMPLETED, .retry_after_ms = -1};
    snprintf(r.status, sizeof r.status, "%s", status);
    return r;
}

static bool parses(const char *text)
{
    cap_json_t root;
    return cap_json_parse(text, &root);
}

// The text of a JSON string literal, compared with `expected`.
static void check_decoded(const char *literal, const char *expected)
{
    cap_json_t root;
    CHECK(cap_json_parse(literal, &root));
    char *text = cap_json_string(root);
    CHECK_STR(text, expected);
    free(text);
}

// ---- the request ---------------------------------------------------------------------------------

static void request_body_exact(void)
{
    char *schema = shipped("response_schema.json", NULL);
    CHECK(schema != NULL);
    cap_gemini_request_t r;
    CHECK(cap_gemini_request("PROMPT", schema, NULL, "audio/ogg", &r));
    CHECK_STR(r.prefix, PREFIX_HEAD "{\"type\":\"text\",\"text\":\"Process this recording.\"}," AUDIO_PART);
    CHECK_STR(r.suffix, SUFFIX);
    cap_gemini_request_free(&r);
    CHECK(r.prefix == NULL && r.suffix == NULL);
    free(schema);
}

// SPEC §9, Additions: the note so far, then what to do, then the recording.
static void append_request_body_exact(void)
{
    char *schema = shipped("response_schema.json", NULL);
    cap_gemini_request_t r;
    CHECK(cap_gemini_request("PROMPT", schema, "The note so far.", "audio/ogg", &r));
    CHECK_STR(r.prefix, PREFIX_HEAD "{\"type\":\"text\",\"text\":\"The note so far.\"},"
                                    "{\"type\":\"text\",\"text\":\"Process this recording, which adds to the note above.\"},"
                                    AUDIO_PART);
    CHECK_STR(r.suffix, SUFFIX);
    cap_gemini_request_free(&r);
    // the note's text is escaped as kotlinx.serialization does
    CHECK(cap_gemini_request("PROMPT", schema, "a \"b\"\nc\\d\te\x01\x7f \xd7\xa9", "audio/ogg", &r));
    CHECK(strstr(r.prefix, "[{\"type\":\"text\",\"text\":\"a \\\"b\\\"\\nc\\\\d\\te\\u0001\x7f \xd7\xa9\"},") != NULL);
    cap_gemini_request_free(&r);
    free(schema);
}

static void prompt_loses_trailing_whitespace_only(void)
{
    cap_gemini_request_t r;
    const char *prompts[] = {"PROMPT\n", "PROMPT \t\r\n\n", "PROMPT\xc2\xa0\xe2\x80\xa8\xe3\x80\x80\n", "PROMPT"};
    for (size_t i = 0; i < sizeof prompts / sizeof *prompts; i++) {
        CHECK(cap_gemini_request(prompts[i], "{}", NULL, "audio/ogg", &r));
        CHECK(r.prefix && strncmp(r.prefix, PREFIX_HEAD, strlen(PREFIX_HEAD)) == 0);
        cap_gemini_request_free(&r);
    }
    CHECK(cap_gemini_request("  Line \"one\"\nline two\n", "{}", NULL, "audio/ogg", &r)); // trimEnd, not trim
    CHECK(strstr(r.prefix, "\"system_instruction\":\"  Line \\\"one\\\"\\nline two\",\"input\":[") != NULL);
    cap_gemini_request_free(&r);
}

static void schema_is_reserialised(void)
{
    cap_gemini_request_t r;
    CHECK(cap_gemini_request("PROMPT", " { \"a\" : [ 1 , -2.5e3 , true , null , \"x\\u0041\\/\\n\" ] ,\n\t\"b\" : { } , \"c\" : [ ] } \n",
                             NULL, "audio/ogg", &r));
    CHECK(strstr(r.suffix, "\"schema\":{\"a\":[1,-2.5e3,true,null,\"xA/\\n\"],\"b\":{},\"c\":[]}},\"store\":false}") != NULL);
    cap_gemini_request_free(&r);
}

static void bad_schema_is_refused(void)
{
    const char *bad[] = {"", "{", "{} x", "{\"type\":}", "{'type':'object'}", "[1,]"};
    for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
        cap_gemini_request_t r = {(char *)"x", (char *)"y"};
        CHECK(!cap_gemini_request("PROMPT", bad[i], NULL, "audio/ogg", &r));
        CHECK(r.prefix == NULL && r.suffix == NULL);
    }
}

static void content_length(void)
{
    cap_gemini_request_t r;
    CHECK(cap_gemini_request("PROMPT", "{}", NULL, "audio/ogg", &r));
    size_t fixed = strlen(r.prefix) + strlen(r.suffix);
    const size_t base64[] = {0, 4, 4, 4, 8};
    for (size_t n = 0; n <= 4; n++) {
        CHECK_INT(cap_gemini_content_length(&r, n), fixed + base64[n]);
    }
    CHECK_INT(cap_gemini_content_length(&r, 300000), fixed + 400000);
    cap_gemini_request_free(&r);
}

// requestBodyShape and appendRequestBodyShape: what Gemini gets is JSON with the audio in place.
static void whole_body_parses(void)
{
    char *schema = shipped("response_schema.json", NULL);
    const char *notes[] = {NULL, "The note so far."};
    for (size_t i = 0; i < 2; i++) {
        cap_gemini_request_t r;
        CHECK(cap_gemini_request("PROMPT", schema, notes[i], CAP_MIME_OGG, &r));
        cap_buf_t body = {0};
        cap_buf_str(&body, r.prefix);
        cap_buf_base64(&body, (const uint8_t *)"ABC", 3);
        cap_buf_str(&body, r.suffix);
        CHECK_INT(body.len, cap_gemini_content_length(&r, 3));
        char *text = cap_buf_take(&body);
        cap_json_t root, input, item, config, format, sent, shipped_schema, value;
        CHECK(cap_json_parse(text, &root));
        char *model = cap_json_member_string(root, "model");
        char *system = cap_json_member_string(root, "system_instruction");
        CHECK_STR(model, CAP_GEMINI_MODEL);
        CHECK_STR(system, "PROMPT");
        free(model);
        free(system);

        CHECK(cap_json_member(root, "input", &input));
        const char *want_type[] = {"text", "text", "audio"};
        const char *want_text[] = {"The note so far.", i ? CAP_GEMINI_USER_TEXT_APPEND : CAP_GEMINI_USER_TEXT};
        size_t count = 0, first = i ? 0 : 1;
        for (const char *cursor = NULL; cap_json_each(input, &cursor, &item) && count + first < 3; count++) {
            char *type = cap_json_member_string(item, "type");
            CHECK_STR(type, want_type[count + first]);
            free(type);
            if (count + first < 2) {
                char *said = cap_json_member_string(item, "text");
                CHECK_STR(said, want_text[count + first]);
                free(said);
            } else {
                char *mime = cap_json_member_string(item, "mime_type");
                char *data = cap_json_member_string(item, "data");
                CHECK_STR(mime, "audio/ogg");
                CHECK_STR(data, "QUJD");
                free(mime);
                free(data);
            }
        }
        CHECK_INT(count, i ? 3 : 2);

        CHECK(cap_json_member(root, "generation_config", &config));
        char *level = cap_json_member_string(config, "thinking_level");
        CHECK_STR(level, CAP_GEMINI_THINKING_LEVEL);
        free(level);
        CHECK(cap_json_member(config, "temperature", &value));
        CHECK(value.len == 3 && memcmp(value.at, "0.2", 3) == 0);

        CHECK(cap_json_member(root, "response_format", &format));
        char *mime = cap_json_member_string(format, "mime_type");
        CHECK_STR(mime, "application/json");
        free(mime);
        // the schema that goes up is the shipped one: same members, same order
        CHECK(cap_json_member(format, "schema", &sent));
        CHECK(cap_json_parse(schema, &shipped_schema));
        cap_json_t key_a, key_b, value_a, value_b;
        const char *a = NULL, *b = NULL;
        size_t members = 0;
        while (cap_json_each_member(sent, &a, &key_a, &value_a)) {
            CHECK(cap_json_each_member(shipped_schema, &b, &key_b, &value_b));
            CHECK(key_a.len == key_b.len && memcmp(key_a.at, key_b.at, key_a.len) == 0);
            CHECK_INT(cap_json_type(value_a), cap_json_type(value_b));
            members++;
        }
        CHECK_INT(members, 4);
        CHECK(!cap_json_each_member(shipped_schema, &b, &key_b, &value_b));
        CHECK(cap_json_member(root, "store", &value));
        CHECK_INT(cap_json_type(value), 'f');
        free(text);
        cap_gemini_request_free(&r);
    }
    free(schema);
}

// ---- base64 --------------------------------------------------------------------------------------

static void base64_known_vectors(void)
{
    const char *plain[] = {"", "f", "fo", "foo", "foob", "fooba", "foobar"};
    const char *coded[] = {"", "Zg==", "Zm8=", "Zm9v", "Zm9vYg==", "Zm9vYmE=", "Zm9vYmFy"};
    for (size_t i = 0; i < sizeof plain / sizeof *plain; i++) {
        char out[16] = {0};
        size_t n = cap_base64_encode((const uint8_t *)plain[i], strlen(plain[i]), out);
        CHECK_INT(n, strlen(coded[i]));
        CHECK_INT(cap_base64_len(strlen(plain[i])), strlen(coded[i]));
        CHECK_STR(out, coded[i]);
        size_t len = 99;
        char *back = cap_base64_decode(coded[i], &len);
        CHECK_STR(back, plain[i]);
        CHECK_INT(len, strlen(plain[i]));
        free(back);
    }
    const uint8_t high[] = {0xfb, 0xff, 0xfe, 0x00, 0xff};
    char out[9] = {0};
    CHECK_INT(cap_base64_encode(high, sizeof high, out), 8);
    CHECK_STR(out, "+//+AP8=");
    CHECK_INT(cap_base64_len(0), 0);
    CHECK_INT(cap_base64_len(1), 4);
    CHECK_INT(cap_base64_len(3), 4);
    CHECK_INT(cap_base64_len(4), 8);
    CHECK_INT(cap_base64_len(3000001), 4000004);
}

static void base64_in_blocks_is_the_same(void)
{
    enum { N = 1000 };
    uint8_t data[N];
    for (size_t i = 0; i < N; i++) {
        data[i] = (uint8_t)(i * 131 + 7);
    }
    char whole[N / 3 * 4 + 8], pieces[N / 3 * 4 + 8];
    size_t total = cap_base64_encode(data, N, whole);
    CHECK_INT(total, cap_base64_len(N));
    const size_t blocks[] = {3, 300, 999, 1023};
    for (size_t b = 0; b < sizeof blocks / sizeof *blocks; b++) {
        size_t o = 0;
        for (size_t i = 0; i < N; i += blocks[b]) {
            o += cap_base64_encode(data + i, N - i < blocks[b] ? N - i : blocks[b], pieces + o);
        }
        CHECK_INT(o, total);
        CHECK(memcmp(whole, pieces, total) == 0);
    }
    cap_buf_t buf = {0};
    cap_buf_str(&buf, "x");
    cap_buf_base64(&buf, data, N);
    CHECK_INT(buf.len, total + 1);
    CHECK(buf.data && memcmp(buf.data + 1, whole, total) == 0 && buf.data[buf.len] == 0);
    size_t len = 0;
    char *back = cap_base64_decode(buf.data + 1, &len);
    CHECK_INT(len, N);
    CHECK(back && memcmp(back, data, N) == 0);
    free(back);
    free(cap_buf_take(&buf));
}

static void base64_decode_as_github_sends_it(void)
{
    size_t len = 0;
    char *text = cap_base64_decode("Zm9v\nYmFy\r\nZm9v\nYg==\n", &len); // wrapped in lines
    CHECK_STR(text, "foobarfoob");
    CHECK_INT(len, 10);
    free(text);
    text = cap_base64_decode("Zm9vYg", &len); // padding left out
    CHECK_STR(text, "foob");
    CHECK_INT(len, 4);
    free(text);
    text = cap_base64_decode("Zm9vYmE", &len);
    CHECK_STR(text, "fooba");
    free(text);
    text = cap_base64_decode("AP8A", &len); // bytes, not only text
    CHECK_INT(len, 3);
    CHECK(text && memcmp(text, "\0\xff\0", 3) == 0);
    free(text);
    text = cap_base64_decode("", &len);
    CHECK_STR(text, "");
    CHECK_INT(len, 0);
    free(text);
    text = cap_base64_decode("Zg==", NULL); // the length is optional
    CHECK_STR(text, "f");
    free(text);
    CHECK(cap_base64_decode("Zm9vY", &len) == NULL); // six bits are not a byte
    CHECK(cap_base64_decode("Z", &len) == NULL);
    CHECK(cap_base64_decode("Zm9v\nY\n", &len) == NULL);
}

// ---- reading the answer --------------------------------------------------------------------------

static void reads_completed_interaction(void)
{
    char *body = interaction("{\"transcript\":\"Buy a skimmer. Maybe a pond.\",\"title\":\"Skimmer and pond\","
                             "\"summary\":\"- Buy a skimmer\\n- Maybe a pond\"}",
                             "completed");
    cap_gemini_result_t r = read_answer(200, body);
    CHECK_INT(r.kind, CAP_GEMINI_PARSED);
    CHECK_STR(r.answer.title, "Skimmer and pond");
    CHECK_STR(r.answer.summary, "- Buy a skimmer\n- Maybe a pond");
    CHECK_STR(r.answer.transcript, "Buy a skimmer. Maybe a pond.");
    CHECK_INT(r.input_tokens, 900);
    CHECK_INT(r.output_tokens, 40);
    CHECK_INT(r.retry_after_ms, -1);
    cap_gemini_result_free(&r);
    CHECK(r.answer.title == NULL);
    r = read_answer(299, body); // any 2xx
    CHECK_INT(r.kind, CAP_GEMINI_PARSED);
    cap_gemini_result_free(&r);
    free(body);
    // unknown keys are ignored, as ignoreUnknownKeys does
    body = interaction("{\"transcript\":\"\xd7\xa9\xd7\x9c\xd7\x95\xd7\x9d\",\"title\":\"t\",\"summary\":\"s\",\"mood\":1}", "completed");
    r = read_answer(200, body);
    CHECK_INT(r.kind, CAP_GEMINI_PARSED);
    CHECK_STR(r.answer.transcript, "\xd7\xa9\xd7\x9c\xd7\x95\xd7\x9d");
    cap_gemini_result_free(&r);
    free(body);
}

static void joins_text_parts_of_model_output(void)
{
    cap_gemini_result_t r = read_answer(
        200, "{\"status\":\"completed\",\"steps\":[{\"type\":\"thought\"},{\"type\":\"model_output\",\"content\":["
             "{\"type\":\"text\",\"text\":\"{\\\"transcript\\\":\\\"\\\",\"},"
             "{\"type\":\"text\",\"text\":\"\\\"title\\\":\\\"\\\",\\\"summary\\\":\\\"\\\"}\"}]}]}");
    CHECK_INT(r.kind, CAP_GEMINI_PARSED);
    CHECK_STR(r.answer.title, "");
    CHECK_STR(r.answer.summary, "");
    CHECK_STR(r.answer.transcript, "");
    CHECK_INT(r.input_tokens, -1);
    CHECK_INT(r.output_tokens, -1);
    cap_gemini_result_free(&r);
}

static void token_counts(void)
{
    const char *steps = "\"steps\":[{\"type\":\"model_output\",\"content\":[{\"type\":\"text\","
                        "\"text\":\"{\\\"transcript\\\":\\\"x\\\",\\\"title\\\":\\\"t\\\",\\\"summary\\\":\\\"s\\\"}\"}]}]";
    const char *usage[] = {"{\"total_input_tokens\":123456789012,\"total_output_tokens\":0}", "{\"total_input_tokens\":7}",
                           "{\"total_input_tokens\":1.5,\"total_output_tokens\":null}", "{}", "[]", "7"};
    const long long input[] = {123456789012LL, 7, -1, -1, -1, -1};
    const long long output[] = {0, -1, -1, -1, -1, -1};
    for (size_t i = 0; i < sizeof usage / sizeof *usage; i++) {
        char body[512];
        snprintf(body, sizeof body, "{\"status\":\"completed\",%s,\"usage\":%s}", steps, usage[i]);
        cap_gemini_result_t r = read_answer(200, body);
        CHECK_INT(r.kind, CAP_GEMINI_PARSED);
        CHECK_INT(r.input_tokens, input[i]);
        CHECK_INT(r.output_tokens, output[i]);
        cap_gemini_result_free(&r);
    }
}

static void non_completed_status_is_reported(void)
{
    char *body = interaction("{}", "in_progress");
    cap_gemini_result_t r = read_answer(200, body);
    CHECK_INT(r.kind, CAP_GEMINI_NOT_COMPLETED);
    CHECK_STR(r.status, "in_progress");
    CHECK(r.answer.title == NULL);
    cap_gemini_result_free(&r);
    free(body);
    r = read_answer(200, "{\"status\":\"budget_exceeded\"}");
    CHECK_INT(r.kind, CAP_GEMINI_NOT_COMPLETED);
    CHECK_STR(r.status, "budget_exceeded");
    cap_gemini_result_free(&r);
}

// unreadableAnswersRetryAndKeepTheBody and aRecordingAnswerWithoutATranscriptRetries
static void unreadable_answers_retry(void)
{
    char *bad_type = interaction("{\"transcript\":\"x\",\"title\":\"t\",\"summary\":[\"s\"]}", "completed");
    char *missing_field = interaction("{\"transcript\":\"x\",\"title\":\"t\"}", "completed");
    char *no_transcript = interaction("{\"title\":\"Skimmer\",\"summary\":\"Buy a skimmer.\"}", "completed");
    char *null_field = interaction("{\"transcript\":null,\"title\":\"t\",\"summary\":\"s\"}", "completed");
    char *not_json = interaction("Sorry, I could not hear that.", "completed");
    char *junk_after = interaction("{\"transcript\":\"x\",\"title\":\"t\",\"summary\":\"s\"} thanks", "completed");
    const char *bodies[] = {"<html>",
                            "",
                            "[]",
                            "{\"status\":\"completed\",\"steps\":[{\"type\":\"thought\"}]}",
                            "{\"status\":\"completed\"}",
                            "{\"status\":7,\"steps\":[]}",
                            "{\"steps\":[]}",
                            bad_type,
                            missing_field,
                            no_transcript,
                            null_field,
                            not_json,
                            junk_after};
    const char *messages[] = {"Unreadable response from Gemini",
                              "Unreadable response from Gemini",
                              "Unreadable response from Gemini",
                              "Gemini returned no text",
                              "Gemini returned no text",
                              "Response has no status",
                              "Response has no status",
                              "Gemini's answer did not match the schema",
                              "Gemini's answer did not match the schema",
                              "Gemini's answer did not match the schema",
                              "Gemini's answer did not match the schema",
                              "Gemini's answer did not match the schema",
                              "Gemini's answer did not match the schema"};
    for (size_t i = 0; i < sizeof bodies / sizeof *bodies; i++) {
        cap_gemini_result_t r = read_answer(200, bodies[i]);
        CHECK_INT(r.kind, CAP_GEMINI_FAILED);
        CHECK_STR(r.message, messages[i]);
        CHECK(!r.terminal);
        CHECK(r.unreadable);
        CHECK_INT(r.retry_after_ms, -1);
        CHECK(r.answer.title == NULL && r.answer.summary == NULL && r.answer.transcript == NULL);
        cap_gemini_result_free(&r);
    }
    cap_gemini_result_t r;
    cap_gemini_interpret(200, NULL, 0, &r); // no body at all
    CHECK_INT(r.kind, CAP_GEMINI_FAILED);
    CHECK(r.unreadable && !r.terminal);
    free(bad_type);
    free(missing_field);
    free(no_transcript);
    free(null_field);
    free(not_json);
    free(junk_after);
}

static void http_errors(void)
{
    cap_gemini_result_t r =
        read_answer(400, "{\"error\":{\"code\":400,\"message\":\"Invalid audio\",\"status\":\"INVALID_ARGUMENT\"}}");
    CHECK_INT(r.kind, CAP_GEMINI_FAILED);
    CHECK_STR(r.message, "HTTP 400: Invalid audio");
    CHECK(r.terminal);
    CHECK(!r.unreadable);
    CHECK_INT(r.retry_after_ms, -1);
    const int terminal[] = {400, 401, 403, 404, 499};
    for (size_t i = 0; i < sizeof terminal / sizeof *terminal; i++) {
        r = read_answer(terminal[i], "");
        CHECK_INT(r.kind, CAP_GEMINI_FAILED);
        CHECK(r.terminal);
    }
    CHECK_STR(read_answer(401, "").message, "HTTP 401: no details");
    CHECK_STR(read_answer(403, " \n\t").message, "HTTP 403: no details");
    const int retry[] = {408, 429, 500, 502, 503, 504, 599, 302, 199};
    for (size_t i = 0; i < sizeof retry / sizeof *retry; i++) {
        r = read_answer(retry[i], "busy");
        CHECK_INT(r.kind, CAP_GEMINI_FAILED);
        CHECK(!r.terminal);
        CHECK(!r.unreadable);
    }
    r = read_answer(503, "busy");
    CHECK_STR(r.message, "HTTP 503: busy");
    CHECK_INT(r.retry_after_ms, -1);
    const char *quota = "{\"error\":{\"message\":\"You exceeded your current quota, please check your plan.\\n* Quota "
                        "exceeded\",\"code\":\"too_many_requests\"}}";
    r = read_answer(429, quota);
    CHECK_STR(r.message, RATE_MESSAGE);
    CHECK_INT(r.retry_after_ms, 60000); // a 429 that names no wait gets a full window
    r = read_answer(429, "{\"error\":{\"message\":\"Quota exceeded. Please retry in 14.898357626s.\",\"code\":\"too_many_requests\"}}");
    CHECK_INT(r.retry_after_ms, 14898);
    r = read_answer(429, "{\"error\":{\"message\":\"x\",\"details\":[{\"@type\":\"type.googleapis.com/google.rpc.RetryInfo\","
                         "\"retryDelay\": \"7s\"}]}}");
    CHECK_INT(r.retry_after_ms, 7000);
    CHECK(!r.terminal);
    r = read_answer(400, "{\"error\":{\"message\":\"Invalid audio\\nmore detail\"}}");
    CHECK_STR(r.message, "HTTP 400: Invalid audio");
    // the first line of the body when it is not Google's error object
    CHECK_STR(read_answer(502, "  \r\n Bad gateway \r\nnginx").message, "HTTP 502: Bad gateway");
    CHECK_STR(read_answer(500, "{\"error\":{\"message\":7}}").message, "HTTP 500: {\"error\":{\"message\":7}}");
    CHECK_STR(read_answer(500, "{\"error\":\"down\"}").message, "HTTP 500: {\"error\":\"down\"}");
    CHECK_STR(read_answer(500, "{\"error\":{\"message\":\"  \"}}").message, "HTTP 500: no details");
}

static void network_error(void)
{
    cap_gemini_result_t r = read_answer(0, NULL);
    CHECK_INT(r.kind, CAP_GEMINI_FAILED);
    CHECK_STR(r.message, "Network error");
    CHECK(!r.terminal);
    CHECK(!r.unreadable);
    CHECK_INT(r.retry_after_ms, -1);
    r = read_answer(0, "{\"status\":\"completed\"}");
    CHECK_STR(r.message, "Network error");
}

// take(200), counted in code points here (capture.h's message is 256 bytes)
static void error_message_is_cut(void)
{
    char body[1024], want[300];
    char a250[251], a190[191];
    memset(a250, 'a', 250);
    a250[250] = 0;
    memset(a190, 'a', 190);
    a190[190] = 0;
    snprintf(body, sizeof body, "{\"error\":{\"message\":\"%s\"}}", a250);
    snprintf(want, sizeof want, "HTTP 400: %.200s", a250);
    CHECK_STR(read_answer(400, body).message, want);
    CHECK_INT(strlen(read_answer(400, body).message), 210);
    snprintf(body, sizeof body, "{\"error\":{\"message\":\"%.200s\"}}", a250); // exactly 200 is whole
    CHECK_STR(read_answer(400, body).message, want);
    CHECK_STR(read_answer(503, a250).message + 8, want + 8); // a plain body too

    // ten 2-byte characters and 240 more: 200 characters are 210 bytes
    const char *e10 = "\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9";
    snprintf(body, sizeof body, "{\"error\":{\"message\":\"%s%s\"}}", e10, a250);
    snprintf(want, sizeof want, "HTTP 400: %s%s", e10, a190);
    CHECK_STR(read_answer(400, body).message, want);

    // text too long for the message in bytes still ends on a whole character
    cap_buf_t b = {0};
    cap_buf_str(&b, "{\"error\":{\"message\":\"");
    for (int i = 0; i < 300; i++) {
        cap_buf_str(&b, "\xd7\xa9");
    }
    cap_buf_str(&b, "\"}}");
    char *hebrew = cap_buf_take(&b);
    cap_gemini_result_t r = read_answer(400, hebrew);
    size_t len = strlen(r.message);
    CHECK(len > 10 + 200 && len < sizeof r.message && (len - 10) % 2 == 0);
    for (size_t i = 10; i + 1 < len; i += 2) {
        CHECK(memcmp(r.message + i, "\xd7\xa9", 2) == 0);
    }
    free(hebrew);
}

static void daily_quota_is_told_from_a_minute_s(void)
{
    CHECK(cap_gemini_is_daily_quota("{\"details\":[{\"quotaId\":\"GenerateRequestsPerDayPerProjectPerModel-FreeTier\"}]}"));
    CHECK(!cap_gemini_is_daily_quota("generate_content_free_tier_requests, limit: 5, model: gemini-3.8-flash"));
    CHECK(cap_gemini_is_daily_quota("generate_content_free_tier_requests, limit: 20, model: gemini-3.8-flash"));
    CHECK(!cap_gemini_is_daily_quota("(limit: 15 requests per minute on Free Tier)"));
    CHECK(!cap_gemini_is_daily_quota("(limit: 500 requests PER MINUTE on Free Tier)")); // the window decides
    CHECK(cap_gemini_is_daily_quota("(limit: 500 requests per day on Free Tier)"));
    CHECK(cap_gemini_is_daily_quota("(limit: 5 requests Per Day on Free Tier)"));
    CHECK(!cap_gemini_is_daily_quota("PerDay quota, measured per minute"));
    CHECK(!cap_gemini_is_daily_quota("perday")); // the quota id is matched as written
    CHECK(!cap_gemini_is_daily_quota("limit: 15"));
    CHECK(cap_gemini_is_daily_quota("limit: 16"));
    CHECK(!cap_gemini_is_daily_quota("limit: 5, then limit: 500")); // the first number counts
    CHECK(cap_gemini_is_daily_quota("limit: none, limit: 500"));
    CHECK(!cap_gemini_is_daily_quota("limit: 99999999999")); // toIntOrNull
    CHECK(!cap_gemini_is_daily_quota("limit: "));
    CHECK(!cap_gemini_is_daily_quota("Limit: 500"));
    CHECK(!cap_gemini_is_daily_quota(""));
}

static void retry_delay(void)
{
    CHECK_INT(cap_gemini_retry_delay_ms("{\"retryDelay\": \"7s\"}"), 7000);
    CHECK_INT(cap_gemini_retry_delay_ms("{\"retryDelay\":\"12.345s\"}"), 12345);
    CHECK_INT(cap_gemini_retry_delay_ms("{\"retryDelay\"\n\t:\r\n \"1.05s\"}"), 1050);
    CHECK_INT(cap_gemini_retry_delay_ms("{\"retryDelay\":\"0s\"}"), 0);
    CHECK_INT(cap_gemini_retry_delay_ms("{\"retryDelay\":\"0.0009s\"}"), 0);
    CHECK_INT(cap_gemini_retry_delay_ms("{\"retryDelay\":\"86400s\"}"), 86400000);
    CHECK_INT(cap_gemini_retry_delay_ms("Please retry in 14.898357626s."), 14898);
    CHECK_INT(cap_gemini_retry_delay_ms("Please retry in 52s or upgrade your tier"), 52000);
    CHECK_INT(cap_gemini_retry_delay_ms("PLEASE RETRY IN 3S"), 3000);
    CHECK_INT(cap_gemini_retry_delay_ms("retry in 0.5s"), 500);
    // the field wins over the text, wherever it is
    CHECK_INT(cap_gemini_retry_delay_ms("Please retry in 40s. {\"retryDelay\":\"7s\"}"), 7000);
    // a field that is not a whole match is passed over
    CHECK_INT(cap_gemini_retry_delay_ms("{\"retryDelay\":\"7\"} retry in 2s"), 2000);
    CHECK_INT(cap_gemini_retry_delay_ms("{\"retryDelay\":7} {\"retryDelay\":\"9s\"}"), 9000);
    CHECK_INT(cap_gemini_retry_delay_ms("{\"retryDelay\":\"7S\"}"), -1);
    CHECK_INT(cap_gemini_retry_delay_ms("{\"retryDelay\":\"7s \"}"), -1);
    CHECK_INT(cap_gemini_retry_delay_ms("{\"retryDelay\":\"1.2.3s\"}"), -1);
    CHECK_INT(cap_gemini_retry_delay_ms("retry in soon, retry in 2s"), 2000);
    CHECK_INT(cap_gemini_retry_delay_ms("retry in .5s"), -1);
    CHECK_INT(cap_gemini_retry_delay_ms("retry in 5 s"), -1);
    CHECK_INT(cap_gemini_retry_delay_ms("retry in 1.s"), -1);
    CHECK_INT(cap_gemini_retry_delay_ms("retry in 5 minutes"), -1);
    CHECK_INT(cap_gemini_retry_delay_ms("retry in -5s"), -1);
    CHECK_INT(cap_gemini_retry_delay_ms("retry in "), -1);
    CHECK_INT(cap_gemini_retry_delay_ms("\"retryDelay\""), -1);
    CHECK_INT(cap_gemini_retry_delay_ms(""), -1);
}

// dailyQuotaWaitsForTheReset: the two 429s seen on 2026-09-13, trimmed.
static void daily_quota_waits_for_the_reset(void)
{
    const char *per_minute =
        "{\"error\":{\"message\":\"You exceeded your current quota.\\n* Quota exceeded for metric: "
        "generativelanguage.googleapis.com/generate_content_free_tier_requests, limit: 5, model: gemini-3.8-flash\\n"
        "Please retry in 14.898357626s.\",\"code\":\"too_many_requests\"}}";
    const char *per_day =
        "{\"error\":{\"message\":\"You exceeded your current quota.\\n* Quota exceeded for metric: "
        "generativelanguage.googleapis.com/generate_content_free_tier_requests, limit: 20, model: gemini-3.8-flash\\n"
        "Please retry in 47.324331527s.\",\"code\":\"too_many_requests\"}}";
    int64_t now = at_ms("2026-09-13T04:51:00Z"); // 21:51 Pacific, 12 Sep
    cap_gemini_result_t r;
    cap_gemini_interpret(429, per_minute, now, &r);
    CHECK_INT(r.kind, CAP_GEMINI_FAILED);
    CHECK_INT(r.retry_after_ms, 14898);
    CHECK_STR(r.message, RATE_MESSAGE);
    CHECK(!r.terminal);
    cap_gemini_interpret(429, per_day, now, &r);
    CHECK_STR(r.message, DAILY_MESSAGE);
    CHECK(!r.terminal);
    CHECK(!r.unreadable);
    CHECK_INT(r.retry_after_ms, (2 * 3600 + 9 * 60 + 60) * 1000LL); // midnight Pacific is 07:00Z; plus the margin
}

// newerRateLimitWordingNamesTheWindow: gemini-3.5-flash-lite's 429 of 2026-09-20.
static void newer_rate_limit_wording_names_the_window(void)
{
    const char *per_minute = "{\"error\":{\"message\":\"Rate limit exceeded for model gemini-3.5-flash-lite (limit: 15 "
                             "requests per minute on Free Tier). Please retry in 52s or upgrade your tier at "
                             "https://ai.dev/rate-limit.\",\"code\":\"too_many_requests\"}}";
    const char *per_day = "{\"error\":{\"message\":\"Rate limit exceeded for model gemini-3.5-flash-lite (limit: 500 "
                          "requests per day on Free Tier). Please retry in 52s or upgrade your tier at "
                          "https://ai.dev/rate-limit.\",\"code\":\"too_many_requests\"}}";
    int64_t now = at_ms("2026-09-20T04:51:00Z"); // 21:51 Pacific, 19 Sep
    cap_gemini_result_t r;
    CHECK(!cap_gemini_is_daily_quota(per_minute));
    cap_gemini_interpret(429, per_minute, now, &r);
    CHECK_INT(r.retry_after_ms, 52000);
    CHECK_STR(r.message, RATE_MESSAGE);
    CHECK(cap_gemini_is_daily_quota(per_day));
    cap_gemini_interpret(429, per_day, now, &r);
    CHECK_STR(r.message, DAILY_MESSAGE);
    CHECK_INT(r.retry_after_ms, (2 * 3600 + 9 * 60 + 60) * 1000LL); // the reset, not the 52 s it names
}

static void quota_reset_is_midnight_pacific_across_daylight_saving(void)
{
    const int64_t hour = 3600 * 1000, margin = 60000;
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-09-13T04:51:00Z")), (2 * 3600 + 9 * 60 + 60) * 1000LL);
    // 30 s before midnight Pacific: the reset is 30 s away, plus the minute's margin
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-10-31T23:59:30-07:00")), 90000);
    // the day the clocks go back (1 Nov 2026) is 25 hours long
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-11-01T00:00:00-07:00")), 25 * hour + margin);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-11-01T01:30:00-07:00")), 23 * hour + hour / 2 + margin);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-11-01T01:30:00-08:00")), 22 * hour + hour / 2 + margin); // 01:30 again
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-11-01T23:59:59-08:00")), 1000 + margin);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-11-02T00:00:00-08:00")), 24 * hour + margin);
    // the day they go forward (8 Mar 2026) is 23 hours long
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-03-07T23:59:30-08:00")), 90000);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-03-08T00:00:00-08:00")), 23 * hour + margin);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-03-08T01:59:59-08:00")), 21 * hour + 1000 + margin);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-03-08T03:00:00-07:00")), 21 * hour + margin);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2026-03-09T00:00:00-07:00")), 24 * hour + margin);
    // winter and summer, other years, and a time with milliseconds
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2027-01-01T07:59:59Z")), 1000 + margin);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2027-01-01T08:00:00Z")), 24 * hour + margin);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2028-02-29T12:00:00-08:00")), 12 * hour + margin);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2027-07-04T06:59:59Z") + 250), 750 + margin);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2027-03-14T00:00:00-08:00")), 23 * hour + margin);
    CHECK_INT(cap_until_quota_reset_ms(at_ms("2027-11-07T00:00:00-07:00")), 25 * hour + margin);
}

// ---- the gate and retries ------------------------------------------------------------------------

// RateGateTest.kt as time arithmetic: the app waits cap_gate_wait_ms, runs, then cap_gate_finished.
static void gate_one_at_a_time_and_spaced(void)
{
    cap_gate_t gate = {.min_interval_ms = 150};
    int64_t now = 1000000;
    int64_t ends[3], starts[3];
    for (int i = 0; i < 3; i++) {
        now += cap_gate_wait_ms(&gate, now);
        starts[i] = now;
        now += 20; // the request
        ends[i] = now;
        cap_gate_finished(&gate, now);
    }
    CHECK_INT(starts[0], 1000000); // a fresh gate lets the first one through
    CHECK_INT(starts[1] - ends[0], 150);
    CHECK_INT(starts[2] - ends[1], 150);
    CHECK_INT(cap_gate_wait_ms(&gate, now), 150);
    CHECK_INT(cap_gate_wait_ms(&gate, now + 100), 50);
    CHECK_INT(cap_gate_wait_ms(&gate, now + 150), 0);
    CHECK_INT(cap_gate_wait_ms(&gate, now + 99999), 0);
}

static void gate_hold_delays_the_next_request(void)
{
    cap_gate_t gate = {.min_interval_ms = 0};
    int64_t now = 5000;
    CHECK_INT(cap_gate_wait_ms(&gate, now), 0);
    cap_gate_hold(&gate, now, 200);
    cap_gate_finished(&gate, now); // the end of the request does not shorten the hold
    CHECK_INT(cap_gate_wait_ms(&gate, now), 200);
    CHECK_INT(cap_gate_wait_ms(&gate, now + 190), 10);
    CHECK_INT(cap_gate_wait_ms(&gate, now + 200), 0);
    cap_gate_hold(&gate, now, 100); // nor does a shorter hold
    CHECK_INT(cap_gate_wait_ms(&gate, now), 200);
    cap_gate_hold(&gate, now + 50, 300);
    CHECK_INT(cap_gate_wait_ms(&gate, now + 50), 300);
}

static void gate_long_hold_is_left_to_the_caller(void)
{
    cap_gate_t gate = {.min_interval_ms = 0};
    cap_gate_hold(&gate, 7000, 60000);
    CHECK(cap_gate_wait_ms(&gate, 7000) > 1000); // more than the caller will sit out
    CHECK_INT(cap_gate_wait_ms(&gate, 7000), 60000);
    CHECK_INT(CAP_GEMINI_MIN_INTERVAL_MS, 5000);
    gate = (cap_gate_t){.min_interval_ms = CAP_GEMINI_MIN_INTERVAL_MS};
    cap_gate_finished(&gate, 1);
    CHECK_INT(cap_gate_wait_ms(&gate, 1), 5000);
}

static void rate_limit_holds_the_gate_and_is_not_an_attempt(void)
{
    cap_gate_t gate = {.min_interval_ms = CAP_GEMINI_MIN_INTERVAL_MS};
    cap_attempts_t state = {0};
    int64_t now = at_ms("2026-09-20T04:51:00Z");
    cap_gemini_result_t r;
    cap_gemini_interpret(429, "limit: 15 requests per minute. Please retry in 52s", now, &r);
    for (int i = 0; i < 3 * CAP_GEMINI_MAX_ATTEMPTS; i++) {
        CHECK_INT(cap_gemini_failed(&state, &r, &gate, now), CAP_TRY_AGAIN);
    }
    CHECK_INT(state.attempts, 0);
    CHECK_STR(state.last_error, RATE_MESSAGE);
    cap_gate_finished(&gate, now);
    CHECK_INT(cap_gate_wait_ms(&gate, now), 52000);

    cap_gemini_interpret(429, "limit: 500 requests per day", now, &r);
    CHECK_INT(cap_gemini_failed(&state, &r, &gate, now), CAP_TRY_AGAIN);
    CHECK_INT(state.attempts, 0);
    CHECK_STR(state.last_error, DAILY_MESSAGE);
    CHECK_INT(cap_gate_wait_ms(&gate, now), (2 * 3600 + 9 * 60 + 60) * 1000LL);

    // an ordinary failure counts and leaves the gate alone
    cap_gate_t idle = {.min_interval_ms = CAP_GEMINI_MIN_INTERVAL_MS};
    r = read_answer(503, "busy");
    CHECK_INT(cap_gemini_failed(&state, &r, &idle, now), CAP_TRY_AGAIN);
    CHECK_INT(state.attempts, 1);
    CHECK_STR(state.last_error, "HTTP 503: busy");
    CHECK_INT(cap_gate_wait_ms(&idle, now), 0);
    // with the attempts used up a rate limit still does not add one
    state.attempts = CAP_GEMINI_MAX_ATTEMPTS - 1;
    cap_gemini_interpret(429, "", now, &r);
    CHECK_INT(cap_gemini_failed(&state, &r, &idle, now), CAP_TRY_AGAIN);
    CHECK_INT(state.attempts, CAP_GEMINI_MAX_ATTEMPTS - 1);
    CHECK_INT(cap_gate_wait_ms(&idle, now), 60000);
}

// badStatusIsTerminalOnlyWhenRepeated
static void bad_status_is_terminal_only_when_repeated(void)
{
    cap_gate_t gate = {0};
    cap_attempts_t state = {0};
    cap_gemini_result_t failed = status_result("failed"), incomplete = status_result("incomplete");
    CHECK_INT(cap_gemini_failed(&state, &failed, &gate, 0), CAP_TRY_AGAIN); // no error before
    CHECK_STR(state.last_error, "Gemini returned status failed");
    CHECK_INT(state.attempts, 1);
    CHECK_INT(cap_gemini_failed(&state, &failed, &gate, 0), CAP_GIVE_UP);
    CHECK_INT(state.attempts, 2);

    state = (cap_attempts_t){0};
    CHECK_INT(cap_gemini_failed(&state, &incomplete, &gate, 0), CAP_TRY_AGAIN);
    CHECK_INT(cap_gemini_failed(&state, &failed, &gate, 0), CAP_TRY_AGAIN); // a different one before
    CHECK_INT(cap_gemini_failed(&state, &incomplete, &gate, 0), CAP_TRY_AGAIN);
    CHECK_INT(cap_gemini_failed(&state, &incomplete, &gate, 0), CAP_GIVE_UP);
    CHECK_STR(state.last_error, "Gemini returned status incomplete");

    const char *bad[] = {"failed", "cancelled", "incomplete", "budget_exceeded"};
    for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
        cap_gemini_result_t r = status_result(bad[i]);
        state = (cap_attempts_t){0};
        snprintf(state.last_error, sizeof state.last_error, "HTTP 503: busy");
        CHECK_INT(cap_gemini_failed(&state, &r, &gate, 0), CAP_TRY_AGAIN);
        CHECK_INT(cap_gemini_failed(&state, &r, &gate, 0), CAP_GIVE_UP);
    }
    // other statuses always retry, until the attempts run out
    cap_gemini_result_t in_progress = status_result("in_progress");
    state = (cap_attempts_t){0};
    for (int i = 1; i < CAP_GEMINI_MAX_ATTEMPTS; i++) {
        CHECK_INT(cap_gemini_failed(&state, &in_progress, &gate, 0), CAP_TRY_AGAIN);
    }
    CHECK_STR(state.last_error, "Gemini returned status in_progress");
    CHECK_INT(cap_gemini_failed(&state, &in_progress, &gate, 0), CAP_GIVE_UP);
    CHECK_INT(state.attempts, CAP_GEMINI_MAX_ATTEMPTS);
    CHECK_INT(cap_gate_wait_ms(&gate, 0), 0);
}

static void eight_attempts_give_up_and_terminal_at_once(void)
{
    cap_gate_t gate = {0};
    cap_attempts_t state = {0};
    cap_gemini_result_t r = read_answer(503, "busy");
    for (int i = 1; i <= 7; i++) {
        CHECK_INT(cap_gemini_failed(&state, &r, &gate, 0), CAP_TRY_AGAIN);
        CHECK_INT(state.attempts, i);
    }
    CHECK_INT(cap_gemini_failed(&state, &r, &gate, 0), CAP_GIVE_UP);
    CHECK_INT(state.attempts, 8);
    CHECK_INT(CAP_GEMINI_MAX_ATTEMPTS, 8);

    state = (cap_attempts_t){0};
    r = read_answer(0, NULL); // no network is an attempt like any other
    CHECK_INT(cap_gemini_failed(&state, &r, &gate, 0), CAP_TRY_AGAIN);
    CHECK_INT(state.attempts, 1);
    CHECK_STR(state.last_error, "Network error");
    r = read_answer(200, "<html>");
    CHECK_INT(cap_gemini_failed(&state, &r, &gate, 0), CAP_TRY_AGAIN);
    CHECK_STR(state.last_error, "Unreadable response from Gemini");

    state = (cap_attempts_t){0};
    r = read_answer(400, "{\"error\":{\"message\":\"Invalid audio\"}}");
    CHECK_INT(cap_gemini_failed(&state, &r, &gate, 0), CAP_GIVE_UP);
    CHECK_INT(state.attempts, 1);
    CHECK_STR(state.last_error, "HTTP 400: Invalid audio");
    CHECK_INT(cap_gate_wait_ms(&gate, 0), 0);
}

// WorkManager's exponential backoff from 30 s, which it caps at 5 h
static void backoff(void)
{
    const int64_t max = 5 * 3600 * 1000;
    CHECK_INT(cap_backoff_ms(0), 0);
    CHECK_INT(cap_backoff_ms(-3), 0);
    int64_t want = 30000;
    for (int attempts = 1; attempts <= 10; attempts++, want *= 2) {
        CHECK_INT(cap_backoff_ms(attempts), want);
    }
    CHECK_INT(cap_backoff_ms(7), 1920000);
    CHECK_INT(cap_backoff_ms(10), 15360000);
    CHECK_INT(cap_backoff_ms(11), max);
    CHECK_INT(cap_backoff_ms(12), max);
    CHECK_INT(cap_backoff_ms(64), max);
    CHECK_INT(cap_backoff_ms(2147483647), max);
}

// ---- the JSON reader -----------------------------------------------------------------------------

static void json_strings(void)
{
    check_decoded("\"\"", "");
    check_decoded("\"a\\\"b\\\\c\\/d\\b\\f\\n\\r\\t\"", "a\"b\\c/d\b\f\n\r\t");
    check_decoded("\"\\u0041\\u00e9\\u05E9\\u20ac\"", "A\xc3\xa9\xd7\xa9\xe2\x82\xac");
    check_decoded("\"\xd7\xa9\xd7\x9c\xd7\x95\xd7\x9d\"", "\xd7\xa9\xd7\x9c\xd7\x95\xd7\x9d"); // UTF-8 as it is
    check_decoded("\"\\ud83d\\ude00\"", "\xf0\x9f\x98\x80");                                   // a surrogate pair
    check_decoded("\"x\\uD83D\\uDE00y\"", "x\xf0\x9f\x98\x80y");
    check_decoded("\"\\ud800\\udc00\\udbff\\udfff\"", "\xf0\x90\x80\x80\xf4\x8f\xbf\xbf");
    check_decoded("\"\\ud83dx\"", "\xef\xbf\xbdx"); // half a pair
    check_decoded("\"\\ud83d\"", "\xef\xbf\xbd");
    check_decoded("\"\\ude00\"", "\xef\xbf\xbd");
    check_decoded("\"\\ud83d\\u0041\"", "\xef\xbf\xbd" "A");
    check_decoded("\"\\ud83d\\ud83d\\ude00\"", "\xef\xbf\xbd\xf0\x9f\x98\x80");
    check_decoded("\"\\ud83d\\n\"", "\xef\xbf\xbd\n");
    check_decoded("\"a\\u0000b\"", "a\xef\xbf\xbd" "b"); // a C string cannot hold U+0000
    cap_json_t root;
    CHECK(cap_json_parse("12", &root));
    CHECK(cap_json_string(root) == NULL);
    CHECK(cap_json_parse("null", &root));
    CHECK(cap_json_string(root) == NULL);
}

static void json_is_strict(void)
{
    const char *good[] = {"{}", "[]", " \t\r\n{ } \n", "0", "-0", "-12.5e+3", "1E2", "true", "false", "null", "\"x\"",
                          "[1,\"a\",null,true,{\"k\":[]}]", "{\"a\":{\"b\":{\"c\":1}}}", "\"\\u00e9\""};
    for (size_t i = 0; i < sizeof good / sizeof *good; i++) {
        CHECK(parses(good[i]));
    }
    const char *bad[] = {"", " ", "{} x", "{}{}", "[1] 2", "1 2", "null,", // trailing junk
                         "[1,]", "{\"a\":1,}", "[,1]", "{,}", "[1 2]", "{\"a\" 1}", "{\"a\":}", "{a:1}", "{'a':1}",
                         "{\"a\":1", "[1", "\"x", "{\"a\"", "{1:2}", "01", "-", "+1", "1.", ".5", "1e", "1e+", "0x10",
                         "tru", "truex", "nul", "True", "NaN", "\"a\nb\"", "\"a\tb\"", "\"\\x\"", "\"\\u12\"",
                         "\"\\u12g4\"", "\"\\\"", "// c\n1", "[1] // c"};
    for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
        if (parses(bad[i])) {
            fprintf(stderr, "parsed: %s\n", bad[i]);
        }
        CHECK(!parses(bad[i]));
    }
    CHECK(!parses(NULL));
    cap_json_t root;
    CHECK(cap_json_parse(" \n [1, 2] \n", &root)); // the span leaves the space out
    CHECK_INT(root.len, 6);
    CHECK_INT(cap_json_type(root), '[');
    const char *types[] = {"{}", "[]", "\"s\"", "true", "false", "null", "-1", "7"};
    const char want[] = {'{', '[', '"', 't', 'f', 'n', '0', '0'};
    for (size_t i = 0; i < sizeof types / sizeof *types; i++) {
        CHECK(cap_json_parse(types[i], &root));
        CHECK_INT(cap_json_type(root), want[i]);
    }
    CHECK_INT(cap_json_type((cap_json_t){0}), 0);
}

static void json_depth_limit(void)
{
    char text[200];
    for (int depth = 1; depth <= 40; depth++) {
        memset(text, '[', (size_t)depth);
        memset(text + depth, ']', (size_t)depth);
        text[2 * depth] = 0;
        CHECK(parses(text) == (depth <= 32));
    }
    // objects count the same way
    cap_buf_t b = {0};
    for (int i = 0; i < 32; i++) {
        cap_buf_str(&b, "{\"a\":");
    }
    size_t open = b.len;
    cap_buf_str(&b, "{}");
    for (int i = 0; i < 32; i++) {
        cap_buf_str(&b, "}");
    }
    char *deep = cap_buf_take(&b);
    CHECK(!parses(deep)); // 33 deep
    memmove(deep + open - 5, deep + open, strlen(deep + open) + 1);
    deep[strlen(deep) - 1] = 0;
    CHECK(parses(deep)); // 32 deep
    free(deep);
    // a great many brackets are refused, not a stack overflow
    size_t many = 200000;
    char *flood = malloc(many + 1);
    memset(flood, '[', many);
    flood[many] = 0;
    CHECK(!parses(flood));
    free(flood);
}

static void json_members(void)
{
    cap_json_t root, value, key;
    CHECK(cap_json_parse("{\"a\":1,\"b\":\"x\",\"a\":2,\"c\":{\"a\":9},\"a\" : 3 , \"d\\u0065\":\"esc\",\"\":\"empty\"}", &root));
    long long n = 0;
    CHECK(cap_json_member(root, "a", &value)); // of two members with one name the last counts
    CHECK(cap_json_integer(value, &n));
    CHECK_INT(n, 3);
    char *text = cap_json_member_string(root, "b");
    CHECK_STR(text, "x");
    free(text);
    text = cap_json_member_string(root, "de"); // a key written with an escape
    CHECK_STR(text, "esc");
    free(text);
    text = cap_json_member_string(root, "");
    CHECK_STR(text, "empty");
    free(text);
    CHECK(cap_json_member_string(root, "a") == NULL); // not a string
    CHECK(cap_json_member_string(root, "missing") == NULL);
    CHECK(!cap_json_member(root, "missing", &value));
    CHECK(!cap_json_member(root, "A", &value));
    CHECK(!cap_json_member(root, "d", &value)); // no prefix matches
    CHECK(!cap_json_member(root, "dee", &value));
    CHECK(cap_json_member(root, "c", &value));
    CHECK_INT(cap_json_type(value), '{');
    CHECK(cap_json_member(value, "a", &value));
    CHECK(cap_json_integer(value, &n));
    CHECK_INT(n, 9);
    CHECK(!cap_json_member(value, "a", &value)); // a number has no members

    // every member, in the file's order
    const char *keys[] = {"\"a\"", "\"b\"", "\"a\"", "\"c\"", "\"a\"", "\"d\\u0065\"", "\"\""};
    const char *values[] = {"1", "\"x\"", "2", "{\"a\":9}", "3", "\"esc\"", "\"empty\""};
    size_t count = 0;
    for (const char *cursor = NULL; cap_json_each_member(root, &cursor, &key, &value); count++) {
        CHECK(count < 7);
        if (count < 7) {
            CHECK(key.len == strlen(keys[count]) && memcmp(key.at, keys[count], key.len) == 0);
            CHECK(value.len == strlen(values[count]) && memcmp(value.at, values[count], value.len) == 0);
        }
    }
    CHECK_INT(count, 7);
    const char *cursor = NULL;
    CHECK(cap_json_parse(" { } ", &root));
    CHECK(!cap_json_each_member(root, &cursor, &key, &value));
    cursor = NULL;
    CHECK(cap_json_parse("[1]", &root));
    CHECK(!cap_json_each_member(root, &cursor, &key, &value)); // not an object
}

static void json_arrays(void)
{
    cap_json_t root, item;
    CHECK(cap_json_parse(" [ 1 , \"two\" ,[3,[4]], {\"five\":[5,5]} ,null,true ] ", &root));
    const char *items[] = {"1", "\"two\"", "[3,[4]]", "{\"five\":[5,5]}", "null", "true"};
    size_t count = 0;
    for (const char *cursor = NULL; cap_json_each(root, &cursor, &item); count++) {
        CHECK(count < 6);
        if (count < 6) {
            CHECK(item.len == strlen(items[count]) && memcmp(item.at, items[count], item.len) == 0);
        }
    }
    CHECK_INT(count, 6);
    const char *cursor = NULL;
    CHECK(cap_json_parse("[ ]", &root));
    CHECK(!cap_json_each(root, &cursor, &item));
    cursor = NULL;
    CHECK(cap_json_parse("{\"a\":[1]}", &root));
    CHECK(!cap_json_each(root, &cursor, &item)); // not an array
    cursor = NULL;
    CHECK(cap_json_parse("[[],[[]]]", &root));
    CHECK(cap_json_each(root, &cursor, &item));
    CHECK_INT(item.len, 2);
    CHECK(cap_json_each(root, &cursor, &item));
    CHECK_INT(item.len, 4);
    CHECK(!cap_json_each(root, &cursor, &item));
    CHECK(!cap_json_each(root, &cursor, &item)); // and stays at the end
}

static void json_integers(void)
{
    const char *good[] = {"0", "-0", "12", "-7", "9223372036854775807", "-9223372036854775808"};
    const long long want[] = {0, 0, 12, -7, 9223372036854775807LL, -9223372036854775807LL - 1};
    cap_json_t root;
    long long n;
    for (size_t i = 0; i < sizeof good / sizeof *good; i++) {
        n = 99;
        CHECK(cap_json_parse(good[i], &root));
        CHECK(cap_json_integer(root, &n));
        CHECK_INT(n, want[i]);
    }
    const char *bad[] = {"1.0", "1.5", "1e3", "-2E0", "9223372036854775808", "-9223372036854775809",
                         "99999999999999999999999", "\"5\"", "true", "null", "[1]", "{}"};
    for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
        n = 99;
        CHECK(cap_json_parse(bad[i], &root));
        CHECK(!cap_json_integer(root, &n));
        CHECK_INT(n, 99);
    }
}

// String.toLongOrNull
static void parse_long(void)
{
    long long n = 99;
    CHECK(cap_parse_long("42", 2, &n) && n == 42);
    CHECK(cap_parse_long("42abc", 2, &n) && n == 42); // only `len` characters are read
    CHECK(cap_parse_long("-42", 3, &n) && n == -42);
    CHECK(cap_parse_long("+42", 3, &n) && n == 42);
    CHECK(cap_parse_long("007", 3, &n) && n == 7);
    CHECK(cap_parse_long("0", 1, &n) && n == 0);
    CHECK(cap_parse_long("9223372036854775807", 19, &n) && n == 9223372036854775807LL);
    CHECK(cap_parse_long("-9223372036854775808", 20, &n) && n == -9223372036854775807LL - 1);
    n = 99;
    const char *bad[] = {"", "-", "+", "4 2", " 42", "42 ", "4.2", "0x1f", "--1", "+-1", "1e3", "abc",
                         "9223372036854775808", "-9223372036854775809", "18446744073709551616", "99999999999999999999"};
    for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
        CHECK(!cap_parse_long(bad[i], strlen(bad[i]), &n));
    }
    CHECK_INT(n, 99);
}

static void utf8_whole(void)
{
    const char *cut[] = {"ab\xc3", "ab\xe2\x82", "ab\xe2", "ab\xf0\x9f\x98", "ab\xf0\x9f", "ab\xf0"};
    for (size_t i = 0; i < sizeof cut / sizeof *cut; i++) {
        char text[16];
        snprintf(text, sizeof text, "%s", cut[i]);
        cap_utf8_whole(text);
        CHECK_STR(text, "ab");
    }
    const char *whole[] = {"", "ab", "ab\xc3\xa9", "ab\xe2\x82\xac", "ab\xf0\x9f\x98\x80", "\xd7\xa9", "\xc3\xa9z"};
    for (size_t i = 0; i < sizeof whole / sizeof *whole; i++) {
        char text[16];
        snprintf(text, sizeof text, "%s", whole[i]);
        cap_utf8_whole(text);
        CHECK_STR(text, whole[i]);
    }
    char only[] = "\xf0\x9f\x98";
    cap_utf8_whole(only);
    CHECK_STR(only, "");
}

// ---- drift from the phone app (only where ~/Projects/capture is) -----------------------------------

static void shipped_prompts(void)
{
    const char *names[] = {"system_prompt.txt", "system_prompt_append.txt"};
    for (size_t i = 0; i < 2; i++) {
        char *prompt = shipped(names[i], NULL);
        CHECK(prompt != NULL);
        CHECK(prompt && strlen(prompt) > 100);
        CHECK(prompt && !strstr(prompt, "{{")); // shippedPromptHasNoPlaceholders
        free(prompt);
    }
    // the schema and the three fields cap_gemini_interpret needs have to stay in step
    char *schema = shipped("response_schema.json", NULL);
    cap_json_t root, required, properties, item, value;
    CHECK(cap_json_parse(schema, &root));
    CHECK(cap_json_member(root, "required", &required));
    CHECK(cap_json_member(root, "properties", &properties));
    size_t count = 0;
    for (const char *cursor = NULL; cap_json_each(required, &cursor, &item); count++) {
        char *name = cap_json_string(item);
        CHECK(name && (!strcmp(name, "transcript") || !strcmp(name, "title") || !strcmp(name, "summary")));
        CHECK(name && cap_json_member(properties, name, &value));
        free(name);
    }
    CHECK_INT(count, 3);
    CHECK(cap_json_member(root, "additionalProperties", &value));
    CHECK_INT(cap_json_type(value), 'f');
    free(schema);
}

static void same_as_the_phone_app(void)
{
    const char *home = getenv("HOME");
    char path[1024];
    snprintf(path, sizeof path, "%s/Projects/capture/app/src/main/java/com/kivan/capture/gemini/GeminiConfig.kt",
             home ? home : "");
    char *config = slurp(path, NULL);
    if (!config) {
        fprintf(stderr, "same_as_the_phone_app: no ~/Projects/capture here, skipped\n");
        return;
    }
    char lines[8][200];
    snprintf(lines[0], sizeof lines[0], "const val ENDPOINT = \"%s\"\n", CAP_GEMINI_ENDPOINT);
    snprintf(lines[1], sizeof lines[1], "const val MODEL = \"%s\"\n", CAP_GEMINI_MODEL);
    snprintf(lines[2], sizeof lines[2], "const val THINKING_LEVEL = \"%s\"\n", CAP_GEMINI_THINKING_LEVEL);
    snprintf(lines[3], sizeof lines[3], "const val TEMPERATURE = %s\n", CAP_GEMINI_TEMPERATURE);
    snprintf(lines[4], sizeof lines[4], "const val USER_TEXT = \"%s\"\n", CAP_GEMINI_USER_TEXT);
    snprintf(lines[5], sizeof lines[5], "const val USER_TEXT_APPEND = \"%s\"\n", CAP_GEMINI_USER_TEXT_APPEND);
    snprintf(lines[6], sizeof lines[6], "const val CALL_TIMEOUT_MINUTES = %dL\n", CAP_GEMINI_CALL_TIMEOUT_MS / 60000);
    snprintf(lines[7], sizeof lines[7], "const val MAX_ATTEMPTS = %d\n", CAP_GEMINI_MAX_ATTEMPTS);
    for (size_t i = 0; i < 8; i++) {
        if (!strstr(config, lines[i])) {
            fprintf(stderr, "GeminiConfig.kt has no line: %s", lines[i]);
        }
        CHECK(strstr(config, lines[i]) != NULL);
    }
    char line[200];
    snprintf(line, sizeof line, "const val REQUESTS_PER_MINUTE = %d\n", CAP_GEMINI_REQUESTS_PER_MINUTE);
    CHECK(strstr(config, line) != NULL);
    CHECK_INT(CAP_GEMINI_CALL_TIMEOUT_MS % 60000, 0);
    free(config);

    // ProcessWorker's gate and backoff
    snprintf(path, sizeof path, "%s/Projects/capture/app/src/main/java/com/kivan/capture/processing/ProcessWorker.kt", home);
    char *worker = slurp(path, NULL);
    CHECK(worker != NULL);
    CHECK(worker && strstr(worker, "RateGate(minIntervalMs = 60_000L / GeminiConfig.REQUESTS_PER_MINUTE + 1_000)"));
    CHECK(worker && strstr(worker, "setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)"));
    free(worker);

    const char *names[] = {"response_schema.json", "system_prompt.txt", "system_prompt_append.txt"};
    for (size_t i = 0; i < sizeof names / sizeof *names; i++) {
        size_t ours_len = 0, theirs_len = 0;
        char *ours = shipped(names[i], &ours_len);
        snprintf(path, sizeof path, "%s/Projects/capture/app/src/main/res/raw/%s", home, names[i]);
        char *theirs = slurp(path, &theirs_len);
        CHECK(ours != NULL);
        CHECK(theirs != NULL);
        if (ours && theirs && (ours_len != theirs_len || memcmp(ours, theirs, ours_len) != 0)) {
            fprintf(stderr, "%s differs from Capture's res/raw\n", names[i]);
            CHECK(false);
        }
        free(ours);
        free(theirs);
    }
}

int main(void)
{
    RUN(request_body_exact);
    RUN(append_request_body_exact);
    RUN(prompt_loses_trailing_whitespace_only);
    RUN(schema_is_reserialised);
    RUN(bad_schema_is_refused);
    RUN(content_length);
    RUN(whole_body_parses);
    RUN(base64_known_vectors);
    RUN(base64_in_blocks_is_the_same);
    RUN(base64_decode_as_github_sends_it);
    RUN(reads_completed_interaction);
    RUN(joins_text_parts_of_model_output);
    RUN(token_counts);
    RUN(non_completed_status_is_reported);
    RUN(unreadable_answers_retry);
    RUN(http_errors);
    RUN(network_error);
    RUN(error_message_is_cut);
    RUN(daily_quota_is_told_from_a_minute_s);
    RUN(retry_delay);
    RUN(daily_quota_waits_for_the_reset);
    RUN(newer_rate_limit_wording_names_the_window);
    RUN(quota_reset_is_midnight_pacific_across_daylight_saving);
    RUN(gate_one_at_a_time_and_spaced);
    RUN(gate_hold_delays_the_next_request);
    RUN(gate_long_hold_is_left_to_the_caller);
    RUN(rate_limit_holds_the_gate_and_is_not_an_attempt);
    RUN(bad_status_is_terminal_only_when_repeated);
    RUN(eight_attempts_give_up_and_terminal_at_once);
    RUN(backoff);
    RUN(json_strings);
    RUN(json_is_strict);
    RUN(json_depth_limit);
    RUN(json_members);
    RUN(json_arrays);
    RUN(json_integers);
    RUN(parse_long);
    RUN(utf8_whole);
    RUN(shipped_prompts);
    RUN(same_as_the_phone_app);
    return unit_done(__FILE__);
}
