# ESP-IDF project for one board (D-025; template: templates/firmware). The pcbgen fw stage
# writes board_pins.h next to this file from board.toml; the shared components live in the
# repo's firmware/components (PCB_ROOT overrides the repo path, e.g. for --out runs).
# Targets: `idf.py build` (real board), or the sim stage's QEMU and Wokwi builds, which add
# sdkconfig.qemu / sdkconfig.wokwi to the defaults in their own build directories.
cmake_minimum_required(VERSION 3.22)

if(DEFINED ENV{PCB_ROOT})
    set(PCB_ROOT "$ENV{PCB_ROOT}")
else()
    get_filename_component(PCB_ROOT "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)
endif()
set(EXTRA_COMPONENT_DIRS "${PCB_ROOT}/firmware/components")
if(NOT DEFINED SDKCONFIG_DEFAULTS)
    set(SDKCONFIG_DEFAULTS "sdkconfig.defaults")
endif()

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
idf_build_set_property(COMPILE_OPTIONS "-I${CMAKE_CURRENT_LIST_DIR}" APPEND)
project(firmware)
