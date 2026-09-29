// SOURCES: firmware/components/selftest/selftest.c
// The real selftest.c against stub board.h / esp_timer.h (firmware/test/stubs): the printed
// lines are the contract the sim stage and devctl parse.
#include "selftest.h"
#include "unit.h"

int stub_marked_good;
int64_t stub_time_us;

static selftest_result_t ok(char *d, size_t n)
{
    snprintf(d, n, "23.1 C, 41 %%RH");
    return SELFTEST_PASS;
}
static selftest_result_t skipped(char *d, size_t n)
{
    snprintf(d, n, "no GPIO in QEMU");
    return SELFTEST_SKIP;
}
static selftest_result_t broken(char *d, size_t n)
{
    snprintf(d, n, "no answer at 0x44");
    return SELFTEST_FAIL;
}
static selftest_result_t messy(char *d, size_t n)
{
    snprintf(d, n, "say \"hi\"\\\n\tend");
    return SELFTEST_PASS;
}
static selftest_result_t long_detail(char *d, size_t n)
{
    memset(d, 'x', n - 1);
    d[n - 1] = 0;
    return SELFTEST_PASS;
}

static const selftest_case_t cases[] = {
    {"ok", ok}, {"skipped", skipped}, {"broken", broken}, {"messy", messy}, {"long", long_detail},
};
#define N_CASES (sizeof cases / sizeof *cases)

static char *run(const char *const *wanted, size_t n, int *bad)
{
    stub_marked_good = 0;
    stub_time_us = 2140500;
    unit_capture_start();
    *bad = selftest_run(wanted, n, cases, N_CASES);
    return unit_capture_end();
}

static void passing_run_marks_good(void)
{
    const char *const wanted[] = {"ok", "skipped"};
    int bad;
    char *out = run(wanted, 2, &bad);
    CHECK_INT(bad, 0);
    CHECK_INT(stub_marked_good, 1);
    CHECK_STR(out, "SELFTEST {\"test\":\"ok\",\"result\":\"pass\",\"detail\":\"23.1 C, 41 %RH\"}\n"
                   "SELFTEST {\"test\":\"skipped\",\"result\":\"skip\",\"detail\":\"no GPIO in QEMU\"}\n"
                   "SELFTEST_DONE {\"pass\":1,\"fail\":0,\"skip\":1,\"missing\":0,\"ms\":2140}\n");
    free(out);
}

static void failure_is_not_marked_good(void)
{
    const char *const wanted[] = {"broken", "ok"};
    int bad;
    char *out = run(wanted, 2, &bad);
    CHECK_INT(bad, 1);
    CHECK_INT(stub_marked_good, 0);
    CHECK(strstr(out, "SELFTEST {\"test\":\"broken\",\"result\":\"fail\",\"detail\":\"no answer at 0x44\"}\n"));
    CHECK(strstr(out, "\"pass\":1,\"fail\":1,\"skip\":0,\"missing\":0"));
    free(out);
}

static void listed_but_not_written_is_missing(void)
{
    const char *const wanted[] = {"ok", "sht40"};
    int bad;
    char *out = run(wanted, 2, &bad);
    CHECK_INT(bad, 1);
    CHECK_INT(stub_marked_good, 0);
    CHECK(strstr(out, "SELFTEST {\"test\":\"sht40\",\"result\":\"missing\",\"detail\":\"not in the firmware\"}\n"));
    CHECK(strstr(out, "\"missing\":1"));
    free(out);
}

// quotes, backslashes and control characters would break the JSON line
static void detail_is_json_safe(void)
{
    const char *const wanted[] = {"messy"};
    int bad;
    char *out = run(wanted, 1, &bad);
    CHECK(strstr(out, "\"detail\":\"say 'hi''''end\"}\n"));
    free(out);
}

static void long_detail_is_cut(void)
{
    const char *const wanted[] = {"long"};
    int bad;
    char *out = run(wanted, 1, &bad);
    CHECK_INT(bad, 0);
    CHECK(strstr(out, "\"detail\":\"xxxx"));
    CHECK(strchr(out, '\n') - out < 160); // one line, the detail within its 96-byte buffer
    free(out);
}

int main(void)
{
    RUN(passing_run_marks_good);
    RUN(failure_is_not_marked_good);
    RUN(listed_but_not_written_is_missing);
    RUN(detail_is_json_safe);
    RUN(long_detail_is_cut);
    return unit_done(__FILE__);
}
