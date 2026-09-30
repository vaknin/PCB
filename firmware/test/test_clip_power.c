// SOURCES: firmware/components/clip/clip_power.c
// The battery rules (research/2026-09-29-esp32-firmware.md §7) and the light's patterns (spec.md).
#include "clip.h"
#include "unit.h"

static void divider_gives_the_cell_voltage(void)
{
    CHECK_INT(clip_battery_cell_mv(1400), 4200);
    CHECK_INT(clip_battery_cell_mv(1150), 3450);
    CHECK_INT(clip_battery_cell_mv(1100), 3300);
    CHECK_INT(clip_battery_cell_mv(0), 0);
    // a full cell stays inside the 6 dB range's 1600 mV
    CHECK(4350 / CLIP_BATTERY_DIVIDER <= 1600);
}

static void levels_going_down(void)
{
    CHECK_INT(clip_battery_level(CLIP_BATTERY_OK, 4200), CLIP_BATTERY_OK);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_OK, 3450), CLIP_BATTERY_OK);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_OK, 3449), CLIP_BATTERY_LOW);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_OK, 3300), CLIP_BATTERY_LOW);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_OK, 3299), CLIP_BATTERY_EMPTY);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_LOW, 3300), CLIP_BATTERY_LOW);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_LOW, 3299), CLIP_BATTERY_EMPTY);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_OK, 0), CLIP_BATTERY_EMPTY);
}

static void levels_come_back_only_above_the_hysteresis(void)
{
    CHECK_INT(clip_battery_level(CLIP_BATTERY_EMPTY, 3300), CLIP_BATTERY_EMPTY);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_EMPTY, 3349), CLIP_BATTERY_EMPTY);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_EMPTY, 3350), CLIP_BATTERY_LOW);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_LOW, 3450), CLIP_BATTERY_LOW);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_LOW, 3499), CLIP_BATTERY_LOW);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_LOW, 3500), CLIP_BATTERY_OK);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_EMPTY, 3499), CLIP_BATTERY_LOW);
    CHECK_INT(clip_battery_level(CLIP_BATTERY_EMPTY, 3500), CLIP_BATTERY_OK);
    // a reading that wobbles around a limit changes the level once
    clip_battery_t level = CLIP_BATTERY_OK;
    int changes = 0;
    for (int i = 0; i < 20; i++) {
        clip_battery_t next = clip_battery_level(level, i % 2 ? 3470 : 3430);
        changes += next != level;
        level = next;
    }
    CHECK_INT(changes, 1);
    CHECK_INT(level, CLIP_BATTERY_LOW);
}

static void percent_follows_the_curve(void)
{
    CHECK_INT(clip_battery_percent(4200), 100);
    CHECK_INT(clip_battery_percent(4350), 100);
    CHECK_INT(clip_battery_percent(4100), 90);
    CHECK_INT(clip_battery_percent(3800), 45);
    CHECK_INT(clip_battery_percent(3750), 35);
    CHECK_INT(clip_battery_percent(3450), 4);
    CHECK_INT(clip_battery_percent(3301), 0);
    CHECK_INT(clip_battery_percent(3300), 0);
    CHECK_INT(clip_battery_percent(0), 0);
    CHECK_INT(clip_battery_percent(-5), 0);
    // never goes down as the voltage goes up, and stays within 0-100
    int before = 0;
    for (int mv = 2500; mv <= 4500; mv++) {
        int percent = clip_battery_percent(mv);
        CHECK(percent >= before && percent <= 100);
        before = percent;
    }
}

static void steady_colours_follow_the_spec(void)
{
    const int64_t times[] = {0, 1, 999, 60000, 15 * 60 * 1000};
    for (size_t i = 0; i < sizeof times / sizeof *times; i++) {
        CHECK_INT(clip_led_colour(CLIP_LED_OFF, times[i]), 0);
        CHECK_INT(clip_led_colour(CLIP_LED_RECORDING, times[i]), CLIP_RED);
        CHECK_INT(clip_led_colour(CLIP_LED_SENDING, times[i]), CLIP_BLUE);
        CHECK(!clip_led_over(CLIP_LED_OFF, times[i]));
        CHECK(!clip_led_over(CLIP_LED_RECORDING, times[i]));
        CHECK(!clip_led_over(CLIP_LED_ADDING, times[i]));
        CHECK(!clip_led_over(CLIP_LED_SENDING, times[i]));
    }
    CHECK_INT(CLIP_AMBER, CLIP_RED | CLIP_GREEN);
}

static void a_hold_winks_once(void)
{
    CHECK_INT(clip_led_colour(CLIP_LED_ADDING, 0), 0);
    CHECK_INT(clip_led_colour(CLIP_LED_ADDING, 149), 0);
    CHECK_INT(clip_led_colour(CLIP_LED_ADDING, 150), CLIP_RED);
    CHECK_INT(clip_led_colour(CLIP_LED_ADDING, 15 * 60 * 1000), CLIP_RED);
}

