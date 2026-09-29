// Laptop stand-in for firmware/components/board (scripts/fw-test.sh): records the call.
#pragma once

#define BOARD_TARGET "test"
#define BOARD_IS_QEMU 0
#define BOARD_IS_WOKWI 0
#define BOARD_IS_REAL 0

extern int stub_marked_good;
static inline void board_mark_good(void)
{
    stub_marked_good++;
}
