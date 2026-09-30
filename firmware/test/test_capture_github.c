// SOURCES: firmware/components/capture/cap_text.c firmware/components/capture/cap_time.c firmware/components/capture/cap_note.c firmware/components/capture/cap_github.c
// The notes repo client against Capture's GitHubTest.kt, and its loops against a scripted server.
#include "cap_internal.h"
#include "capture.h"
#include "unit.h"

#define API "https://api.github.com/repos/owner/notes/contents/"
#define ID "0123456789abcdef0123456789abcdef"
#define NOTE_PUT API "notes/" ID ".md"
#define NOTE_GET NOTE_PUT "?ref=main"
#define COUNTER_PUT API "next-number"
#define COUNTER_GET COUNTER_PUT "?ref=main"
#define CHANGED "The notes repo changed during sync; will retry"
#define NOT_FOUND "Notes repo or branch not found, or the token cannot see it (HTTP 404)"
#define PUT_ANSWER(sha) "{\"content\":{\"name\":\"x.md\",\"sha\":\"" sha "\"},\"commit\":{\"sha\":\"c\"}}"

// ---- the scripted server ---------------------------------------------------------------------

typedef struct {
    const char *method, *url;
    int status;
    char *body; // malloc'd; handed to the code under test
    bool rate_limited, network_fails;
} step_t;

#define STEPS 16
static step_t script[STEPS];
static char *sent[STEPS]; // each request's body; NULL for a GET
static size_t script_len, script_at;

static step_t *expect_take(const char *method, const char *url, int status, char *body)
{
    step_t *step = &script[script_len++];
    *step = (step_t){.method = method, .url = url, .status = status, .body = body};
    return step;
}

static step_t *expect(const char *method, const char *url, int status, const char *body)
{
    return expect_take(method, url, status, strdup(body));
}

static bool mock_send(void *ctx, const char *method, const char *url, const char *body, cap_http_response_t *response)
{
    (void)ctx;
    if (script_at >= script_len) {
        fprintf(stderr, "a request beyond the script: %s %s\n", method, url);
        unit_test_failed = 1;
        return false;
    }
    step_t *step = &script[script_at];
    CHECK_STR(method, step->method);
    CHECK_STR(url, step->url);
    CHECK((body == NULL) == (strcmp(method, "GET") == 0));
    sent[script_at++] = body ? strdup(body) : NULL;
    if (step->network_fails) {
        return false;
    }
    response->status = step->status;
    response->body = step->body;
    response->rate_limited = step->rate_limited;
    step->body = NULL;
    return true;
}

// Every scripted request was made; clears the script for the next one.
static void script_done(void)
{
    CHECK_INT(script_at, script_len);
    for (size_t i = 0; i < STEPS; i++) {
        free(script[i].body);
        free(sent[i]);
        script[i] = (step_t){0};
        sent[i] = NULL;
    }
    script_len = script_at = 0;
}

static const cap_github_t github = {.repo = "owner/notes", .branch = "main", .send = mock_send};

// A contents GET's answer, shaped like GitHub's: the base64 wrapped in lines of 60.
static char *file_body(const char *text, const char *sha)
{
    cap_buf_t base64 = {0}, out = {0};
    cap_buf_base64(&base64, (const uint8_t *)text, strlen(text));
    cap_buf_str(&out, "{\"name\":\"x\",\"sha\":\"");
    cap_buf_str(&out, sha);
    cap_buf_str(&out, "\",\"size\":2,\"type\":\"file\",\"content\":\"");
    for (size_t at = 0; at < base64.len; at += 60) {
        cap_buf_add(&out, base64.data + at, base64.len - at < 60 ? base64.len - at : 60);
        cap_buf_str(&out, "\\n");
    }
    cap_buf_str(&out, "\",\"encoding\":\"base64\"}");
    free(cap_buf_take(&base64));
    return cap_buf_take(&out);
}

// Request `at` was a PUT of `text` with this commit message; sha NULL means it creates the file.
static void check_put(size_t at, const char *message, const char *sha, const char *text)
{
    cap_json_t root, value;
    CHECK(at < script_at && sent[at]);
    if (at >= script_at || !sent[at]) {
        return;
    }
    CHECK_STR(script[at].method, "PUT");
    CHECK(cap_json_parse(sent[at], &root));
    if (!cap_json_parse(sent[at], &root)) {
        return;
    }
    char *got_message = cap_json_member_string(root, "message");
    char *branch = cap_json_member_string(root, "branch");
    char *content = cap_json_member_string(root, "content");
    char *decoded = content ? cap_base64_decode(content, NULL) : NULL;
    CHECK_STR(got_message, message);
    CHECK_STR(branch, "main");
    CHECK_STR(decoded, text);
    if (sha) {
        char *got_sha = cap_json_member_string(root, "sha");
        CHECK_STR(got_sha, sha);
        free(got_sha);
    } else {
        CHECK(!cap_json_member(root, "sha", &value));
    }
    free(got_message);
    free(branch);
    free(content);
    free(decoded);
}

