idf_component_register(SRCS "selftest.c"
                       INCLUDE_DIRS "include"
                       REQUIRES board
                       PRIV_REQUIRES esp_timer)
