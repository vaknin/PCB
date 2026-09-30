# speech.wav is made by run.sh / run-real.sh from a local Capture recording and is not committed
# (public repo). It is only embedded when CONFIG_SPIKE_EMBED_SPEECH is on (sdkconfig.speech).
set(embed "")
if(CONFIG_SPIKE_EMBED_SPEECH)
    set(embed "${CMAKE_CURRENT_LIST_DIR}/../../out/speech.wav")
endif()
idf_component_register(SRCS "real.c"
                       PRIV_REQUIRES esp_psram esp_partition esp_timer esp_pm esp_wifi esp_event
                                     esp_netif nvs_flash esp_driver_usb_serial_jtag esp_driver_gpio
                                     spi_flash esp_hw_support
                       EMBED_FILES ${embed})
