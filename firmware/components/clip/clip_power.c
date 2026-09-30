// The battery rules and the light's patterns: small, and both only data and arithmetic.
#include "clip.h"

int clip_battery_cell_mv(int adc_mv)
{
    return adc_mv * CLIP_BATTERY_DIVIDER;
}

clip_battery_t clip_battery_level(clip_battery_t before, int cell_mv)
{
    // going down a level happens at its limit; coming back up needs the hysteresis on top
    if (cell_mv < CLIP_RECORD_MIN_MV) {
        return CLIP_BATTERY_EMPTY;
    }
    if (before == CLIP_BATTERY_EMPTY && cell_mv < CLIP_RECORD_MIN_MV + CLIP_BATTERY_HYSTERESIS_MV) {
        return CLIP_BATTERY_EMPTY;
    }
    if (cell_mv < CLIP_UPLOAD_MIN_MV) {
        return CLIP_BATTERY_LOW;
    }
    if (before != CLIP_BATTERY_OK && cell_mv < CLIP_UPLOAD_MIN_MV + CLIP_BATTERY_HYSTERESIS_MV) {
        return CLIP_BATTERY_LOW;
    }
    return CLIP_BATTERY_OK;
}

int clip_battery_loaded_mv(int loaded_mv)
{
    if (loaded_mv >= CLIP_UPLOAD_LOADED_MIN_MV) {
        // under load it reads below the at-rest limit and is still fine
        return loaded_mv > CLIP_UPLOAD_MIN_MV ? loaded_mv : CLIP_UPLOAD_MIN_MV;
    }
    // low, never empty: what the cell does under Wi-Fi says nothing about recording
    return CLIP_UPLOAD_MIN_MV - 1;
}

int clip_battery_percent(int cell_mv)
{
    // INFERRED: a generic single-cell LiPo discharge curve at a light load, not this cell's
    static const struct {
        int mv, percent;
    } curve[] = {{CLIP_RECORD_MIN_MV, 0}, {3500, 5}, {3600, 10}, {3700, 25}, {3800, 45}, {3900, 62}, {4000, 78}, {4100, 90}, {4200, 100}};
    const int last = (int)(sizeof curve / sizeof *curve) - 1;
    if (cell_mv <= curve[0].mv) {
        return 0;
    }
    for (int i = 1; i <= last; i++) {
        if (cell_mv < curve[i].mv) {
            int span = curve[i].mv - curve[i - 1].mv, rise = curve[i].percent - curve[i - 1].percent;
            return curve[i - 1].percent + ((cell_mv - curve[i - 1].mv) * rise + span / 2) / span;
        }
    }
    return 100;
}

const clip_led_pattern_t clip_led_patterns[CLIP_LED_COUNT] = {
    [CLIP_LED_OFF] = {0},
    [CLIP_LED_RECORDING] = {.colour = CLIP_RED},
    [CLIP_LED_ADDING] = {.colour = CLIP_RED, .lead_off_ms = 150},
    [CLIP_LED_SENDING] = {.colour = CLIP_BLUE},
    [CLIP_LED_SAVED] = {.colour = CLIP_GREEN, .total_ms = 1500},
    [CLIP_LED_WAITING] = {.colour = CLIP_AMBER, .total_ms = 3000},
    [CLIP_LED_ERROR] = {.colour = CLIP_RED, .on_ms = 150, .off_ms = 150, .total_ms = 3000},
    [CLIP_LED_LOW_BATTERY] = {.colour = CLIP_RED, .on_ms = 100, .off_ms = 900, .total_ms = 3000},
};

bool clip_led_over(clip_led_t led, int64_t t_ms)
{
    if (led < 0 || led >= CLIP_LED_COUNT) {
        return true;
    }
    return clip_led_patterns[led].total_ms != 0 && t_ms >= clip_led_patterns[led].total_ms;
}

uint8_t clip_led_colour(clip_led_t led, int64_t t_ms)
{
    if (led < 0 || led >= CLIP_LED_COUNT || t_ms < 0 || clip_led_over(led, t_ms)) {
        return 0;
    }
    const clip_led_pattern_t *p = &clip_led_patterns[led];
    if (t_ms < p->lead_off_ms) {
        return 0;
    }
    if (p->off_ms == 0) {
        return p->colour;
    }
    return (t_ms - p->lead_off_ms) % (p->on_ms + p->off_ms) < p->on_ms ? p->colour : 0;
}
