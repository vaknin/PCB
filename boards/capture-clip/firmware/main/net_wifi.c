// Wi-Fi with the provisioned network, then the clock by SNTP. The radio is on only between
// net_up() and net_down(); the charger is at its slow rate for all of that (the caller's rule,
// and again here). Not provable before hardware: QEMU has no Wi-Fi.
#include <stdio.h>
#include <string.h>
#include <sys/time.h>
#include "app.h"
#include "esp_event.h"
#include "esp_netif.h"
#include "esp_netif_sntp.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
#include "hw.h"
#include "provision.h"

#define GOT_IP BIT0
#define FAILED BIT1
#define CONNECT_MS 15000
#define SNTP_MS 10000
#define CLOCK_SET_S 1767225600 // 2026-01-01: before this the clock was never set

static EventGroupHandle_t events;
static bool ready, up, wanted;
static void (*on_lost)(void);

static void on_event(void *arg, esp_event_base_t base, int32_t id, void *data)
{
    (void)arg;
    (void)data;
    if (base == WIFI_EVENT && id == WIFI_EVENT_STA_START) {
        esp_wifi_connect();
    } else if (base == WIFI_EVENT && id == WIFI_EVENT_STA_DISCONNECTED) {
        xEventGroupSetBits(events, FAILED);
        if (up && wanted && on_lost) {
            up = false;
            on_lost();
        }
    } else if (base == IP_EVENT && id == IP_EVENT_STA_GOT_IP) {
        xEventGroupSetBits(events, GOT_IP);
    }
}

static bool prepare(void)
{
    if (ready) {
        return true;
    }
    events = xEventGroupCreate();
    if (esp_netif_init() != ESP_OK || esp_event_loop_create_default() != ESP_OK || !esp_netif_create_default_wifi_sta()) {
        return false;
    }
    wifi_init_config_t init = WIFI_INIT_CONFIG_DEFAULT();
    if (esp_wifi_init(&init) != ESP_OK) {
        return false;
    }
    esp_wifi_set_storage(WIFI_STORAGE_RAM); // the credentials stay in the provisioning store only
    esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID, on_event, NULL);
    esp_event_handler_register(IP_EVENT, IP_EVENT_STA_GOT_IP, on_event, NULL);
    esp_sntp_config_t sntp = ESP_NETIF_SNTP_DEFAULT_CONFIG("pool.ntp.org");
    sntp.start = false;
    esp_netif_sntp_init(&sntp);
    ready = true;
    return true;
}

bool net_up(char *why, size_t len)
{
    hw_charge_fast(false); // never the radio and the fast charge together (board.toml, [power])
    wifi_config_t config = {0};
    if (provision_get("wifi_ssid", (char *)config.sta.ssid, sizeof config.sta.ssid) != ESP_OK || !config.sta.ssid[0]) {
        snprintf(why, len, "wifi_ssid is not provisioned");
        return false;
    }
    provision_get("wifi_pass", (char *)config.sta.password, sizeof config.sta.password); // none: an open network
    if (!prepare()) {
        snprintf(why, len, "Wi-Fi did not start");
        return false;
    }
    wanted = true;
    xEventGroupClearBits(events, GOT_IP | FAILED);
    esp_wifi_set_mode(WIFI_MODE_STA);
    esp_err_t err = esp_wifi_set_config(WIFI_IF_STA, &config);
    memset(&config, 0, sizeof config);
    if (err != ESP_OK || esp_wifi_start() != ESP_OK) {
        snprintf(why, len, "Wi-Fi did not start");
        net_down();
        return false;
    }
    EventBits_t bits = xEventGroupWaitBits(events, GOT_IP | FAILED, pdFALSE, pdFALSE, pdMS_TO_TICKS(CONNECT_MS));
    if (!(bits & GOT_IP)) {
        snprintf(why, len, bits & FAILED ? "the network is not in range, or the password is wrong" : "no address in %d s",
                 CONNECT_MS / 1000);
        net_down();
        return false;
    }
    struct timeval now;
    gettimeofday(&now, NULL);
    esp_netif_sntp_start();
    // a clock that was set before keeps good time through deep sleep: then SNTP only corrects it
    if (esp_netif_sntp_sync_wait(pdMS_TO_TICKS(SNTP_MS)) != ESP_OK && now.tv_sec < CLOCK_SET_S) {
        snprintf(why, len, "no time from the network");
        net_down();
        return false;
    }
    up = true;
    return true;
}

void net_down(void)
{
    wanted = false;
    up = false;
    http_close();
    if (ready) {
        esp_wifi_stop();
    }
}

bool net_is_up(void)
{
    return up;
}

void net_on_lost(void (*lost)(void))
{
    on_lost = lost;
}
