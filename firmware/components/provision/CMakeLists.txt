idf_component_register(SRCS "provision.c"
                       INCLUDE_DIRS "include"
                       PRIV_REQUIRES nvs_flash esp_timer esp_driver_uart esp_driver_usb_serial_jtag vfs)
