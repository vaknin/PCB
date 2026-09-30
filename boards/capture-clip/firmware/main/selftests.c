// The six self-tests board.toml lists (spec.md, "What done means"). On the board they run after
// a reset while USB is in (devctl selftest resets the board and reads the report) and on the
// console command SELFTEST. In QEMU the pins, the mic and Wi-Fi are stand-ins (hw.h), and a
// test says so.
#include <stdio.h>
#include <stdlib.h>
#include "app.h"
#include "board.h"
#include "board_pins.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "hw.h"
#include "selftest.h"

static selftest_result_t mic(char *detail, size_t len)
{
    // 300 ms of sound: a working mic is never silent (its own noise is about 30 counts here),
    // and one that is stuck gives a constant
    enum { SAMPLES = 4800 };
    int16_t *pcm = malloc(SAMPLES * sizeof *pcm);
    if (!pcm || !hw_mic_start()) {
        free(pcm);
        snprintf(detail, len, "the microphone did not start");
        return SELFTEST_FAIL;
    }
    int got = hw_mic_read(pcm, SAMPLES);
    hw_mic_stop();
    int lo = 32767, hi = -32768;
    int64_t sum = 0;
    for (int i = 0; i < got; i++) {
        lo = pcm[i] < lo ? pcm[i] : lo;
        hi = pcm[i] > hi ? pcm[i] : hi;
        sum += pcm[i];
    }
    free(pcm);
    if (got != SAMPLES) {
        snprintf(detail, len, "only %d of %d samples", got, SAMPLES);
        return SELFTEST_FAIL;
    }
    snprintf(detail, len, "%s%d to %d, mean %d", BOARD_IS_REAL ? "" : "made-up sound (no I2S here): ", lo, hi,
             (int)(sum / SAMPLES));
    if (!BOARD_IS_REAL) {
        return hi > lo ? SELFTEST_SKIP : SELFTEST_FAIL;
    }
    // all zeros: no data line; a constant or full scale: a stuck line
    return hi - lo >= 4 && hi < 32767 && lo > -32768 ? SELFTEST_PASS : SELFTEST_FAIL;
}

static selftest_result_t led_rgb(char *detail, size_t len)
{
    if (BOARD_IS_QEMU) {
        snprintf(detail, len, "no GPIO in QEMU");
        return SELFTEST_SKIP;
    }
    static const struct {
        uint8_t colour;
        const char *name;
    } steps[] = {{CLIP_RED, "red"}, {CLIP_GREEN, "green"}, {CLIP_BLUE, "blue"}};
    for (int i = 0; i < 3; i++) {
        hw_led(steps[i].colour);
        printf("SELFTEST_LOOK led_rgb %s\n", steps[i].name);
        vTaskDelay(pdMS_TO_TICKS(700));
    }
    hw_led(0);
    snprintf(detail, len, "red, green, blue for 0.7 s each");
    return SELFTEST_PASS;
}

static selftest_result_t button(char *detail, size_t len)
{
    if (BOARD_IS_QEMU) {
        snprintf(detail, len, "no GPIO in QEMU");
        return SELFTEST_SKIP;
    }
    printf("SELFTEST_PRESS button\n");
    int down = -1;
    for (int i = 0; i < 1000; i++) {
        bool pressed = hw_button();
        if (pressed && down < 0) {
            down = i;
        } else if (!pressed && down >= 0) {
            snprintf(detail, len, "pressed for %d ms", (i - down) * 10);
            return SELFTEST_PASS;
        }
        vTaskDelay(pdMS_TO_TICKS(10));
    }
    snprintf(detail, len, down < 0 ? "no press in 10 s" : "held, never released");
    return SELFTEST_FAIL;
}

static selftest_result_t battery(char *detail, size_t len)
{
    int mv = hw_battery_mv();
    snprintf(detail, len, "%d mV, %d %%%s", mv, clip_battery_percent(mv), BOARD_IS_QEMU ? " (SIM BATTERY, no ADC in QEMU)" : "");
    // a single cell, or the charger's own 4.2 V with no cell fitted
    return mv >= 3000 && mv <= 4400 ? SELFTEST_PASS : SELFTEST_FAIL;
}

static selftest_result_t usb_sense(char *detail, size_t len)
{
    bool usb = hw_usb();
    if (BOARD_IS_QEMU) {
        snprintf(detail, len, "SIM USB %d (no GPIO in QEMU)", usb);
        return SELFTEST_SKIP;
    }
    // the self-test is run over the USB console, so USB must read as present
    snprintf(detail, len, usb ? "USB present, %s" : "USB not seen (the test runs over USB)",
             hw_charging() ? "charging" : hw_charged() ? "charge complete" : "charger idle");
    return usb ? SELFTEST_PASS : SELFTEST_FAIL;
}

static selftest_result_t wifi(char *detail, size_t len)
{
    char why[80] = "";
    hw_charge_fast(false); // never the radio and the fast charge together
    bool ok = http_load_keys(why, sizeof why) && net_up(why, sizeof why);
    net_down();
    if (ok) {
        snprintf(detail, len, BOARD_IS_QEMU ? "QEMU's Ethernet stands in: connected, clock set" : "connected, clock set");
        return SELFTEST_PASS;
    }
    snprintf(detail, len, "%s", why);
    // a QEMU run without the mock server (the sim stage's) has no network to test
    return BOARD_IS_QEMU ? SELFTEST_SKIP : SELFTEST_FAIL;
}

static const selftest_case_t cases[] = {
    {"mic", mic}, {"led_rgb", led_rgb}, {"button", button}, {"battery", battery}, {"usb_sense", usb_sense}, {"wifi", wifi},
};

void selftests_run(void)
{
    static const char *const wanted[] = BOARD_SELF_TESTS;
    selftest_run(wanted, BOARD_SELF_TEST_COUNT, cases, sizeof cases / sizeof cases[0]);
}