// ---- notes -----------------------------------------------------------------------------------

static cap_note_t make_note(const char *title, const char *summary)
{
    return (cap_note_t){.has_created = true,
                        .created_s = 1789803733,
                        .num = 7,
                        .duration_ms = 4200,
                        .title = strdup(title),
                        .summary = strdup(summary),
                        .transcript = strdup("A made-up transcript."),
                        .source = strdup("clip")};
}

static void add(cap_note_t *note, const char *id, int64_t created_s, const char *source, const char *text)
{
    note->additions = realloc(note->additions, (note->addition_count + 1) * sizeof *note->additions);
    cap_addition_t *addition = &note->additions[note->addition_count++];
    *addition = (cap_addition_t){.created_s = created_s, .source = strdup(source), .duration_ms = 1500, .text = strdup(text)};
    strcpy(addition->id, id);
}

#define OURS "aaaaaaaaaaaaaaaa0000000000000002"
#define THEIRS_EARLY "cccccccccccccccc0000000000000001"
#define THEIRS_SAME "bbbbbbbbbbbbbbbb0000000000000003" // made in the same second as ours; sorts after it

static cap_note_t our_note(void)
{
    cap_note_t note = make_note("Our title", "Our summary.");
    add(&note, OURS, 1789804000, "clip", "Added on the clip.");
    return note;
}

static cap_note_t their_note(void)
{
    cap_note_t note = make_note("Their title", "Their summary.");
    add(&note, THEIRS_EARLY, 1789803900, "phone", "Added on the phone.");
    add(&note, THEIRS_SAME, 1789804000, "laptop", "Added on the laptop.");
    return note;
}

// What the merge of the two above must put: GitHub's file, our title and summary, the union.
static cap_note_t merged_note(void)
{
    cap_note_t note = make_note("Our title", "Our summary.");
    add(&note, THEIRS_EARLY, 1789803900, "phone", "Added on the phone.");
    add(&note, OURS, 1789804000, "clip", "Added on the clip.");
    add(&note, THEIRS_SAME, 1789804000, "laptop", "Added on the laptop.");
    return note;
}

static void expect_note_get(cap_note_t *note, const char *sha)
{
    char *text = cap_note_render(ID, note);
    expect_take("GET", NOTE_GET, 200, file_body(text, sha));
    free(text);
    cap_note_free(note);
}

static void check_note_put(size_t at, const char *message, const char *sha, cap_note_t *expected)
{
    char *text = cap_note_render(ID, expected);
    CHECK(text != NULL);
    if (text) {
        check_put(at, message, sha, text);
    }
    free(text);
}

static void check_same_note(const cap_note_t *got, cap_note_t *expected)
{
    char *a = got->has_created ? cap_note_render(ID, got) : NULL, *b = cap_note_render(ID, expected);
    CHECK_STR(a, b);
    free(a);
    free(b);
}

// ---- GitHubTest.kt -----------------------------------------------------------------------------

static cap_status_t status_of(int code, const char *body, bool rate_limited)
{
    return cap_github_classify(code, body, rate_limited).status;
}

static void status_classification(void)
{
    CHECK_INT(status_of(200, "{}", false), CAP_OK);
    CHECK_INT(status_of(201, "{}", false), CAP_OK);
    CHECK_INT(status_of(401, "{}", false), CAP_TERMINAL);
    CHECK_INT(status_of(403, "{\"message\":\"Resource not accessible by personal access token\"}", false), CAP_TERMINAL);
    CHECK_INT(status_of(403, "{\"message\":\"You have exceeded a secondary rate limit.\"}", false), CAP_RETRYABLE);
    CHECK_INT(status_of(403, "{\"message\":\"API Rate Limit exceeded\"}", false), CAP_RETRYABLE);
    CHECK_INT(status_of(403, "{}", true), CAP_RETRYABLE);
    CHECK_INT(status_of(429, "{}", false), CAP_RETRYABLE);
    CHECK_INT(status_of(404, "{}", false), CAP_TERMINAL);
    CHECK_INT(status_of(409, "{}", false), CAP_RETRYABLE);
    CHECK_INT(status_of(422, "{\"message\":\"Invalid request.\\n\\n\\\"sha\\\" wasn't supplied.\"}", false), CAP_RETRYABLE);
    CHECK_INT(status_of(408, "{}", false), CAP_RETRYABLE);
    CHECK_INT(status_of(502, "{}", false), CAP_RETRYABLE);
    CHECK_INT(status_of(400, "{}", false), CAP_TERMINAL);
    CHECK_STR(cap_github_classify(400, "{\"message\":\"Problems parsing JSON\"}", false).message,
              "GitHub HTTP 400: Problems parsing JSON");
    CHECK_STR(cap_github_classify(418, "not json", false).message, "GitHub HTTP 418");
    CHECK_STR(cap_github_classify(418, "[]", false).message, "GitHub HTTP 418");
    CHECK_STR(cap_github_classify(401, "{}", false).message, "GitHub token rejected (expired or revoked?)");
    CHECK_STR(cap_github_classify(403, "{}", true).message, "GitHub rate limit; will retry");
    CHECK_STR(cap_github_classify(403, "{}", false).message, "GitHub token has no access to the notes repo (HTTP 403)");
    CHECK_STR(cap_github_classify(404, "", false).message, NOT_FOUND);
    CHECK_STR(cap_github_classify(409, "", false).message, CHANGED);
    CHECK_STR(cap_github_classify(502, "<html>", false).message, "GitHub HTTP 502; will retry");
}

