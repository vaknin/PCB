// provision_handle and provision_get against a real NVS (ESP-IDF's linux target emulates the
// flash in a file). Run by scripts/fw-test.sh; prints the UNIT line like the gcc tests.
#include <stdlib.h>
#include "nvs_flash.h"
#include "provision.h"
#include "unit.h"

static char replies[2048];

static void collect(const char *reply, void *ctx)
{
    (void)ctx;
    strncat(replies, reply, sizeof replies - strlen(replies) - 2);
    strcat(replies, "\n");
}

// the reply lines to one command
static const char *ask(const char *line)
{
    replies[0] = 0;
    provision_handle(line, collect, NULL);
    return replies;
}

static void round_trip(void)
{
    char buf[64];
    CHECK_STR(ask("PROV SET wifi_ssid 486f6d65"), "PROV OK wifi_ssid 4\n");
    CHECK_INT(provision_get("wifi_ssid", buf, sizeof buf), ESP_OK);
    CHECK_STR(buf, "Home");
    CHECK_STR(ask("PROV SET wifi_ssid 4f6666696365"), "PROV OK wifi_ssid 6\n"); // overwrite
    CHECK_INT(provision_get("wifi_ssid", buf, sizeof buf), ESP_OK);
    CHECK_STR(buf, "Office");
    CHECK_STR(ask("PROV SET api_key 736563726574"), "PROV OK api_key 6\n");
    ask("PROV LIST"); // NVS lists in storage order
    CHECK(strstr(replies, "PROV KEY api_key 6\n") && strstr(replies, "PROV KEY wifi_ssid 6\n"));
    CHECK_INT(strlen(replies), strlen("PROV KEY api_key 6\nPROV KEY wifi_ssid 6\nPROV END\n"));
    CHECK(strstr(replies, "PROV END\n") == replies + strlen(replies) - 9);
    CHECK_STR(ask("PROV DEL wifi_ssid"), "PROV OK wifi_ssid 0\n");
    CHECK_STR(ask("PROV DEL wifi_ssid"), "PROV OK wifi_ssid 0\n"); // already gone is fine
    CHECK_INT(provision_get("wifi_ssid", buf, sizeof buf), ESP_ERR_NVS_NOT_FOUND);
    CHECK_STR(ask("PROV LIST"), "PROV KEY api_key 6\nPROV END\n");
    CHECK_STR(ask("PROV DEL api_key"), "PROV OK api_key 0\n");
}

static void values_never_come_back(void)
{
    CHECK(!strstr(ask("PROV SET token 736563726574"), "secret"));
    CHECK(!strstr(replies, "736563726574"));
    CHECK(!strstr(ask("PROV LIST"), "736563726574"));
    CHECK(!strstr(ask("PROV SET token 7365637265zz"), "7365")); // an error doesn't echo either
    ask("PROV DEL token");
}

static void errors(void)
{
    CHECK_STR(ask("PROV SET Bad 00"), "PROV ERR key\n");
    CHECK_STR(ask("PROV SET k 0"), "PROV ERR k value\n");
    CHECK_STR(ask("PROV SET k 00 extra"), "PROV ERR k line\n");
    CHECK_STR(ask("PROV GET k"), "PROV ERR verb\n");
    CHECK_STR(ask("PROV LIST"), "PROV END\n"); // none of those wrote anything
    CHECK_STR(ask("I (123) boot: something else"), ""); // other console traffic
}

static void get_needs_room_for_the_nul(void)
{
    char buf[6];
    CHECK_STR(ask("PROV SET five 3132333435"), "PROV OK five 5\n");
    CHECK_INT(provision_get("five", buf, 6), ESP_OK);
    CHECK_STR(buf, "12345");
    CHECK_INT(provision_get("five", buf, 5), ESP_ERR_NVS_INVALID_LENGTH);
    CHECK_STR(buf, "");
    CHECK_INT(provision_get("five", buf, 0), ESP_ERR_INVALID_ARG);
    ask("PROV DEL five");
}

static void largest_value_survives_a_restart(void)
{
    static char line[16 + 2 * 512 + 1], buf[513];
    int n = snprintf(line, sizeof line, "PROV SET big ");
    for (int i = 0; i < 512; i++) {
        n += snprintf(line + n, sizeof line - n, "%02x", 'a' + i % 26);
    }
    CHECK_STR(ask(line), "PROV OK big 512\n");
    nvs_flash_deinit();
    CHECK_INT(nvs_flash_init(), ESP_OK);
    CHECK_INT(provision_get("big", buf, sizeof buf), ESP_OK);
    CHECK_INT(strlen(buf), 512);
    CHECK_INT(buf[511], 'a' + 511 % 26);
    ask("PROV DEL big");
}

void app_main(void)
{
    nvs_flash_erase();
    if (nvs_flash_init() != ESP_OK) {
        printf("UNIT {\"file\":\"%s\",\"pass\":0,\"fail\":1}\n", "test_provision_nvs.c");
        exit(1);
    }
    RUN(round_trip);
    RUN(values_never_come_back);
    RUN(errors);
    RUN(get_needs_room_for_the_nul);
    RUN(largest_value_survives_a_restart);
    exit(unit_done(__FILE__));
}
