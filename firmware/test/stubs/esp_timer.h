// Laptop stand-in for ESP-IDF's esp_timer (scripts/fw-test.sh): the test sets the time.
#pragma once

#include <stdint.h>

extern int64_t stub_time_us;
static inline int64_t esp_timer_get_time(void)
{
    return stub_time_us;
}
