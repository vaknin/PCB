# provision_text.c is pure C and provision_cmd.c needs only NVS, so both also build for
# ESP-IDF's linux target (scripts/fw-test.sh); the console part needs the chip's drivers.
idf_build_get_property(target IDF_TARGET)
set(srcs "provision_text.c" "provision_cmd.c")
set(priv_requires nvs_flash)
if(NOT target STREQUAL "linux")
    list(APPEND srcs "provision.c")
    list(APPEND priv_requires esp_timer esp_driver_uart esp_driver_usb_serial_jtag vfs)
endif()
idf_component_register(SRCS ${srcs}
                       INCLUDE_DIRS "include"
                       PRIV_INCLUDE_DIRS "."
                       PRIV_REQUIRES ${priv_requires})
