// QEMU has no Wi-Fi: the network is its OpenCores Ethernet behind QEMU's own NAT, where the host
// is 10.0.2.2. The clock comes from the mock server, so a run needs nothing from the internet.
// `SIM NET 0` makes the network unreachable, as out of Wi-Fi range.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#include "app.h"
#include "esp_eth.h"
#include "esp_eth_mac_openeth.h"
#include "esp_event.h"
#include "esp_netif.h"
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
#include "hw.h"

#define GOT_IP BIT0

static EventGroupHandle_t events;
static esp_eth_handle_t eth;
static bool started, up;

static void on_ip(void *arg, esp_event_base_t base, int32_t id, void *data)
{
    (void)arg;
    (void)base;
    (void)id;
    (void)data;
    xEventGroupSetBits(events, GOT_IP);
}

static bool start(void)
{
    if (started) {
        return eth != NULL;
    }
    started = true;
    events = xEventGroupCreate();
    esp_netif_init();
    esp_event_loop_create_default();
    esp_netif_config_t netif_cfg = ESP_NETIF_DEFAULT_ETH();
    esp_netif_t *netif = esp_netif_new(&netif_cfg);
    eth_mac_config_t mac_cfg = ETH_MAC_DEFAULT_CONFIG();
    eth_phy_config_t phy_cfg = ETH_PHY_DEFAULT_CONFIG();
    phy_cfg.autonego_timeout_ms = 100;
    esp_eth_mac_t *mac = esp_eth_mac_new_openeth(&mac_cfg);
    esp_eth_phy_t *phy = esp_eth_phy_new_generic(&phy_cfg);
    esp_eth_config_t cfg = ETH_DEFAULT_CONFIG(mac, phy);
    if (!mac || !phy || esp_eth_driver_install(&cfg, &eth) != ESP_OK) {
        eth = NULL;
        return false;
    }
    esp_netif_attach(netif, esp_eth_new_netif_glue(eth));
    esp_event_handler_register(IP_EVENT, IP_EVENT_ETH_GOT_IP, on_ip, NULL);
    return esp_eth_start(eth) == ESP_OK;
}

bool net_up(char *why, size_t len)
{
    bool down;
    hw_sim_lock_net(&down);
    if (down) {
        snprintf(why, len, "no network (SIM NET 0)");
        return false;
    }
    if (!start()) {
        snprintf(why, len, "no network device in this QEMU run");
        return false;
    }
    if (!(xEventGroupWaitBits(events, GOT_IP, pdFALSE, pdTRUE, pdMS_TO_TICKS(8000)) & GOT_IP)) {
        snprintf(why, len, "no address (QEMU without -nic user,model=open_eth)");
        return false;
    }
    // the clock, from the mock server
    const char *base = http_sim_base();
    char url[96], *body = NULL;
    int status = 0;
    snprintf(url, sizeof url, "%s/sim/time", base ? base : "");
    if (!base || !http_get(url, false, 64, &status, &body, NULL) || status != 200) {
        snprintf(why, len, "the mock server did not give the time");
        free(body);
        return false;
    }
    struct timeval now = {.tv_sec = (time_t)strtoll(body, NULL, 10)};
    free(body);
    settimeofday(&now, NULL);
    up = true;
    return true;
}

void net_down(void)
{
    http_close();
    up = false;
}

bool net_is_up(void)
{
    return up;
}

void net_on_lost(void (*lost)(void))
{
    (void)lost;
}
