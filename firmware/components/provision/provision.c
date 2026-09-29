// The console side: reads command lines from USB Serial/JTAG (a real board) or UART0 (QEMU).
#include "provision.h"

#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include "sdkconfig.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "provision_text.h"
#if CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG
#include "driver/usb_serial_jtag.h"
#include "driver/usb_serial_jtag_vfs.h"
#else
#include "driver/uart.h"
#include "driver/uart_vfs.h"
#endif

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
    esp_err_t err = uart_driver_install(CONFIG_ESP_CONSOLE_UART_NUM, 2 * PROV_MAX_LINE, 0, 0, NULL, 0);
    if (err == ESP_OK) {
        uart_vfs_dev_use_driver(CONFIG_ESP_CONSOLE_UART_NUM);
    }
#endif
    return err;
}

static void print_line(const char *reply, void *ctx)
{
    (void)ctx;
    printf("%s\n", reply);
}

void provision_serve(uint32_t ms)
{
    static char line[PROV_MAX_LINE + 2];
    size_t n = 0;
    bool overflow = false;
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
            if (!overflow) {
                provision_handle(line, print_line, NULL);
            } else if (strncmp(line, "PROV", 4) == 0) {
                printf("PROV ERR line\n"); // too long: never store a cut-off value
            }
            memset(line, 0, sizeof line);
            n = 0;
            overflow = false;
            last = esp_timer_get_time();
        } else if (n + 1 < sizeof line) {
            line[n++] = (char)c;
        } else {
            overflow = true;
        }
    }
}