static void bodies(void)
{
    char *put = cap_github_put_body("phone: add T", "# T\n", "main", NULL);
    CHECK_STR(put, "{\"message\":\"phone: add T\",\"content\":\"IyBUCg==\",\"branch\":\"main\"}");
    free(put);
    char *counter = cap_github_put_body("phone: next number 8", "8\n", "main", "abc");
    CHECK_STR(counter, "{\"message\":\"phone: next number 8\",\"content\":\"OAo=\",\"branch\":\"main\",\"sha\":\"abc\"}");
    free(counter);
    // a title with quotes and a line break stays one JSON string
    char *quoted = cap_github_put_body("clip: add \"T\"\n", "", "main", NULL);
    cap_json_t root;
    CHECK(quoted && cap_json_parse(quoted, &root));
    if (quoted && cap_json_parse(quoted, &root)) {
        char *message = cap_json_member_string(root, "message"), *content = cap_json_member_string(root, "content");
        CHECK_STR(message, "clip: add \"T\"\n");
        CHECK_STR(content, "");
        free(message);
        free(content);
    }
    free(quoted);
}

static void put_answer(void)
{
    char sha[CAP_SHA_LEN] = "";
    CHECK(cap_github_parse_put_sha(PUT_ANSWER("043909f46cb3a71bbfd2e1cd0cb43789d192ddaf"), sha));
    CHECK_STR(sha, "043909f46cb3a71bbfd2e1cd0cb43789d192ddaf");
    CHECK(!cap_github_parse_put_sha("{}", sha));
    CHECK(!cap_github_parse_put_sha("not json", sha));
    CHECK(!cap_github_parse_put_sha("{\"content\":null}", sha));
    CHECK(!cap_github_parse_put_sha("{\"content\":{\"sha\":7}}", sha));
}

#define COUNTER_BODY(content)                                                                            \
    "{\"name\":\"next-number\",\"path\":\"next-number\",\"sha\":\"c0ffee\",\"size\":2,\"type\":\"file\","  \
    "\"content\":\"" content "\",\"encoding\":\"base64\"}"

static void counter_answer(void)
{
    int next = 0;
    char sha[CAP_SHA_LEN] = "";
    CHECK_INT(cap_github_parse_counter(COUNTER_BODY("Nwo=\\n"), &next, sha).status, CAP_OK);
    CHECK_INT(next, 7);
    CHECK_STR(sha, "c0ffee");
    // GitHub wraps the base64 in lines
    CHECK_INT(cap_github_parse_counter(COUNTER_BODY("MTIz\\nCg==\\n"), &next, sha).status, CAP_OK);
    CHECK_INT(next, 123);
    CHECK_INT(cap_github_parse_counter(COUNTER_BODY("IDQyIAo="), &next, sha).status, CAP_OK);
    CHECK_INT(next, 42);
    // Int.MAX_VALUE - 1 can still move on
    CHECK_INT(cap_github_parse_counter(COUNTER_BODY("MjE0NzQ4MzY0Ngo="), &next, sha).status, CAP_OK);
    CHECK_INT(next, 2147483646);
}

