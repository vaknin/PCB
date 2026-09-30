// The simulators have no deep sleep: the "sleep" is a wait for what would wake the board. The
// button and USB come from the pins (Wokwi) or SIM lines (QEMU). The timer wake does not wait
// in real time: `SIM TIMER` jumps the clocks to it, so a scenario does not sit out a backoff.
#include "hw.h"

#include <stdio.h>
#include <sys/time.h>
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

static int64_t skipped_ms;
static volatile bool timer_now;

int64_t hw_uptime_ms(void)
{
    return esp_timer_get_time() / 1000 + skipped_ms;
}

hw_wake_t hw_wake(void)
{
    return HW_WAKE_RESET;
}

void hw_sim_skip(int64_t ms)
{
    struct timeval now;
    gettimeofday(&now, NULL);
    now.tv_sec += ms / 1000;
    settimeofday(&now, NULL);
    skipped_ms += ms;
}

void sleep_fake_timer(void)
{
    timer_now = true;
}

hw_wake_t hw_sleep(int64_t wake_after_ms)
{
    int64_t since = hw_uptime_ms();
    printf("SLEEP {\"wake_after_ms\":%lld}\n", (long long)wake_after_ms);
    timer_now = false;
    for (;;) {
        if (hw_button()) {
            return HW_WAKE_BUTTON;
        }
        if (hw_usb()) {
            return HW_WAKE_USB;
        }
        if (wake_after_ms > 0 && timer_now) {
            int64_t left = wake_after_ms - (hw_uptime_ms() - since);
            hw_sim_skip(left > 0 ? left : 0);
            return HW_WAKE_TIMER;
        }
        if (wake_after_ms > 0 && hw_uptime_ms() - since >= wake_after_ms) {
            return HW_WAKE_TIMER;
        }
        vTaskDelay(pdMS_TO_TICKS(10));
    }
}
