#include "provision.h"

#include <ctype.h>
#include <stdio.h>
#include <string.h>
#include "sdkconfig.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "nvs.h"
#if CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG
#include "driver/usb_serial_jtag.h"
#include "driver/usb_serial_jtag_vfs.h"
#else
#include "driver/uart.h"
#include "driver/uart_vfs.h"
#endif

#define NS "prov"
#define MAX_VALUE 512
#define MAX_LINE (32 + 2 * MAX_VALUE)

esp_err_t provision_console_init(void)
{
    setvbuf(stdin, NULL, _IONBF, 0);
#if CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG
    usb_serial_jtag_driver_config_t cfg = USB_SERIAL_JTAG_DRIVER_CONFIG_DEFAULT();
    esp_err_t err = usb_serial_jtag_driver_install(&cfg);
    if (err == ESP_OK) {
        usb_serial_jtag_vfs_use_driver();
    }
#else
    esp_err_t err = uart_driver_install(CONFIG_ESP_CONSOLE_UART_NUM, 2 * MAX_LINE, 0, 0, NULL, 0);
    if (err == ESP_OK) {
        uart_vfs_dev_use_driver(CONFIG_ESP_CONSOLE_UART_NUM);
    }
#endif
    return err;
}

static bool key_ok(const char *k)
{
    size_t n = strlen(k);
    if (n == 0 || n > 15) {
        return false;
    }
    for (; *k; k++) {
        if (!(islower((unsigned char)*k) || isdigit((unsigned char)*k) || *k == '_')) {
            return false;
        }
    }
    return true;
}

static int unhex(const char *s, uint8_t *out, size_t cap)
{
    size_t n = strlen(s);
    if (n % 2 || n / 2 > cap) {
        return -1;
    }
    for (size_t i = 0; i < n / 2; i++) {
        unsigned v;
        if (sscanf(s + 2 * i, "%2x", &v) != 1 || !isxdigit((unsigned char)s[2 * i]) || !isxdigit((unsigned char)s[2 * i + 1])) {
            return -1;
        }
        out[i] = (uint8_t)v;
    }
    return (int)(n / 2);
}

static void list(nvs_handle_t h)
{
    nvs_iterator_t it = NULL;
    esp_err_t err = nvs_entry_find_in_handle(h, NVS_TYPE_BLOB, &it);
    while (err == ESP_OK) {
        nvs_entry_info_t info;
        nvs_entry_info(it, &info);
        size_t len = 0;
        nvs_get_blob(h, info.key, NULL, &len);
        printf("PROV KEY %s %u\n", info.key, (unsigned)len);
        err = nvs_entry_next(&it);
    }
    nvs_release_iterator(it);
    printf("PROV END\n");
}

static void handle(char *line)
{
    char *cmd = strtok(line, " \r\n");
    char *verb = strtok(NULL, " \r\n");
    char *key = strtok(NULL, " \r\n");
    char *arg = strtok(NULL, " \r\n");
    if (!cmd || strcmp(cmd, "PROV") != 0 || !verb) {
        return; // not for us: the console may carry other traffic
    }
    nvs_handle_t h;
    if (nvs_open(NS, NVS_READWRITE, &h) != ESP_OK) {
        printf("PROV ERR nvs\n");
        return;
    }
    if (strcmp(verb, "LIST") == 0) {
        list(h);
    } else if (!key || !key_ok(key)) {
        printf("PROV ERR key\n");
    } else if (strcmp(verb, "SET") == 0) {
        static uint8_t value[MAX_VALUE];
        int n = arg ? unhex(arg, value, sizeof value) : -1;
        if (n < 0) {
            printf("PROV ERR %s value\n", key);
        } else if (nvs_set_blob(h, key, value, n) == ESP_OK && nvs_commit(h) == ESP_OK) {
            printf("PROV OK %s %d\n", key, n);
        } else {
            printf("PROV ERR %s write\n", key);
        }
        memset(value, 0, sizeof value);
    } else if (strcmp(verb, "DEL") == 0) {
        esp_err_t err = nvs_erase_key(h, key);
        if ((err == ESP_OK || err == ESP_ERR_NVS_NOT_FOUND) && nvs_commit(h) == ESP_OK) {
            printf("PROV OK %s 0\n", key);
        } else {
            printf("PROV ERR %s erase\n", key);
        }
    } else {
        printf("PROV ERR verb\n");
    }
    nvs_close(h);
}

void provision_serve(uint32_t ms)
{
    static char line[MAX_LINE];
    size_t n = 0;
    int64_t last = esp_timer_get_time();
    printf("PROV READY\n");
    while (ms == 0 || esp_timer_get_time() - last < (int64_t)ms * 1000) {
        int c = getchar();
        if (c == EOF) {
            clearerr(stdin);
            vTaskDelay(pdMS_TO_TICKS(10));
            continue;
        }
        if (c == '\n') {
            line[n] = 0;
            handle(line);
            memset(line, 0, sizeof line);
            n = 0;
            last = esp_timer_get_time();
        } else if (n + 1 < sizeof line) {
            line[n++] = (char)c;
        }
    }
}

esp_err_t provision_get(const char *key, char *buf, size_t len)
{
    nvs_handle_t h;
    esp_err_t err = nvs_open(NS, NVS_READONLY, &h);
    if (err != ESP_OK) {
        return err == ESP_ERR_NVS_NOT_FOUND ? ESP_ERR_NVS_NOT_FOUND : err;
    }
    size_t n = len;
    err = nvs_get_blob(h, key, buf, &n);
    nvs_close(h);
    if (err == ESP_OK) {
        buf[n < len ? n : len - 1] = 0;
    }
    return err;
}
