# Layer 1 (D-025): shared firmware code that needs NVS or FreeRTOS, built for ESP-IDF's
# linux target and run as a laptop program by scripts/fw-test.sh, with the sanitizers on.
cmake_minimum_required(VERSION 3.22)

get_filename_component(PCB_FIRMWARE "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
set(EXTRA_COMPONENT_DIRS "${PCB_FIRMWARE}/components")
set(COMPONENTS main) # main and what it requires: not the chip-only components

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
idf_build_set_property(COMPILE_OPTIONS "-fsanitize=address,undefined;-fno-sanitize-recover=all;-fno-omit-frame-pointer" APPEND)
idf_build_set_property(LINK_OPTIONS "-fsanitize=address,undefined" APPEND)
project(fw_linux_test)
