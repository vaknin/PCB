# speech.wav is made by run.sh from a local Capture recording and is not committed (public repo).
idf_component_register(SRCS "main.c"
                       PRIV_REQUIRES esp_psram esp_partition esp_timer
                       EMBED_FILES "${CMAKE_CURRENT_LIST_DIR}/../out/speech.wav")