static void bad_counter_stops_sync(void)
{
    // "abc", "0", "-3", Int.MAX_VALUE (it could not move on), "" and "1.5": a hand edit
    const char *edited[] = {COUNTER_BODY("YWJjCg=="), COUNTER_BODY("MAo="), COUNTER_BODY("LTMK"),
                            COUNTER_BODY("MjE0NzQ4MzY0Nwo="), COUNTER_BODY(""), COUNTER_BODY("MS41Cg==")};
    int next = -1;
    char sha[CAP_SHA_LEN] = "";
    for (size_t i = 0; i < sizeof edited / sizeof *edited; i++) {
        cap_result_t out = cap_github_parse_counter(edited[i], &next, sha);
        CHECK_INT(out.status, CAP_TERMINAL);
        CHECK(strncmp(out.message, "next-number in the notes repo is not a number", 45) == 0);
    }
    CHECK_STR(cap_github_parse_counter(edited[0], &next, sha).message,
              "next-number in the notes repo is not a number: \"abc\"");
    // only the first 20 characters are shown: "abcdefghijklmnopqrstuvwxyz"
    CHECK_STR(cap_github_parse_counter(COUNTER_BODY("YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo="), &next, sha).message,
              "next-number in the notes repo is not a number: \"abcdefghijklmnopqrst\"");
    CHECK_INT(next, -1);
    const char *unreadable[] = {"{}", "not json", "[]", "{\"sha\":\"c0ffee\"}", "{\"content\":\"Nwo=\"}"};
    for (size_t i = 0; i < sizeof unreadable / sizeof *unreadable; i++) {
        cap_result_t out = cap_github_parse_counter(unreadable[i], &next, sha);
        CHECK_INT(out.status, CAP_RETRYABLE);
        CHECK_STR(out.message, "Unreadable answer from GitHub");
    }
}

static void urls(void)
{
    char *url = cap_github_contents_url("owner/notes", "notes/x.md", NULL);
    CHECK_STR(url, "https://api.github.com/repos/owner/notes/contents/notes/x.md");
    free(url);
    url = cap_github_contents_url("owner/notes", CAP_COUNTER_PATH, NULL);
    CHECK_STR(url, "https://api.github.com/repos/owner/notes/contents/next-number");
    free(url);
    url = cap_github_contents_url("o/r", CAP_COUNTER_PATH, "feature/x");
    CHECK_STR(url, "https://api.github.com/repos/o/r/contents/next-number?ref=feature%2Fx");
    free(url);
    // URLEncoder.encode: .-*_ stay, a space is +, UTF-8 goes byte by byte
    url = cap_github_contents_url("o/r", "notes/x.md", "a b.c-d*e_f~\xc3\xa9");
    CHECK_STR(url, "https://api.github.com/repos/o/r/contents/notes/x.md?ref=a+b.c-d*e_f%7E%C3%A9");
    free(url);
}

static void file_answer(void)
{
    const char *text = "A file long enough for its base64 to be wrapped over more than one line, as GitHub sends it.\n";
    char *body = file_body(text, "f00d"), *got = NULL;
    char sha[CAP_SHA_LEN] = "";
    CHECK(strstr(body, "\\n") != strrchr(body, '\\')); // wrapped inside, not only at the end
    CHECK_INT(cap_github_parse_file(body, &got, sha).status, CAP_OK);
    CHECK_STR(got, text);
    CHECK_STR(sha, "f00d");
    free(got);
    free(body);
    // an empty file
    CHECK_INT(cap_github_parse_file("{\"sha\":\"e69de29b\",\"content\":\"\"}", &got, sha).status, CAP_OK);
    CHECK_STR(got, "");
    CHECK_STR(sha, "e69de29b");
    free(got);
    // a directory listing, a missing or wrong-typed member, a sha too long to be one
    const char *bad[] = {"[]", "not json", "", "{\"sha\":\"abc\"}", "{\"content\":\"Nwo=\"}", "{\"sha\":\"abc\",\"content\":null}",
                         "{\"sha\":7,\"content\":\"Nwo=\"}", "{\"sha\":\"\",\"content\":\"Nwo=\"}",
                         "{\"sha\":\"0123456789012345678901234567890123456789012345678901234567890123456789\",\"content\":\"Nwo=\"}"};
    for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
        cap_result_t out = cap_github_parse_file(bad[i], &got, sha);
        CHECK_INT(out.status, CAP_RETRYABLE);
        CHECK_STR(out.message, "Unreadable answer from GitHub");
        CHECK(got == NULL);
        free(got);
    }
}

// ---- reserve_number ------------------------------------------------------------------------------

static void reserve_first_number(void)
{
    int number = 0;
    expect("GET", COUNTER_GET, 404, "{\"message\":\"Not Found\"}");
    expect("PUT", COUNTER_PUT, 201, PUT_ANSWER("c1"));
    CHECK_INT(cap_github_reserve_number(&github, &number).status, CAP_OK);
    CHECK_INT(number, 1);
    check_put(1, "clip: next number 2", NULL, "2\n");
    script_done();
}

