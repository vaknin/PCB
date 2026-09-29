// SOURCES: firmware/components/provision/provision_text.c
#include "provision_text.h"
#include "unit.h"

static prov_cmd_t cmd;

static void set(void)
{
    CHECK_INT(prov_parse("PROV SET wifi_ssid 48656c6C6F", &cmd), PROV_SET);
    CHECK_STR(cmd.key, "wifi_ssid");
    CHECK_INT(cmd.len, 5);
    CHECK(memcmp(cmd.value, "Hello", 5) == 0);
    // line endings and repeated spaces
    CHECK_INT(prov_parse("PROV  SET  k  00ff\r\n", &cmd), PROV_SET);
    CHECK_INT(cmd.len, 2);
    CHECK_INT(cmd.value[1], 0xff);
}

static void del_and_list(void)
{
    CHECK_INT(prov_parse("PROV DEL gemini_api_key", &cmd), PROV_DEL);
    CHECK_STR(cmd.key, "gemini_api_key");
    CHECK_INT(prov_parse("PROV LIST\n", &cmd), PROV_LIST);
}

static void other_traffic_is_ignored(void)
{
    const char *lines[] = {"", "hello", "PROVX SET k 00", "prov SET k 00", "I (123) wifi: connected"};
    for (size_t i = 0; i < sizeof lines / sizeof *lines; i++) {
        CHECK_INT(prov_parse(lines[i], &cmd), PROV_NONE);
    }
}

static void bad_verb(void)
{
    CHECK_INT(prov_parse("PROV", &cmd), PROV_BAD);
    CHECK_STR(cmd.error, "verb");
    CHECK_INT(prov_parse("PROV GET k", &cmd), PROV_BAD);
    CHECK_STR(cmd.error, "verb");
    CHECK_INT(prov_parse("PROV set k 00", &cmd), PROV_BAD);
}

static void bad_key(void)
{
    const char *lines[] = {"PROV DEL", "PROV DEL Key", "PROV DEL a-b", "PROV DEL sixteen_chars_xx", "PROV SET  "};
    for (size_t i = 0; i < sizeof lines / sizeof *lines; i++) {
        CHECK_INT(prov_parse(lines[i], &cmd), PROV_BAD);
        CHECK_STR(cmd.error, "key");
        CHECK_STR(cmd.key, "");
    }
    CHECK_INT(prov_parse("PROV DEL fifteen_chars_x", &cmd), PROV_DEL); // 15 is the limit
}

static void bad_value(void)
{
    const char *lines[] = {"PROV SET k", "PROV SET k 0", "PROV SET k 0g", "PROV SET k 0x12", "PROV SET k -1"};
    for (size_t i = 0; i < sizeof lines / sizeof *lines; i++) {
        CHECK_INT(prov_parse(lines[i], &cmd), PROV_BAD);
        CHECK_STR(cmd.error, "value");
        CHECK_STR(cmd.key, "k"); // the reply names the key
        CHECK_INT(cmd.len, 0);
    }
    // a bad digit late in the value leaves nothing of the earlier bytes behind
    CHECK_INT(prov_parse("PROV SET k 414243zz", &cmd), PROV_BAD);
    CHECK_INT(cmd.value[0], 0);
}

static void extra_words(void)
{
    CHECK_INT(prov_parse("PROV SET k 4142 extra", &cmd), PROV_BAD);
    CHECK_STR(cmd.error, "line");
    CHECK_INT(cmd.value[0], 0);
    CHECK_INT(cmd.len, 0);
    CHECK_INT(prov_parse("PROV LIST all", &cmd), PROV_BAD);
    CHECK_INT(prov_parse("PROV DEL k k", &cmd), PROV_BAD);
}

static void size_limits(void)
{
    static char line[PROV_MAX_LINE + 8];
    int n = snprintf(line, sizeof line, "PROV SET %s ", "fifteen_chars_x");
    for (int i = 0; i < PROV_MAX_VALUE; i++) {
        n += snprintf(line + n, sizeof line - n, "%02x", i & 0xff);
    }
    CHECK(strlen(line) <= PROV_MAX_LINE); // the longest valid line fits the console buffer
    CHECK_INT(prov_parse(line, &cmd), PROV_SET);
    CHECK_INT(cmd.len, PROV_MAX_VALUE);
    CHECK_INT(cmd.value[PROV_MAX_VALUE - 1], 0xff);
    strcat(line, "00"); // one byte too many
    CHECK_INT(prov_parse(line, &cmd), PROV_BAD);
    CHECK_STR(cmd.error, "value");
}

int main(void)
{
    RUN(set);
    RUN(del_and_list);
    RUN(other_traffic_is_ignored);
    RUN(bad_verb);
    RUN(bad_key);
    RUN(bad_value);
    RUN(extra_words);
    RUN(size_limits);
    return unit_done(__FILE__);
}
