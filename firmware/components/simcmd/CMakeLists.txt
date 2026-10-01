# SIM console commands (simcmd.h). Only the simulator builds (BOARD_TARGET QEMU or WOKWI) get
# the code; a REAL build gets the header alone, so a stray call fails to link instead of
# shipping. simcmd.c is pure C (scripts/fw-test.sh tests it on the laptop); simcmd_esp.c gives it
# the chip's clock. (CONFIG_ values are unset in IDF's early requirements pass, which only
# collects REQUIRES, so the sources are listed there; the real pass sees the board's sdkconfig.)
set(srcs "")
if(NOT CONFIG_BOARD_TARGET_REAL)
    set(srcs "simcmd.c" "simcmd_esp.c")
endif()
idf_component_register(SRCS ${srcs}
                       INCLUDE_DIRS "include"
                       PRIV_REQUIRES esp_timer)