static void reserve_number(void)
{
    int number = 0;
    expect("GET", COUNTER_GET, 200, COUNTER_BODY("Nwo=\\n"));
    expect("PUT", COUNTER_PUT, 200, PUT_ANSWER("c1"));
    CHECK_INT(cap_github_reserve_number(&github, &number).status, CAP_OK);
    CHECK_INT(number, 7);
    check_put(1, "clip: next number 8", "c0ffee", "8\n");
    script_done();
}

// Another device took the number first: read again and take the next.
static void reserve_after(int conflict)
{
    int number = 0;
    expect("GET", COUNTER_GET, 200, COUNTER_BODY("Nwo=\\n"));
    expect("PUT", COUNTER_PUT, conflict, "{\"message\":\"is at c0ffee2 but expected c0ffee\"}");
    expect_take("GET", COUNTER_GET, 200, file_body("8\n", "c0ffee2"));
    expect("PUT", COUNTER_PUT, 200, PUT_ANSWER("c1"));
    CHECK_INT(cap_github_reserve_number(&github, &number).status, CAP_OK);
    CHECK_INT(number, 8);
    check_put(1, "clip: next number 8", "c0ffee", "8\n");
    check_put(3, "clip: next number 9", "c0ffee2", "9\n");
    script_done();
}

static void reserve_after_conflict(void)
{
    reserve_after(409);
}

static void reserve_after_created_meanwhile(void)
{
    reserve_after(422);
    // no counter when read, there when written
    int number = 0;
    expect("GET", COUNTER_GET, 404, "{}");
    expect("PUT", COUNTER_PUT, 422, "{\"message\":\"Invalid request.\\n\\n\\\"sha\\\" wasn't supplied.\"}");
    expect_take("GET", COUNTER_GET, 200, file_body("2\n", "c2"));
    expect("PUT", COUNTER_PUT, 200, PUT_ANSWER("c3"));
    CHECK_INT(cap_github_reserve_number(&github, &number).status, CAP_OK);
    CHECK_INT(number, 2);
    check_put(1, "clip: next number 2", NULL, "2\n");
    check_put(3, "clip: next number 3", "c2", "3\n");
    script_done();
}

static void reserve_gives_up(void)
{
    int number = -1;
    for (int i = 0; i < CAP_RESERVE_TRIES; i++) {
        expect("GET", COUNTER_GET, 200, COUNTER_BODY("Nwo=\\n"));
        expect("PUT", COUNTER_PUT, i % 2 ? 422 : 409, "{}");
    }
    cap_result_t out = cap_github_reserve_number(&github, &number);
    CHECK_INT(out.status, CAP_RETRYABLE);
    CHECK_STR(out.message, "The note counter kept changing; will retry");
    CHECK_INT(number, -1);
    CHECK_INT(script_at, 2 * CAP_RESERVE_TRIES);
    script_done();
}

static void reserve_failures(void)
{
    int number = -1;
    expect("GET", COUNTER_GET, 401, "{\"message\":\"Bad credentials\"}");
    cap_result_t out = cap_github_reserve_number(&github, &number);
    CHECK_INT(out.status, CAP_TERMINAL);
    CHECK_STR(out.message, "GitHub token rejected (expired or revoked?)");
    script_done();

    expect("GET", COUNTER_GET, 0, "")->network_fails = true;
    out = cap_github_reserve_number(&github, &number);
    CHECK_INT(out.status, CAP_RETRYABLE);
    CHECK_STR(out.message, "Network error");
    script_done();

    // the answer to the write is lost: the number may be taken, the caller tries again
    expect("GET", COUNTER_GET, 200, COUNTER_BODY("Nwo=\\n"));
    expect("PUT", COUNTER_PUT, 0, "")->network_fails = true;
    out = cap_github_reserve_number(&github, &number);
    CHECK_INT(out.status, CAP_RETRYABLE);
    script_done();

    // a hand-edited counter is not written over
    expect("GET", COUNTER_GET, 200, COUNTER_BODY("YWJjCg=="));
    out = cap_github_reserve_number(&github, &number);
    CHECK_INT(out.status, CAP_TERMINAL);
    CHECK_STR(out.message, "next-number in the notes repo is not a number: \"abc\"");
    script_done();

    expect("GET", COUNTER_GET, 200, "<html>");
    out = cap_github_reserve_number(&github, &number);
    CHECK_INT(out.status, CAP_RETRYABLE);
    CHECK_STR(out.message, "Unreadable answer from GitHub");
    script_done();

    // a rate limit is in the headers; the body of a 403 does not say
    expect("GET", COUNTER_GET, 403, "{\"message\":\"Forbidden\"}")->rate_limited = true;
    out = cap_github_reserve_number(&github, &number);
    CHECK_INT(out.status, CAP_RETRYABLE);
    CHECK_STR(out.message, "GitHub rate limit; will retry");
    script_done();

    expect("GET", COUNTER_GET, 403, "{\"message\":\"Resource not accessible by personal access token\"}");
    CHECK_INT(cap_github_reserve_number(&github, &number).status, CAP_TERMINAL);
    script_done();

    expect("GET", COUNTER_GET, 502, "<html>");
    CHECK_INT(cap_github_reserve_number(&github, &number).status, CAP_RETRYABLE);
    script_done();

    // no counter and no repo both answer 404 to the read; the write tells them apart
    expect("GET", COUNTER_GET, 404, "{}");
    expect("PUT", COUNTER_PUT, 404, "{\"message\":\"Not Found\"}");
    out = cap_github_reserve_number(&github, &number);
    CHECK_INT(out.status, CAP_TERMINAL);
    CHECK_STR(out.message, NOT_FOUND);
    script_done();

    expect("GET", COUNTER_GET, 200, COUNTER_BODY("Nwo=\\n"));
    expect("PUT", COUNTER_PUT, 429, "{}")->rate_limited = true;
    CHECK_INT(cap_github_reserve_number(&github, &number).status, CAP_RETRYABLE);
    script_done();
    CHECK_INT(number, -1);
}

