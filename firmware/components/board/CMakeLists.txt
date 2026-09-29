idf_component_register(SRCS "board.c"
                       INCLUDE_DIRS "include"
                       PRIV_REQUIRES app_update esp_app_format nvs_flash esp_psram)
