// SOURCES: firmware/components/clip/clip_button.c firmware/components/clip/clip_sm.c firmware/components/clip/clip_power.c
// The button's debounce dates each edge, so a loop that is late still tells a press from a hold.
#include "clip.h"
#include "unit.h"

static clip_button_t button;
static int64_t at;

static void a_press_is_dated_when_the_pin_first_moved(void)
{
    clip_button_init(&button, false);
    CHECK(!clip_button_sample(&button, false, 1000, &at));
    CHECK(!clip_button_sample(&button, true, 1010, &at));
    CHECK(!clip_button_sample(&button, true, 1020, &at));
    CHECK(!clip_button_sample(&button, true, 1030, &at));
    CHECK(clip_button_sample(&button, true, 1040, &at));
    CHECK_INT(at, 1010);
    CHECK(button.down);
    CHECK(!clip_button_sample(&button, true, 1050, &at)); // once
    CHECK(!clip_button_sample(&button, false, 1200, &at));
    CHECK(clip_button_sample(&button, false, 1230, &at));
    CHECK_INT(at, 1200);
    CHECK(!button.down);
}

static void a_bounce_is_not_a_press(void)
{
    clip_button_init(&button, false);
    CHECK(!clip_button_sample(&button, true, 0, &at));
    CHECK(!clip_button_sample(&button, false, 10, &at));
    CHECK(!clip_button_sample(&button, true, 20, &at));
    CHECK(!clip_button_sample(&button, false, 30, &at));
    // the count starts again at the last time the pin moved
    CHECK(!clip_button_sample(&button, true, 40, &at));
    CHECK(!clip_button_sample(&button, true, 60, &at));
    CHECK(clip_button_sample(&button, true, 70, &at));
    CHECK_INT(at, 40);
}

static void a_late_sampler_still_dates_the_edge(void)
{
    // the sampler itself was held up (flash being written): the next sample settles it
    clip_button_init(&button, false);
    CHECK(!clip_button_sample(&button, true, 500, &at));
    CHECK(clip_button_sample(&button, true, 900, &at));
    CHECK_INT(at, 500);
}

static void starts_down_after_a_wake_by_the_button(void)
{
    clip_button_init(&button, true);
    CHECK(!clip_button_sample(&button, true, 0, &at));
    CHECK(!clip_button_sample(&button, false, 100, &at));
    CHECK(clip_button_sample(&button, false, 130, &at));
    CHECK_INT(at, 100);
}

// What went wrong in QEMU (PROGRESS.md): the loop was 5 s late, and a 220 ms press became a hold.
static void a_late_loop_does_not_turn_a_press_into_a_hold(void)
{
    clip_sm_t sm;
    clip_sm_init(&sm, true);
    clip_sm_step(&sm, &(clip_event_t){.kind = CLIP_EV_BATTERY, .cell_mv = 3900}, 0);
    int64_t down_at = 0, up_at = 0;
    clip_button_init(&button, false);
    for (int64_t t = 4490; t <= 4800; t += 10) { // the sampler: pressed from 4493 to 4714
        bool pin = t >= 4493 && t < 4714;
        if (clip_button_sample(&button, pin, t, &at)) {
            *(button.down ? &down_at : &up_at) = at;
        }
    }
    CHECK_INT(down_at, 4500);
    CHECK_INT(up_at, 4720);
    // the loop gets to them at 10 s, in order, each with its own time, and only then ticks
    clip_output_t out = clip_sm_step(&sm, &(clip_event_t){.kind = CLIP_EV_BUTTON_DOWN}, down_at);
    CHECK(out.actions & CLIP_DO_RECORD_START);
    clip_sm_step(&sm, &(clip_event_t){.kind = CLIP_EV_BUTTON_UP}, up_at);
    out = clip_sm_step(&sm, &(clip_event_t){.kind = CLIP_EV_TICK}, 10021);
    CHECK(!(out.actions & CLIP_DO_RECORD_ADDITION));
    CHECK_INT(sm.state, CLIP_RECORDING);
    CHECK_INT(sm.kind, CLIP_NEW);
}

static void a_real_hold_is_still_a_hold(void)
{
    clip_sm_t sm;
    clip_sm_init(&sm, true);
    clip_sm_step(&sm, &(clip_event_t){.kind = CLIP_EV_BATTERY, .cell_mv = 3900}, 0);
    clip_sm_step(&sm, &(clip_event_t){.kind = CLIP_EV_BUTTON_DOWN}, 4500);
    clip_output_t out = clip_sm_step(&sm, &(clip_event_t){.kind = CLIP_EV_TICK}, 4500 + CLIP_HOLD_MS);
    CHECK(out.actions & CLIP_DO_RECORD_ADDITION);
    CHECK_INT(sm.kind, CLIP_ADDITION);
}

int main(void)
{
    RUN(a_press_is_dated_when_the_pin_first_moved);
    RUN(a_bounce_is_not_a_press);
    RUN(a_late_sampler_still_dates_the_edge);
    RUN(starts_down_after_a_wake_by_the_button);
    RUN(a_late_loop_does_not_turn_a_press_into_a_hold);
    RUN(a_real_hold_is_still_a_hold);
    return unit_done(__FILE__);
}
