# Which stand-ins a build gets (hw.h): QEMU has no pins, microphone, sleep or Wi-Fi; Wokwi has
# pins and Wi-Fi.
set(srcs "main.c" "pins_seq.c" "store.c" "recorder.c" "http.c" "update.c" "selftests.c")
# Requirements are read before the configuration exists, so every target names them all; what a
# target does not call is left out by the linker.
set(requires board selftest provision clip capture esp_psram esp_driver_gpio esp_timer esp_http_client
             app_update esp_app_format mbedtls esp_netif esp_event nvs_flash esp_hw_support
             esp_eth esp_adc esp_wifi esp_driver_i2s)
if(CONFIG_BOARD_TARGET_QEMU)
    list(APPEND srcs "hw_stub.c" "mic_synth.c" "sleep_fake.c" "net_eth.c")
elseif(CONFIG_BOARD_TARGET_WOKWI)
    list(APPEND srcs "hw_gpio.c" "mic_synth.c" "sleep_fake.c" "net_wifi.c")
else()
    list(APPEND srcs "hw_gpio.c" "mic_i2s.c" "sleep_deep.c" "net_wifi.c")
endif()

set(prompts "${PCB_ROOT}/firmware/components/capture/prompts")
idf_component_register(SRCS ${srcs}
                       PRIV_REQUIRES ${requires}
                       EMBED_TXTFILES "${prompts}/system_prompt.txt" "${prompts}/system_prompt_append.txt"
                                      "${prompts}/response_schema.json")
