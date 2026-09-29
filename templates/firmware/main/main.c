// Firmware for this board (template: templates/firmware, D-025). Implement one function per
// self-test that board.toml lists; a listed test with no implementation fails the run.
#include <stdio.h>
#include "board.h"
#include "board_pins.h"
#include "provision.h"
#include "selftest.h"

static selftest_result_t example(char *detail, size_t len)
{
    snprintf(detail, len, "replace me");
    return SELFTEST_SKIP;
}

static const selftest_case_t cases[] = {
    {"example", example},
};

void app_main(void)
{
    board_start(BOARD_NAME, BOARD_REVISION);
    static const char *const wanted[] = BOARD_SELF_TESTS;
    selftest_run(wanted, BOARD_SELF_TEST_COUNT, cases, sizeof cases / sizeof cases[0]);
    provision_console_init();
    provision_serve(0);
}
