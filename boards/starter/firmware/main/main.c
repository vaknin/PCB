// Starter board firmware (D-025): the four self-tests board.toml lists, then the
// provisioning console. Proves the shared firmware base; the board itself is a tooling test.
#include <stdio.h>
#include "board.h"
#include "board_pins.h"
#include "driver/gpio.h"
#include "driver/i2c_master.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "provision.h"
#include "sensirion.h"
#include "selftest.h"

#define SHT40_ADDR 0x44
#define I2C_TIMEOUT_MS 50

static i2c_master_bus_handle_t bus;

static esp_err_t i2c_up(void)
{
    if (bus) {
        return ESP_OK;
    }
    i2c_master_bus_config_t cfg = {
        .i2c_port = -1,
        .sda_io_num = PIN_I2C_SDA,
        .scl_io_num = PIN_I2C_SCL,
        .clk_source = I2C_CLK_SRC_DEFAULT,
        .glitch_ignore_cnt = 7,
        .flags.enable_internal_pullup = false, // R7/R8 are on the board
    };
    return i2c_new_master_bus(&cfg, &bus);
}

static selftest_result_t sht40(char *detail, size_t len)
{
    if (!BOARD_IS_REAL) { // QEMU has no I2C; Wokwi has no SHT40
        snprintf(detail, len, "no SHT40 in the %s simulation", BOARD_TARGET);
        return SELFTEST_SKIP;
    }
    if (i2c_up() != ESP_OK) {
        snprintf(detail, len, "I2C bus failed");
        return SELFTEST_FAIL;
    }
    i2c_device_config_t dev = {.dev_addr_length = I2C_ADDR_BIT_LEN_7, .device_address = SHT40_ADDR, .scl_speed_hz = 100000};
    i2c_master_dev_handle_t h;
    if (i2c_master_bus_add_device(bus, &dev, &h) != ESP_OK) {
        snprintf(detail, len, "add device failed");
        return SELFTEST_FAIL;
    }
    uint8_t cmd = 0xFD, r[6]; // measure, high precision
    esp_err_t err = i2c_master_transmit(h, &cmd, 1, I2C_TIMEOUT_MS);
    if (err == ESP_OK) {
        vTaskDelay(pdMS_TO_TICKS(10));
        err = i2c_master_receive(h, r, sizeof r, I2C_TIMEOUT_MS);
    }
    i2c_master_bus_rm_device(h);
    if (err != ESP_OK) {
        snprintf(detail, len, "no answer at 0x44 (%s)", esp_err_to_name(err));
        return SELFTEST_FAIL;
    }
    float t, rh;
    if (!sht4x_convert(r, &t, &rh)) {
        snprintf(detail, len, "CRC mismatch");
        return SELFTEST_FAIL;
    }
    snprintf(detail, len, "%.1f C, %.0f %%RH", t, rh);
    // plausible indoors; outside this the sensor or its reading is suspect
    return t > 0 && t < 50 && rh > 0 && rh < 100 ? SELFTEST_PASS : SELFTEST_FAIL;
}

static selftest_result_t i2c_scan(char *detail, size_t len)
{
    if (!BOARD_IS_REAL) { // QEMU has no I2C; Wokwi has no SHT40
        snprintf(detail, len, "no SHT40 in the %s simulation", BOARD_TARGET);
        return SELFTEST_SKIP;
    }
    if (i2c_up() != ESP_OK) {
        snprintf(detail, len, "I2C bus failed");
        return SELFTEST_FAIL;
    }
    int n = snprintf(detail, len, "found:");
    bool sht = false;
    for (uint16_t a = 0x08; a < 0x78; a++) {
        if (i2c_master_probe(bus, a, I2C_TIMEOUT_MS) == ESP_OK) {
            sht |= a == SHT40_ADDR;
            if (n > 0 && (size_t)n < len) {
                n += snprintf(detail + n, len - n, " 0x%02x", a);
            }
        }
    }
    return sht ? SELFTEST_PASS : SELFTEST_FAIL;
}

// Drives the LED on for 1 s. Wokwi checks the pin; on a board the owner confirms by eye.
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

// Waits up to 10 s for a press of BOOT and its release (a Wokwi scenario presses it). It
// reports only after the release, so a scenario's next wait can't miss the report.
static selftest_result_t boot_button(char *detail, size_t len)
{
    if (BOARD_IS_QEMU) {
        snprintf(detail, len, "no GPIO in QEMU");
        return SELFTEST_SKIP;
    }
    gpio_reset_pin(PIN_BOOT);
    gpio_set_direction(PIN_BOOT, GPIO_MODE_INPUT); // R5 pulls it up
    printf("SELFTEST_PRESS boot_button\n");
    int down = -1;
    for (int i = 0; i < 1000; i++) {
        bool pressed = gpio_get_level(PIN_BOOT) == 0; // active low
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

static const selftest_case_t cases[] = {
    {"sht40", sht40},
    {"i2c_scan", i2c_scan},
    {"status_led", status_led},
    {"boot_button", boot_button},
};

void app_main(void)
{
    board_start(BOARD_NAME, BOARD_REVISION);
    static const char *const wanted[] = BOARD_SELF_TESTS;
    selftest_run(wanted, BOARD_SELF_TEST_COUNT, cases, sizeof cases / sizeof cases[0]);
    provision_console_init();
    provision_serve(0);
}