// ---- create_note ---------------------------------------------------------------------------------

static void create_note(void)
{
    cap_note_t note = make_note("A made-up title", "A made-up summary.");
    char sha[CAP_SHA_LEN] = "";
    expect("PUT", NOTE_PUT, 201, PUT_ANSWER("new1"));
    CHECK_INT(cap_github_create_note(&github, ID, &note, sha).status, CAP_OK);
    CHECK_STR(sha, "new1");
    check_note_put(0, "clip: add A made-up title", NULL, &note);
    script_done();
    cap_note_free(&note);
}

static void create_note_already_there(void)
{
    cap_note_t note = make_note("A made-up title", "A made-up summary."), there = make_note("A made-up title", "A made-up summary.");
    char sha[CAP_SHA_LEN] = "";
    expect("PUT", NOTE_PUT, 422, "{\"message\":\"Invalid request.\\n\\n\\\"sha\\\" wasn't supplied.\"}");
    expect_note_get(&there, "there1");
    CHECK_INT(cap_github_create_note(&github, ID, &note, sha).status, CAP_OK);
    CHECK_STR(sha, "there1");
    check_note_put(0, "clip: add A made-up title", NULL, &note);
    script_done();
    cap_note_free(&note);
}

static void create_note_failures(void)
{
    cap_note_t note = make_note("A made-up title", "A made-up summary.");
    char sha[CAP_SHA_LEN] = "";
    // refused, and not because the file is there
    expect("PUT", NOTE_PUT, 422, "{}");
    expect("GET", NOTE_GET, 404, "{\"message\":\"Not Found\"}");
    cap_result_t out = cap_github_create_note(&github, ID, &note, sha);
    CHECK_INT(out.status, CAP_RETRYABLE);
    CHECK_STR(out.message, CHANGED);
    script_done();

    expect("PUT", NOTE_PUT, 409, "{}");
    expect("GET", NOTE_GET, 0, "")->network_fails = true;
    out = cap_github_create_note(&github, ID, &note, sha);
    CHECK_INT(out.status, CAP_RETRYABLE);
    CHECK_STR(out.message, "Network error");
    script_done();

    expect("PUT", NOTE_PUT, 404, "{\"message\":\"Not Found\"}");
    out = cap_github_create_note(&github, ID, &note, sha);
    CHECK_INT(out.status, CAP_TERMINAL);
    CHECK_STR(out.message, NOT_FOUND);
    script_done();

    expect("PUT", NOTE_PUT, 401, "{}");
    CHECK_INT(cap_github_create_note(&github, ID, &note, sha).status, CAP_TERMINAL);
    script_done();

    expect("PUT", NOTE_PUT, 201, "{}");
    out = cap_github_create_note(&github, ID, &note, sha);
    CHECK_INT(out.status, CAP_RETRYABLE);
    CHECK_STR(out.message, "Unreadable answer from GitHub");
    script_done();
    CHECK_STR(sha, "");

    // a note with no time cannot be written; nothing is sent
    note.has_created = false;
    CHECK_INT(cap_github_create_note(&github, ID, &note, sha).status, CAP_TERMINAL);
    script_done();
    cap_note_free(&note);
}

