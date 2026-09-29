#include "selftest.h"

#include <stdio.h>
#include <string.h>
#include "board.h"
#include "esp_timer.h"

// Detail text goes inside a JSON string: keep it printable and unquoted.
static void clean(char *s)
{
    for (; *s; s++) {
        if (*s == '"' || *s == '\\' || (unsigned char)*s < 0x20) {
            *s = '\'';
        }
    }
}

int selftest_run(const char *const wanted[], size_t n_wanted, const selftest_case_t cases[], size_t n_cases)
{
    static const char *const words[] = {"pass", "fail", "skip"};
    int count[3] = {0}, missing = 0;
    for (size_t i = 0; i < n_wanted; i++) {
        const selftest_case_t *c = NULL;
        for (size_t j = 0; j < n_cases; j++) {
            if (strcmp(cases[j].name, wanted[i]) == 0) {
                c = &cases[j];
            }
        }
        if (!c) {
            printf("SELFTEST {\"test\":\"%s\",\"result\":\"missing\",\"detail\":\"not in the firmware\"}\n", wanted[i]);
            missing++;
            continue;
        }
        char detail[96] = "";
        selftest_result_t r = c->run(detail, sizeof detail);
        clean(detail);
        count[r]++;
        printf("SELFTEST {\"test\":\"%s\",\"result\":\"%s\",\"detail\":\"%s\"}\n", c->name, words[r], detail);
    }
    // ms: uptime, so a simulator run can tell how much simulated time it used
    printf("SELFTEST_DONE {\"pass\":%d,\"fail\":%d,\"skip\":%d,\"missing\":%d,\"ms\":%lld}\n",
           count[SELFTEST_PASS], count[SELFTEST_FAIL], count[SELFTEST_SKIP], missing, esp_timer_get_time() / 1000);
    int bad = count[SELFTEST_FAIL] + missing;
    if (bad == 0) {
        board_mark_good();
    }
    return bad;
}
