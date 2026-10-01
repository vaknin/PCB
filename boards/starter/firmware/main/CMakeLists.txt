idf_component_register(SRCS "main.c"
                       PRIV_REQUIRES board selftest provision simcmd esp_psram esp_timer esp_driver_gpio esp_driver_i2c sensirion)