// The commit message takes the title as the file has it.
static void create_note_titles(void)
{
    cap_note_t note = make_note("  Two\nlines ", "");
    char sha[CAP_SHA_LEN] = "";
    expect("PUT", NOTE_PUT, 201, PUT_ANSWER("new1"));
    CHECK_INT(cap_github_create_note(&github, ID, &note, sha).status, CAP_OK);
    char *text = cap_note_render(ID, &note);
    const char *title = text ? strstr(text, "\n# ") : NULL;
    CHECK(title != NULL);
    if (title) {
        char message[64];
        snprintf(message, sizeof message, "clip: add %.*s", (int)strcspn(title + 3, "\n"), title + 3);
        check_put(0, message, NULL, text);
    }
    free(text);
    script_done();
    cap_note_free(&note);

    note = make_note("", "");
    expect("PUT", NOTE_PUT, 201, PUT_ANSWER("new1"));
    CHECK_INT(cap_github_create_note(&github, ID, &note, sha).status, CAP_OK);
    check_note_put(0, "clip: add Untitled", NULL, &note);
    script_done();
    cap_note_free(&note);
}

// ---- get_note ------------------------------------------------------------------------------------

static void get_note(void)
{
    cap_note_t there = their_note(), expected = their_note(), note;
    char sha[CAP_SHA_LEN] = "";
    expect_note_get(&there, "there1");
    CHECK_INT(cap_github_get_note(&github, ID, &note, sha).status, CAP_OK);
    CHECK_STR(sha, "there1");
    CHECK_STR(note.title, "Their title");
    CHECK_INT(note.num, 7);
    CHECK_INT(note.addition_count, 2);
    check_same_note(&note, &expected);
    script_done();
    cap_note_free(&note);
    cap_note_free(&expected);
}

static void get_note_failures(void)
{
    cap_note_t note;
    char sha[CAP_SHA_LEN] = "";
    expect("GET", NOTE_GET, 404, "{\"message\":\"Not Found\"}");
    cap_result_t out = cap_github_get_note(&github, ID, &note, sha);
    CHECK_INT(out.status, CAP_GONE);
    CHECK_STR(out.message, "The note is not on GitHub");
    script_done();
    cap_note_free(&note);

    expect("GET", NOTE_GET, 401, "{}");
    CHECK_INT(cap_github_get_note(&github, ID, &note, sha).status, CAP_TERMINAL);
    script_done();
    cap_note_free(&note);

    expect("GET", NOTE_GET, 200, "[]");
    CHECK_INT(cap_github_get_note(&github, ID, &note, sha).status, CAP_RETRYABLE);
    script_done();
    cap_note_free(&note);

    expect("GET", NOTE_GET, 0, "")->network_fails = true;
    CHECK_INT(cap_github_get_note(&github, ID, &note, sha).status, CAP_RETRYABLE);
    script_done();
    cap_note_free(&note);
}

// ---- update_note ---------------------------------------------------------------------------------

static void update_note(void)
{
    cap_note_t note = our_note(), expected = our_note();
    char sha[CAP_SHA_LEN] = "old1";
    expect("PUT", NOTE_PUT, 200, PUT_ANSWER("new1"));
    CHECK_INT(cap_github_update_note(&github, ID, &note, sha).status, CAP_OK);
    CHECK_STR(sha, "new1");
    check_note_put(0, "clip: add to Our title", "old1", &expected);
    check_same_note(&note, &expected);
    script_done();
    cap_note_free(&note);
    cap_note_free(&expected);
}

static void update_note_merges(void)
{
    cap_note_t note = our_note(), ours = our_note(), there = their_note(), expected = merged_note();
    char sha[CAP_SHA_LEN] = "old1";
    expect("PUT", NOTE_PUT, 409, "{\"message\":\"notes/x.md does not match old1\"}");
    expect_note_get(&there, "there1");
    expect("PUT", NOTE_PUT, 200, PUT_ANSWER("new2"));
    CHECK_INT(cap_github_update_note(&github, ID, &note, sha).status, CAP_OK);
    CHECK_STR(sha, "new2");
    check_note_put(0, "clip: add to Our title", "old1", &ours);
    check_note_put(2, "clip: add to Our title", "there1", &expected);
    CHECK_INT(note.addition_count, 3);
    if (note.addition_count == 3) {
        CHECK_STR(note.additions[0].id, THEIRS_EARLY);
        CHECK_STR(note.additions[1].id, OURS);
        CHECK_STR(note.additions[2].id, THEIRS_SAME);
    }
    check_same_note(&note, &expected);
    script_done();
    cap_note_free(&note);
    cap_note_free(&ours);
    cap_note_free(&expected);
}

// An earlier try got through and its answer was lost, or another device merged ours already.
static void update_note_already_there(void)
{
    cap_note_t note = our_note(), there = merged_note(), expected = merged_note();
    free(there.title);
    free(expected.title);
    there.title = strdup("Their title");
    expected.title = strdup("Their title");
    char sha[CAP_SHA_LEN] = "old1";
    expect("PUT", NOTE_PUT, 409, "{}");
    expect_note_get(&there, "there1");
    CHECK_INT(cap_github_update_note(&github, ID, &note, sha).status, CAP_OK);
    CHECK_STR(sha, "there1");
    CHECK_STR(note.title, "Their title");
    check_same_note(&note, &expected);
    script_done();
    cap_note_free(&note);
    cap_note_free(&expected);
}

