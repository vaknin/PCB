// Firmware for this board (template: templates/firmware, D-025). Implement one function per
// self-test that board.toml lists; a listed test with no implementation fails the run.
// After the self-tests the provisioning console runs; it also takes SELFTEST (the tests again)
// and, in simulator builds, the SIM lines of firmware/components/simcmd that stand in for the
// pins in QEMU (board.toml [[sim.scenario]] `sim = "PRESS 300"`). Read a button through
// simcmd_button() when BOARD_IS_QEMU, as boards/starter/firmware/main/main.c does.
#include <stdio.h>
#include <string.h>
#include "board.h"
#include "board_pins.h"
#include "driver/gpio.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "provision.h"
#include "selftest.h"
#if !BOARD_IS_REAL
#include "simcmd.h"
#endif

// The status light: on for a second (Wokwi checks the pin on SELFTEST_LOOK; at bring-up the
// owner looks at it). QEMU has no GPIO, so it skips there.
static selftest_result_t status_led(char *detail, size_t len)
{
    if (BOARD_IS_QEMU) {
        snprintf(detail, len, "no GPIO in QEMU");
        return SELFTEST_SKIP;
    }
    gpio_reset_pin(PIN_STATUS_LED);
    gpio_set_direction(PIN_STATUS_LED, GPIO_MODE_OUTPUT);
    gpio_set_level(PIN_STATUS_LED, 1);
    printf("SELFTEST_LOOK status_led on\n");
    vTaskDelay(pdMS_TO_TICKS(1000));
    gpio_set_level(PIN_STATUS_LED, 0);
    snprintf(detail, len, "on for 1 s");
    return SELFTEST_PASS;
}

static const selftest_case_t cases[] = {
    {"status_led", status_led},
};

static void run_selftests(void)
{
    static const char *const wanted[] = BOARD_SELF_TESTS;
    selftest_run(wanted, BOARD_SELF_TEST_COUNT, cases, sizeof cases / sizeof cases[0]);
}

// A console SELFTEST runs in its own task, so the console keeps reading lines: a test that
// waits for a (simulated) button press needs the SIM PRESS that comes after it.
static volatile bool selftest_busy;

static void selftest_task(void *arg)
{
    (void)arg;
    run_selftests();
    selftest_busy = false;
    vTaskDelete(NULL);
}

// Console lines that aren't PROV:
//   SELFTEST   the self-tests again (SELFTEST_BUSY while one runs)
//   SIM ...    the simulator's pins (not in a REAL build; simcmd.h)
static void console_line(const char *line, void *ctx)
{
    (void)ctx;
#if !BOARD_IS_REAL
    if (simcmd_serve(line)) {
        return;
    }
#endif
    if (strcmp(line, "SELFTEST") == 0) {
        if (selftest_busy) {
            printf("SELFTEST_BUSY\n");
        } else {
            selftest_busy = true;
            xTaskCreate(selftest_task, "selftest", 6144, NULL, 5, NULL);
        }
    }
}

void app_main(void)
{
    board_start(BOARD_NAME, BOARD_REVISION);
    run_selftests();
    provision_console_init();
#if !BOARD_IS_REAL
    simcmd_start(NULL, NULL);
#endif
    provision_serve_with(0, console_line, NULL);
}
