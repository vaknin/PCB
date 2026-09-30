// Deep sleep on the board: about 8 µA for the module with the button (ext1, low) and USB (ext0,
// high) armed. The wake is a restart of the app; RTC memory and the RTC clock carry over.
// Not provable before hardware: neither simulator has deep sleep.
#include "hw.h"

#include "board_pins.h"
#include "esp_rtc_time.h"
#include "esp_sleep.h"

int64_t hw_uptime_ms(void)
{
    return (int64_t)(esp_rtc_get_time_us() / 1000);
}

hw_wake_t hw_wake(void)
{
    uint32_t causes = esp_sleep_get_wakeup_causes();
    if (causes & BIT(ESP_SLEEP_WAKEUP_EXT1)) {
        return HW_WAKE_BUTTON;
    }
    if (causes & BIT(ESP_SLEEP_WAKEUP_EXT0)) {
        return HW_WAKE_USB;
    }
    if (causes & BIT(ESP_SLEEP_WAKEUP_TIMER)) {
        return HW_WAKE_TIMER;
    }
    return HW_WAKE_RESET;
}

hw_wake_t hw_sleep(int64_t wake_after_ms)
{
    esp_sleep_disable_wakeup_source(ESP_SLEEP_WAKEUP_ALL);
    esp_sleep_enable_ext1_wakeup_io(1ULL << PIN_BUTTON, ESP_EXT1_WAKEUP_ANY_LOW); // R8 pulls it up
    esp_sleep_enable_ext0_wakeup(PIN_VBUS_SENSE, 1);                               // its divider pulls it down
    if (wake_after_ms > 0) {
        esp_sleep_enable_timer_wakeup((uint64_t)wake_after_ms * 1000);
    }
    esp_deep_sleep_start();
    return HW_WAKE_RESET; // not reached
}

void hw_sim_skip(int64_t ms)
{
    (void)ms;
}
