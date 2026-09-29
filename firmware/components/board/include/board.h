// Shared board basics (D-025): the boot banner devctl and the sim stage read, the build
// target, NVS start-up and marking a new firmware good. Pins come from board_pins.h,
// which the pcbgen fw stage writes from board.toml.
#pragma once

#include "sdkconfig.h"

// "real", "qemu" or "wokwi" (Kconfig BOARD_TARGET).
#define BOARD_TARGET CONFIG_BOARD_TARGET_NAME

// 0/1 flags for the build target (Kconfig leaves unset options undefined, not 0).
#ifdef CONFIG_BOARD_TARGET_QEMU
#define BOARD_IS_QEMU 1
#else
#define BOARD_IS_QEMU 0
#endif
#ifdef CONFIG_BOARD_TARGET_WOKWI
#define BOARD_IS_WOKWI 1
#else
#define BOARD_IS_WOKWI 0
#endif
#define BOARD_IS_REAL (!BOARD_IS_QEMU && !BOARD_IS_WOKWI)

// Starts NVS (erasing it only if its layout is from an incompatible IDF version) and prints
// the one-line banner:  BOARD {"name":...,"rev":...,"target":...,"fw":...}
void board_start(const char *name, const char *revision);

// Called after a passing self-test: a firmware that was just updated stays; without this
// call the bootloader rolls back to the previous one at the next reset.
void board_mark_good(void);