static void update_note_gone(void)
{
    cap_note_t note = our_note(), expected = our_note();
    char sha[CAP_SHA_LEN] = "old1";
    expect("PUT", NOTE_PUT, 409, "{}");
    expect("GET", NOTE_GET, 404, "{\"message\":\"Not Found\"}");
    CHECK_INT(cap_github_update_note(&github, ID, &note, sha).status, CAP_GONE);
    script_done();

    expect("PUT", NOTE_PUT, 404, "{\"message\":\"Not Found\"}");
    cap_result_t out = cap_github_update_note(&github, ID, &note, sha);
    CHECK_INT(out.status, CAP_GONE);
    CHECK_STR(out.message, "The note is not on GitHub");
    script_done();
    // the caller's note and sha are as they were
    CHECK_STR(sha, "old1");
    check_same_note(&note, &expected);
    cap_note_free(&note);
    cap_note_free(&expected);
}

static void update_note_gives_up(void)
{
    cap_note_t note = our_note();
    char sha[CAP_SHA_LEN] = "old1";
    const char *shas[] = {"there1", "there2", "there3"};
    for (int i = 0; i < CAP_UPDATE_TRIES; i++) {
        cap_note_t there = their_note();
        expect("PUT", NOTE_PUT, i % 2 ? 422 : 409, "{}");
        expect_note_get(&there, shas[i]);
    }
    cap_result_t out = cap_github_update_note(&github, ID, &note, sha);
    CHECK_INT(out.status, CAP_RETRYABLE);
    CHECK_STR(out.message, "The notes repo kept changing during sync; will retry");
    CHECK_INT(script_at, 2 * CAP_UPDATE_TRIES);
    // each try was put against the sha read before it
    cap_note_t ours = our_note(), merged = merged_note();
    check_note_put(0, "clip: add to Our title", "old1", &ours);
    check_note_put(2, "clip: add to Our title", "there1", &merged);
    check_note_put(4, "clip: add to Our title", "there2", &merged);
    // what is left is the merge, so the next run starts from it
    CHECK_STR(sha, "there3");
    check_same_note(&note, &merged);
    script_done();
    cap_note_free(&note);
    cap_note_free(&ours);
    cap_note_free(&merged);
}

static void update_note_failures(void)
{
    cap_note_t note = our_note(), expected = our_note();
    char sha[CAP_SHA_LEN] = "old1";
    expect("PUT", NOTE_PUT, 0, "")->network_fails = true;
    cap_result_t out = cap_github_update_note(&github, ID, &note, sha);
    CHECK_INT(out.status, CAP_RETRYABLE);
    CHECK_STR(out.message, "Network error");
    script_done();

    expect("PUT", NOTE_PUT, 409, "{}");
    expect("GET", NOTE_GET, 0, "")->network_fails = true;
    CHECK_INT(cap_github_update_note(&github, ID, &note, sha).status, CAP_RETRYABLE);
    script_done();

    expect("PUT", NOTE_PUT, 401, "{}");
    CHECK_INT(cap_github_update_note(&github, ID, &note, sha).status, CAP_TERMINAL);
    script_done();

    expect("PUT", NOTE_PUT, 403, "{}")->rate_limited = true;
    CHECK_INT(cap_github_update_note(&github, ID, &note, sha).status, CAP_RETRYABLE);
    script_done();

    expect("PUT", NOTE_PUT, 200, "not json");
    CHECK_INT(cap_github_update_note(&github, ID, &note, sha).status, CAP_RETRYABLE);
    script_done();
    CHECK_STR(sha, "old1");
    check_same_note(&note, &expected);
    cap_note_free(&note);
    cap_note_free(&expected);
}

int main(void)
{
    RUN(status_classification);
    RUN(bodies);
    RUN(put_answer);
    RUN(counter_answer);
    RUN(bad_counter_stops_sync);
    RUN(urls);
    RUN(file_answer);
    RUN(reserve_first_number);
    RUN(reserve_number);
    RUN(reserve_after_conflict);
    RUN(reserve_after_created_meanwhile);
    RUN(reserve_gives_up);
    RUN(reserve_failures);
    RUN(create_note);
    RUN(create_note_already_there);
    RUN(create_note_failures);
    RUN(create_note_titles);
    RUN(get_note);
    RUN(get_note_failures);
    RUN(update_note);
    RUN(update_note_merges);
    RUN(update_note_already_there);
    RUN(update_note_gone);
    RUN(update_note_gives_up);
    RUN(update_note_failures);
    return unit_done(__FILE__);
}
