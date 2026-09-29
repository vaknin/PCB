#include "board.h"

#include <stdio.h>
#include "esp_app_desc.h"
#include "esp_ota_ops.h"
#include "esp_psram.h"
#include "nvs_flash.h"

void board_start(const char *name, const char *revision)
{
    esp_err_t err = nvs_flash_init();
    if (err == ESP_ERR_NVS_NO_FREE_PAGES || err == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        // not a secret store yet: provisioning writes happen after this
        nvs_flash_erase();
        err = nvs_flash_init();
    }
    const esp_app_desc_t *app = esp_app_get_description();
    const esp_partition_t *running = esp_ota_get_running_partition();
    printf("BOARD {\"name\":\"%s\",\"rev\":\"%s\",\"target\":\"%s\",\"fw\":\"%s\",\"slot\":\"%s\","
           "\"psram\":%u,\"nvs\":%s}\n",
           name, revision, BOARD_TARGET, app->version, running ? running->label : "?",
           (unsigned)esp_psram_get_size(), err == ESP_OK ? "true" : "false");
}

void board_mark_good(void)
{
    esp_ota_img_states_t state;
    const esp_partition_t *running = esp_ota_get_running_partition();
    if (esp_ota_get_state_partition(running, &state) == ESP_OK && state == ESP_OTA_IMG_PENDING_VERIFY) {
        esp_ota_mark_app_valid_cancel_rollback();
        printf("BOARD_OTA {\"marked_good\":\"%s\"}\n", running->label);
    }
}