static void saved_is_a_green_flash(void)
{
    CHECK_INT(clip_led_colour(CLIP_LED_SAVED, 0), CLIP_GREEN);
    CHECK_INT(clip_led_colour(CLIP_LED_SAVED, 1499), CLIP_GREEN);
    CHECK(!clip_led_over(CLIP_LED_SAVED, 1499));
    CHECK_INT(clip_led_colour(CLIP_LED_SAVED, 1500), 0);
    CHECK(clip_led_over(CLIP_LED_SAVED, 1500));
}

static void waiting_is_amber_for_a_while(void)
{
    CHECK_INT(clip_led_colour(CLIP_LED_WAITING, 0), CLIP_AMBER);
    CHECK_INT(clip_led_colour(CLIP_LED_WAITING, 2999), CLIP_AMBER);
    CHECK_INT(clip_led_colour(CLIP_LED_WAITING, 3000), 0);
    CHECK(clip_led_over(CLIP_LED_WAITING, 3000));
}

// How many times the pattern turns on, and that it only ever shows `colour`.
static int blinks(clip_led_t led, uint8_t colour, int64_t until)
{
    int count = 0;
    uint8_t before = 0;
    for (int64_t t = 0; t <= until; t++) {
        uint8_t now = clip_led_colour(led, t);
        CHECK(now == 0 || now == colour);
        count += now && !before;
        before = now;
    }
    return count;
}

static void error_blinks_red_fast(void)
{
    CHECK_INT(clip_led_colour(CLIP_LED_ERROR, 0), CLIP_RED);
    CHECK_INT(clip_led_colour(CLIP_LED_ERROR, 149), CLIP_RED);
    CHECK_INT(clip_led_colour(CLIP_LED_ERROR, 150), 0);
    CHECK_INT(clip_led_colour(CLIP_LED_ERROR, 299), 0);
    CHECK_INT(clip_led_colour(CLIP_LED_ERROR, 300), CLIP_RED);
    CHECK_INT(blinks(CLIP_LED_ERROR, CLIP_RED, 10000), 10);
    CHECK(!clip_led_over(CLIP_LED_ERROR, 2999));
    CHECK(clip_led_over(CLIP_LED_ERROR, 3000));
}

static void low_battery_is_three_short_red_blinks(void)
{
    CHECK_INT(clip_led_colour(CLIP_LED_LOW_BATTERY, 0), CLIP_RED);
    CHECK_INT(clip_led_colour(CLIP_LED_LOW_BATTERY, 99), CLIP_RED);
    CHECK_INT(clip_led_colour(CLIP_LED_LOW_BATTERY, 100), 0);
    CHECK_INT(clip_led_colour(CLIP_LED_LOW_BATTERY, 999), 0);
    CHECK_INT(clip_led_colour(CLIP_LED_LOW_BATTERY, 1000), CLIP_RED);
    CHECK_INT(blinks(CLIP_LED_LOW_BATTERY, CLIP_RED, 10000), 3);
    CHECK(clip_led_over(CLIP_LED_LOW_BATTERY, 3000));
}

static void every_pattern_is_in_the_table(void)
{
    for (int led = CLIP_LED_OFF + 1; led < CLIP_LED_COUNT; led++) {
        const clip_led_pattern_t *p = &clip_led_patterns[led];
        CHECK(p->colour != 0);
        CHECK(p->colour <= (CLIP_RED | CLIP_GREEN | CLIP_BLUE));
        // a pattern that ends is dark from then on
        if (p->total_ms) {
            CHECK_INT(clip_led_colour((clip_led_t)led, p->total_ms), 0);
            CHECK_INT(clip_led_colour((clip_led_t)led, p->total_ms + 100000), 0);
        }
        CHECK_INT(clip_led_colour((clip_led_t)led, -1), 0);
    }
    CHECK_INT(clip_led_colour(CLIP_LED_COUNT, 0), 0);
    CHECK(clip_led_over(CLIP_LED_COUNT, 0));
}

int main(void)
{
    RUN(divider_gives_the_cell_voltage);
    RUN(levels_going_down);
    RUN(levels_come_back_only_above_the_hysteresis);
    RUN(percent_follows_the_curve);
    RUN(steady_colours_follow_the_spec);
    RUN(a_hold_winks_once);
    RUN(saved_is_a_green_flash);
    RUN(waiting_is_amber_for_a_while);
    RUN(error_blinks_red_fast);
    RUN(low_battery_is_three_short_red_blinks);
    RUN(every_pattern_is_in_the_table);
    return unit_done(__FILE__);
}
