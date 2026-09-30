// SOURCES: firmware/components/clip/clip_sm.c firmware/components/clip/clip_power.c
// The clip's state machine: every transition, and the rules of spec.md R2-R8. The actions of
// each step are also carried out on a model of the Wi-Fi radio and the charger's fast setting,
// in the order the app must use, to show the two are never on together.
#include "clip.h"
#include "unit.h"

#define MINUTE 60000

static clip_sm_t sm;
static int64_t now;
static clip_output_t out;
static bool hw_wifi, hw_fast;

// Everything but the light.
#define ACTIONS (out.actions & ~(unsigned)CLIP_DO_LED)

static void apply(void)
{
    unsigned a = out.actions;
    // only one of each pair in a step, or the order between them would matter
    CHECK(!((a & CLIP_DO_CHARGE_SLOW) && (a & CLIP_DO_CHARGE_FAST)));
    CHECK(!((a & CLIP_DO_WIFI_ON) && (a & CLIP_DO_WIFI_OFF)));
    CHECK(!((a & CLIP_DO_RECORD_START) && (a & CLIP_DO_RECORD_STOP)));
    if (a & CLIP_DO_CHARGE_SLOW) {
        hw_fast = false;
    }
    if (a & CLIP_DO_WIFI_OFF) {
        hw_wifi = false;
    }
    if (a & CLIP_DO_WIFI_ON) {
        CHECK(!hw_fast);
        hw_wifi = true;
    }
    if (a & CLIP_DO_UPLOAD) {
        CHECK(hw_wifi);
        CHECK(!hw_fast);
    }
    if (a & CLIP_DO_CHARGE_FAST) {
        CHECK(!hw_wifi);
        CHECK(sm.usb);
        hw_fast = true;
    }
    if (a & CLIP_DO_SLEEP) {
        CHECK(!hw_wifi);
        CHECK(!hw_fast);
        CHECK(!sm.usb);
        CHECK(!sm.button_down);
        CHECK_INT(sm.state, CLIP_ASLEEP);
    } else {
        CHECK_INT(out.wake_after_ms, 0);
    }
    CHECK_INT(hw_fast, sm.charge_fast);
    CHECK_INT(hw_wifi, sm.wifi != CLIP_WIFI_OFF);
    CHECK(!(sm.charge_fast && sm.wifi != CLIP_WIFI_OFF));
    CHECK(!(sm.charge_fast && !sm.usb));
}

static unsigned send(clip_event_t event)
{
    out = clip_sm_step(&sm, &event, now);
    // the radio dropping by itself is news from the app, not an action
    if (event.kind == CLIP_EV_WIFI && !event.on) {
        hw_wifi = false;
    }
    apply();
    return ACTIONS;
}

static unsigned tick(int64_t ms)
{
    now += ms;
    return send((clip_event_t){.kind = CLIP_EV_TICK});
}

static unsigned down(void)
{
    return send((clip_event_t){.kind = CLIP_EV_BUTTON_DOWN});
}

static unsigned up(void)
{
    return send((clip_event_t){.kind = CLIP_EV_BUTTON_UP});
}

static unsigned usb(bool on)
{
    return send((clip_event_t){.kind = CLIP_EV_USB, .on = on});
}

static unsigned battery(int cell_mv)
{
    return send((clip_event_t){.kind = CLIP_EV_BATTERY, .cell_mv = cell_mv});
}

static unsigned queue(int queued, int64_t wait_ms)
{
    return send((clip_event_t){.kind = CLIP_EV_QUEUE, .queued = queued, .wait_ms = wait_ms});
}

static unsigned wifi(bool on)
{
    return send((clip_event_t){.kind = CLIP_EV_WIFI, .on = on});
}

static unsigned uploaded(clip_upload_t result, int queued, int64_t wait_ms)
{
    return send((clip_event_t){.kind = CLIP_EV_UPLOADED, .upload = result, .queued = queued, .wait_ms = wait_ms});
}

// Power-on with a good battery.
static void start(bool has_last_note)
{
    clip_sm_init(&sm, has_last_note);
    now = 1000;
    hw_wifi = hw_fast = false;
    battery(4000);
    queue(0, -1);
}

// A deep-sleep wake, with what the app reads first.
static void wake(int64_t slept_ms, int cell_mv, int queued, int64_t wait_ms)
{
    CHECK_INT(sm.state, CLIP_ASLEEP);
    now += slept_ms;
    clip_sm_wake(&sm);
    CHECK_INT(sm.state, CLIP_IDLE);
    battery(cell_mv);
    queue(queued, wait_ms);
}

// A press, a few seconds of speech, a press: the recording is stopped with the button still down.
static unsigned record_and_stop(void)
{
    CHECK_INT(down() & CLIP_DO_RECORD_START, CLIP_DO_RECORD_START);
    tick(200);
    up();
    tick(5000);
    return down();
}

// ---- the button ----------------------------------------------------------------------------------

static void a_press_records_a_new_note(void)
{
    start(true);
    CHECK_INT(down(), CLIP_DO_RECORD_START);
    CHECK(out.actions & CLIP_DO_LED);
    CHECK_INT(sm.state, CLIP_PRESSED);
    CHECK_INT(sm.led, CLIP_LED_RECORDING);
    CHECK_INT(sm.led_since_ms, now);
    CHECK_INT(tick(300), 0);
    CHECK_INT(up(), 0);
    CHECK_INT(sm.state, CLIP_RECORDING);
    CHECK_INT(sm.kind, CLIP_NEW);
    // long after the hold time, it is still a new note
    CHECK_INT(tick(5000), 0);
    CHECK_INT(sm.kind, CLIP_NEW);
    CHECK_INT(sm.led, CLIP_LED_RECORDING);
}

static void a_hold_adds_to_the_last_note(void)
{
    start(true);
    CHECK_INT(down(), CLIP_DO_RECORD_START);
    CHECK_INT(tick(CLIP_HOLD_MS - 1), 0);
    CHECK_INT(sm.state, CLIP_PRESSED);
    CHECK_INT(tick(1), CLIP_DO_RECORD_ADDITION);
    CHECK_INT(sm.state, CLIP_RECORDING);
    CHECK_INT(sm.kind, CLIP_ADDITION);
    CHECK_INT(sm.led, CLIP_LED_ADDING);
    // letting go after a hold does not stop the recording
    CHECK_INT(tick(400), 0);
    CHECK_INT(up(), 0);
    CHECK_INT(sm.state, CLIP_RECORDING);
    CHECK_INT(tick(3000), 0);
    // the next press does
    CHECK_INT(down(), CLIP_DO_RECORD_STOP | CLIP_DO_WIFI_ON);
    CHECK_INT(sm.kind, CLIP_ADDITION);
}

static void a_hold_with_no_last_note_is_a_new_note(void)
{
    start(false);
    down();
    CHECK_INT(tick(CLIP_HOLD_MS), 0);
    CHECK_INT(sm.state, CLIP_RECORDING);
    CHECK_INT(sm.kind, CLIP_NEW);
    CHECK_INT(sm.led, CLIP_LED_RECORDING);
    up();
    CHECK_INT(down(), CLIP_DO_RECORD_STOP | CLIP_DO_WIFI_ON);
    // that recording is a note a later hold can add to, uploaded yet or not
    CHECK(sm.has_last_note);
    up();
    down();
    CHECK_INT(sm.kind, CLIP_NEW);
    CHECK_INT(tick(CLIP_HOLD_MS), CLIP_DO_RECORD_ADDITION);
}

static void the_second_press_stops_and_the_note_is_sent_and_saved(void)
{
    start(false);
    CHECK_INT(record_and_stop(), CLIP_DO_RECORD_STOP | CLIP_DO_WIFI_ON);
    CHECK_INT(sm.state, CLIP_CONNECTING);
    CHECK_INT(sm.led, CLIP_LED_SENDING);
    CHECK_INT(sm.queued, 1);
    CHECK_INT(up(), 0);
    CHECK_INT(tick(800), 0);
    CHECK_INT(wifi(true), CLIP_DO_UPLOAD);
    CHECK_INT(sm.state, CLIP_UPLOADING);
    CHECK_INT(sm.led, CLIP_LED_SENDING);
    CHECK_INT(tick(4000), 0);
    CHECK_INT(uploaded(CLIP_UP_SAVED, 0, -1), 0);
    CHECK_INT(sm.state, CLIP_SHOWING);
    CHECK_INT(sm.led, CLIP_LED_SAVED);
    CHECK_INT(tick(1499), 0);
    CHECK_INT(tick(1), CLIP_DO_WIFI_OFF | CLIP_DO_SLEEP);
    CHECK_INT(out.wake_after_ms, 0);
    CHECK_INT(sm.state, CLIP_ASLEEP);
    CHECK_INT(sm.led, CLIP_LED_OFF);
}

static void asleep_nothing_happens_until_the_wake(void)
{
    start(false);
    CHECK_INT(tick(50), CLIP_DO_SLEEP);
    CHECK_INT(down(), 0);
    CHECK_INT(usb(true), 0);
    CHECK_INT(tick(1000), 0);
    CHECK_INT(wifi(true), 0);
    hw_wifi = false; // the model's, since the step was ignored
    sm.usb = false;
    CHECK_INT(sm.state, CLIP_ASLEEP);
}

static void a_recording_stops_at_fifteen_minutes(void)
{
    start(false);
    down();
    tick(100);
    up();
    CHECK_INT(tick(CLIP_RECORD_MAX_MS - 101), 0);
    CHECK_INT(sm.state, CLIP_RECORDING);
    CHECK_INT(tick(1), CLIP_DO_RECORD_STOP | CLIP_DO_WIFI_ON);
    CHECK_INT(sm.state, CLIP_CONNECTING);
    CHECK_INT(sm.queued, 1);
    // also with the button held down the whole time
    start(true);
    down();
    CHECK_INT(tick(CLIP_HOLD_MS), CLIP_DO_RECORD_ADDITION);
    CHECK_INT(tick(CLIP_RECORD_MAX_MS - CLIP_HOLD_MS), CLIP_DO_RECORD_STOP | CLIP_DO_WIFI_ON);
    // it does not sleep, or start another recording, until the button is let go
    wifi(false);
    CHECK_INT(tick(3000), 0);
    CHECK_INT(sm.state, CLIP_IDLE);
    CHECK_INT(up(), 0);
    CHECK_INT(tick(50), CLIP_DO_SLEEP);
}

static void a_wake_by_the_button_times_the_hold_from_the_press(void)
{
    start(true);
    tick(50);
    wake(10 * MINUTE, 4000, 0, -1);
    // the wake stub's time; the app gets to run 250 ms later
    CHECK_INT(down(), CLIP_DO_RECORD_START);
    CHECK_INT(tick(250), 0);
    CHECK_INT(sm.state, CLIP_PRESSED);
    CHECK_INT(tick(749), 0);
    CHECK_INT(tick(1), CLIP_DO_RECORD_ADDITION);
    // and a short press that was over before the app ran is a new note
    start(true);
    tick(50);
    wake(10 * MINUTE, 4000, 0, -1);
    down();
    now += 150;
    up();
    CHECK_INT(tick(1500), 0);
    CHECK_INT(sm.state, CLIP_RECORDING);
    CHECK_INT(sm.kind, CLIP_NEW);
}

static void a_repeated_button_event_changes_nothing(void)
{
    start(false);
    down();
    CHECK_INT(down(), 0);
    CHECK_INT(sm.state, CLIP_PRESSED);
    up();
    CHECK_INT(up(), 0);
    CHECK_INT(sm.state, CLIP_RECORDING);
}

// ---- Wi-Fi and the queue -------------------------------------------------------------------------

static void with_no_wifi_the_note_waits_and_is_tried_later(void)
{
    start(false);
    record_and_stop();
    up();
    int64_t failed_at = now;
    CHECK_INT(wifi(false), CLIP_DO_WIFI_OFF);
    CHECK_INT(sm.state, CLIP_SHOWING);
    CHECK_INT(sm.led, CLIP_LED_WAITING);
    CHECK_INT(sm.queued, 1);
    CHECK_INT(tick(3000), CLIP_DO_SLEEP);
    CHECK_INT(now + out.wake_after_ms, failed_at + MINUTE);

    // the timer wake: still no Wi-Fi, so the wait doubles
    wake(out.wake_after_ms, 4000, 1, 0);
    CHECK_INT(tick(0), CLIP_DO_WIFI_ON);
    CHECK_INT(sm.state, CLIP_CONNECTING);
    failed_at = now;
    wifi(false);
    CHECK_INT(tick(3000), CLIP_DO_SLEEP);
    CHECK_INT(now + out.wake_after_ms, failed_at + 2 * MINUTE);

    // back in range: sent, saved, and no timer for the next sleep
    wake(out.wake_after_ms, 4000, 1, 0);
    CHECK_INT(tick(0), CLIP_DO_WIFI_ON);
    CHECK_INT(wifi(true), CLIP_DO_UPLOAD);
    CHECK_INT(uploaded(CLIP_UP_SAVED, 0, -1), 0);
    CHECK_INT(sm.led, CLIP_LED_SAVED);
    CHECK_INT(sm.offline_tries, 0);
    CHECK_INT(tick(1500), CLIP_DO_WIFI_OFF | CLIP_DO_SLEEP);
    CHECK_INT(out.wake_after_ms, 0);
}

static void the_offline_wait_doubles_up_to_an_hour(void)
{
    CHECK_INT(clip_offline_backoff_ms(0), 0);
    CHECK_INT(clip_offline_backoff_ms(1), MINUTE);
    CHECK_INT(clip_offline_backoff_ms(2), 2 * MINUTE);
    CHECK_INT(clip_offline_backoff_ms(6), 32 * MINUTE);
    CHECK_INT(clip_offline_backoff_ms(7), 60 * MINUTE);
    CHECK_INT(clip_offline_backoff_ms(1000), 60 * MINUTE);
}

static void a_wake_before_anything_is_due_sleeps_again(void)
{
    start(false);
    tick(50);
    wake(MINUTE, 4000, 2, 10 * MINUTE);
    CHECK_INT(tick(0), CLIP_DO_SLEEP);
    CHECK_INT(out.wake_after_ms, 10 * MINUTE);
    // with an empty queue there is no timer
    wake(MINUTE, 4000, 0, -1);
    CHECK_INT(tick(0), CLIP_DO_SLEEP);
    CHECK_INT(out.wake_after_ms, 0);
}

static void a_new_recording_tries_wifi_at_once(void)
{
    start(false);
    record_and_stop();
    up();
    wifi(false);
    CHECK(sm.wifi_retry_ms > now);
    tick(3000);
    wake(1000, 4000, 1, 0); // the button, long before the timer
    CHECK_INT(down(), CLIP_DO_RECORD_START);
    tick(100);
    up();
    tick(2000);
    CHECK_INT(down(), CLIP_DO_RECORD_STOP | CLIP_DO_WIFI_ON);
    CHECK_INT(sm.queued, 2);
    CHECK_INT(sm.offline_tries, 0);
}

static void the_queue_is_sent_one_after_the_other(void)
{
    start(false);
    tick(50);
    wake(MINUTE, 4000, 2, 0);
    CHECK_INT(tick(0), CLIP_DO_WIFI_ON);
    CHECK_INT(wifi(true), CLIP_DO_UPLOAD);
    // the next one has to wait 5 s for the Gemini gate: Wi-Fi stays up, the light blue again
    CHECK_INT(uploaded(CLIP_UP_SAVED, 1, 5000), 0);
    CHECK_INT(sm.led, CLIP_LED_SAVED);
    CHECK_INT(tick(1500), 0);
    CHECK_INT(sm.state, CLIP_UPLOADING);
    CHECK_INT(sm.led, CLIP_LED_SENDING);
    CHECK_INT(tick(3499), 0);
    CHECK_INT(tick(1), CLIP_DO_UPLOAD);
    CHECK_INT(uploaded(CLIP_UP_SAVED, 0, -1), 0);
    CHECK_INT(tick(1500), CLIP_DO_WIFI_OFF | CLIP_DO_SLEEP);
}

static void wifi_lost_between_two_uploads(void)
{
    start(false);
    tick(50);
    wake(MINUTE, 4000, 2, 0);
    tick(0);
    wifi(true);
    uploaded(CLIP_UP_SAVED, 1, 5000);
    tick(1500);
    CHECK_INT(sm.state, CLIP_UPLOADING);
    CHECK_INT(wifi(false), CLIP_DO_SLEEP);
    CHECK_INT(out.wake_after_ms, 3500);
}

static void upload_results(void)
{
    // a short wait: stays up and tries again
    start(false);
    record_and_stop();
    up();
    wifi(true);
    CHECK_INT(uploaded(CLIP_UP_LATER, 1, 5000), 0);
    CHECK_INT(sm.state, CLIP_UPLOADING);
    CHECK_INT(sm.led, CLIP_LED_SENDING);
    CHECK_INT(tick(4999), 0);
    CHECK_INT(tick(1), CLIP_DO_UPLOAD);

    // a long wait: amber, then asleep until it is due
    CHECK_INT(uploaded(CLIP_UP_LATER, 1, 2 * MINUTE), 0);
    CHECK_INT(sm.state, CLIP_SHOWING);
    CHECK_INT(sm.led, CLIP_LED_WAITING);
    CHECK_INT(tick(3000), CLIP_DO_WIFI_OFF | CLIP_DO_SLEEP);
    CHECK_INT(out.wake_after_ms, 2 * MINUTE - 3000);

    // given up: red blinking, and nothing left to wake for
    wake(2 * MINUTE - 3000, 4000, 1, 0);
    tick(0);
    wifi(true);
    CHECK_INT(uploaded(CLIP_UP_GAVE_UP, 0, -1), 0);
    CHECK_INT(sm.led, CLIP_LED_ERROR);
    CHECK_INT(tick(3000), CLIP_DO_WIFI_OFF | CLIP_DO_SLEEP);
    CHECK_INT(out.wake_after_ms, 0);

    // an addition in which nothing was heard: the same
    wake(MINUTE, 4000, 1, 0);
    tick(0);
    wifi(true);
    CHECK_INT(uploaded(CLIP_UP_NOTHING_ADDED, 0, -1), 0);
    CHECK_INT(sm.led, CLIP_LED_ERROR);
    tick(3000);

    // GitHub refuses: red blinking, the note kept and tried hours later
    wake(MINUTE, 4000, 1, 0);
    tick(0);
    wifi(true);
    CHECK_INT(uploaded(CLIP_UP_STUCK, 1, 300 * MINUTE), 0);
    CHECK_INT(sm.led, CLIP_LED_ERROR);
    CHECK_INT(tick(3000), CLIP_DO_WIFI_OFF | CLIP_DO_SLEEP);
    CHECK_INT(out.wake_after_ms, 300 * MINUTE - 3000);

    // the network went away during the upload: amber, and the Wi-Fi wait
    wake(300 * MINUTE, 4000, 1, 0);
    tick(0);
    wifi(true);
    CHECK_INT(uploaded(CLIP_UP_OFFLINE, 1, 0), CLIP_DO_WIFI_OFF);
    CHECK_INT(sm.led, CLIP_LED_WAITING);
    CHECK_INT(tick(3000), CLIP_DO_SLEEP);
    CHECK_INT(out.wake_after_ms, MINUTE - 3000);
}

static void the_recorder_failing_blinks_red(void)
{
    start(false);
    down();
    tick(100);
    up();
    tick(3000);
    // flash full: what was recorded is queued by the app
    CHECK_INT(send((clip_event_t){.kind = CLIP_EV_RECORD_FAILED, .queued = 1}), 0);
    CHECK_INT(sm.state, CLIP_SHOWING);
    CHECK_INT(sm.led, CLIP_LED_ERROR);
    CHECK_INT(tick(3000), CLIP_DO_WIFI_ON);
    // and it means nothing when no recording is running
    CHECK_INT(send((clip_event_t){.kind = CLIP_EV_RECORD_FAILED, .queued = 7}), 0);
    CHECK_INT(sm.queued, 1);
    CHECK_INT(sm.state, CLIP_CONNECTING);
}

// ---- recording wins ------------------------------------------------------------------------------

static void a_press_during_an_upload_records_at_once(void)
{
    start(false);
    record_and_stop();
    up();
    wifi(true);
    CHECK(sm.upload_busy);
    CHECK_INT(down(), CLIP_DO_RECORD_START);
    CHECK_INT(sm.state, CLIP_PRESSED);
    CHECK_INT(sm.led, CLIP_LED_RECORDING);
    up();
    // the upload finishes behind the recording; the light stays red
    CHECK_INT(uploaded(CLIP_UP_SAVED, 0, -1), 0);
    CHECK_INT(sm.state, CLIP_RECORDING);
    CHECK_INT(sm.led, CLIP_LED_RECORDING);
    CHECK_INT(tick(2000), 0);
    // Wi-Fi is still up, so the new one goes straight out
    CHECK_INT(down(), CLIP_DO_RECORD_STOP | CLIP_DO_UPLOAD);
    CHECK_INT(sm.state, CLIP_UPLOADING);
    CHECK_INT(sm.queued, 1);
}

static void a_recording_that_ends_during_an_upload_waits_for_it(void)
{
    start(false);
    record_and_stop();
    up();
    wifi(true);
    down();
    tick(100);
    up();
    tick(1000);
    CHECK_INT(down(), CLIP_DO_RECORD_STOP); // no second upload while one runs
    CHECK_INT(sm.state, CLIP_UPLOADING);
    CHECK_INT(sm.led, CLIP_LED_SENDING);
    up();
    CHECK_INT(tick(1000), 0);
    CHECK_INT(uploaded(CLIP_UP_SAVED, 1, 0), 0);
    CHECK_INT(sm.led, CLIP_LED_SAVED);
    CHECK_INT(tick(1500), CLIP_DO_UPLOAD);
}

static void a_press_while_connecting_records_at_once(void)
{
    start(false);
    record_and_stop();
    up();
    CHECK_INT(sm.state, CLIP_CONNECTING);
    CHECK_INT(down(), CLIP_DO_RECORD_START);
    up();
    CHECK_INT(wifi(true), 0); // connected meanwhile; nothing is sent while recording
    CHECK_INT(sm.state, CLIP_RECORDING);
    tick(1000);
    CHECK_INT(down(), CLIP_DO_RECORD_STOP | CLIP_DO_UPLOAD);
    CHECK_INT(sm.queued, 2);

    // and when the connect fails meanwhile, it is tried again after the recording
    start(false);
    record_and_stop();
    up();
    down();
    up();
    CHECK_INT(wifi(false), 0);
    CHECK_INT(sm.state, CLIP_RECORDING);
    CHECK_INT(sm.led, CLIP_LED_RECORDING);
    tick(1000);
    CHECK_INT(down(), CLIP_DO_RECORD_STOP | CLIP_DO_WIFI_ON);
}

static void a_press_during_a_shown_result_records(void)
{
    start(false);
    record_and_stop();
    up();
    wifi(true);
    uploaded(CLIP_UP_SAVED, 0, -1);
    tick(500);
    CHECK_INT(down(), CLIP_DO_RECORD_START);
    CHECK_INT(sm.led, CLIP_LED_RECORDING);
    CHECK_INT(tick(2000), CLIP_DO_RECORD_ADDITION); // no sleep when the green flash would have ended
    CHECK_INT(sm.state, CLIP_RECORDING);
}

// ---- USB -----------------------------------------------------------------------------------------

static void on_usb_it_stays_awake(void)
{
    start(false);
    CHECK_INT(usb(true), CLIP_DO_CHARGE_FAST);
    for (int i = 0; i < 1000; i++) {
        CHECK_INT(tick(50), 0);
    }
    CHECK_INT(sm.state, CLIP_IDLE);
    CHECK_INT(sm.led, CLIP_LED_OFF);
    CHECK_INT(usb(false), CLIP_DO_CHARGE_SLOW);
    CHECK_INT(tick(50), CLIP_DO_SLEEP);
}

static void plugging_in_sends_the_queue(void)
{
    start(false);
    tick(50);
    // two notes waiting, the battery too low to send them, and Wi-Fi in its backoff
    wake(MINUTE, 3400, 2, 0);
    sm.offline_tries = 5;
    sm.wifi_retry_ms = now + 16 * MINUTE;
    CHECK_INT(usb(true), CLIP_DO_CHARGE_FAST);
    CHECK_INT(tick(50), CLIP_DO_CHARGE_SLOW | CLIP_DO_WIFI_ON);
    CHECK_INT(wifi(true), CLIP_DO_UPLOAD);
    CHECK_INT(uploaded(CLIP_UP_SAVED, 1, 5000), 0);
    tick(1500);
    CHECK_INT(tick(3500), CLIP_DO_UPLOAD);
    CHECK_INT(uploaded(CLIP_UP_SAVED, 0, -1), 0);
    // done, and on USB with Wi-Fi up: the firmware check, once
    CHECK_INT(tick(1500), CLIP_DO_CHECK_UPDATE);
    CHECK_INT(sm.state, CLIP_UPDATING);
    CHECK_INT(tick(5000), 0);
    // then Wi-Fi off, the fast charge back, awake
    CHECK_INT(send((clip_event_t){.kind = CLIP_EV_UPDATE_CHECKED}), CLIP_DO_WIFI_OFF | CLIP_DO_CHARGE_FAST);
    CHECK_INT(sm.state, CLIP_IDLE);
    CHECK_INT(tick(50), 0);
}

static void on_usb_a_waiting_note_is_tried_when_it_is_due(void)
{
    start(false);
    usb(true);
    record_and_stop();
    up();
    CHECK_INT(wifi(false), CLIP_DO_WIFI_OFF | CLIP_DO_CHARGE_FAST);
    CHECK_INT(tick(3000), 0);
    CHECK_INT(sm.state, CLIP_IDLE);
    CHECK_INT(tick(MINUTE - 3001), 0);
    CHECK_INT(tick(1), CLIP_DO_CHARGE_SLOW | CLIP_DO_WIFI_ON);
}

static void usb_overrides_the_battery_reading(void)
{
    start(false);
    usb(true);
    battery(0); // no cell fitted
    CHECK_INT(record_and_stop(), CLIP_DO_RECORD_STOP | CLIP_DO_CHARGE_SLOW | CLIP_DO_WIFI_ON);
}

// ---- the battery ---------------------------------------------------------------------------------

static void a_low_battery_records_but_does_not_upload(void)
{
    start(false);
    battery(CLIP_UPLOAD_MIN_MV - 1);
    CHECK_INT(sm.battery, CLIP_BATTERY_LOW);
    CHECK_INT(record_and_stop(), CLIP_DO_RECORD_STOP);
    CHECK_INT(sm.state, CLIP_SHOWING);
    CHECK_INT(sm.led, CLIP_LED_WAITING);
    up();
    // then the warning, then a sleep with no timer: only the button or USB wakes it
    CHECK_INT(tick(3000), 0);
    CHECK_INT(sm.led, CLIP_LED_LOW_BATTERY);
    CHECK_INT(tick(3000), CLIP_DO_SLEEP);
    CHECK_INT(out.wake_after_ms, 0);
    CHECK_INT(sm.queued, 1);
    // a reading just over the limit does not bring Wi-Fi back; one past the hysteresis does
    wake(MINUTE, CLIP_UPLOAD_MIN_MV + 10, 1, 0);
    CHECK_INT(tick(0), CLIP_DO_SLEEP);
    wake(MINUTE, CLIP_UPLOAD_MIN_MV + CLIP_BATTERY_HYSTERESIS_MV, 1, 0);
    CHECK_INT(tick(0), CLIP_DO_WIFI_ON);
}

static void an_empty_battery_does_not_record(void)
{
    start(false);
    battery(CLIP_RECORD_MIN_MV - 1);
    CHECK_INT(sm.battery, CLIP_BATTERY_EMPTY);
    CHECK_INT(down(), 0);
    CHECK_INT(sm.state, CLIP_SHOWING);
    CHECK_INT(sm.led, CLIP_LED_LOW_BATTERY);
    CHECK_INT(tick(3000), 0); // the button is still down
    CHECK_INT(sm.state, CLIP_IDLE);
    up();
    CHECK_INT(tick(50), CLIP_DO_SLEEP);
    CHECK_INT(sm.queued, 0);
}

static void a_battery_that_runs_out_ends_the_recording(void)
{
    start(false);
    down();
    tick(100);
    up();
    CHECK_INT(battery(3400), 0); // low: the recording goes on
    CHECK_INT(sm.state, CLIP_RECORDING);
    CHECK_INT(battery(3290), CLIP_DO_RECORD_STOP);
    CHECK_INT(sm.state, CLIP_SHOWING);
    CHECK_INT(sm.led, CLIP_LED_LOW_BATTERY);
    CHECK_INT(sm.queued, 1);
    CHECK_INT(tick(3000), CLIP_DO_SLEEP);
    CHECK_INT(out.wake_after_ms, 0);
}

static void a_timer_wake_does_not_blink_the_warning(void)
{
    start(false);
    tick(50);
    wake(MINUTE, 3400, 1, 0);
    CHECK_INT(tick(0), CLIP_DO_SLEEP); // nobody is looking
    CHECK_INT(sm.led, CLIP_LED_OFF);
}

// ---- the firmware check --------------------------------------------------------------------------

static void the_firmware_check_is_once_per_time_on_usb(void)
{
    start(false);
    // on the battery: never
    record_and_stop();
    up();
    wifi(true);
    uploaded(CLIP_UP_SAVED, 0, -1);
    CHECK_INT(tick(1500), CLIP_DO_WIFI_OFF | CLIP_DO_SLEEP);
    // on USB, after the queue, with Wi-Fi still up
    wake(MINUTE, 4000, 0, -1);
    usb(true);
    CHECK_INT(tick(50), 0); // Wi-Fi is not brought up for it
    record_and_stop();
    up();
    wifi(true);
    uploaded(CLIP_UP_SAVED, 0, -1);
    CHECK_INT(tick(1500), CLIP_DO_CHECK_UPDATE);
    CHECK_INT(sm.led, CLIP_LED_OFF);
    // the button still records during it
    CHECK_INT(down(), CLIP_DO_RECORD_START);
    up();
    CHECK_INT(send((clip_event_t){.kind = CLIP_EV_UPDATE_CHECKED}), 0);
    CHECK_INT(sm.state, CLIP_RECORDING);
    CHECK_INT(down(), CLIP_DO_RECORD_STOP | CLIP_DO_UPLOAD);
    up();
    uploaded(CLIP_UP_SAVED, 0, -1);
    // not a second time while it stays plugged in
    CHECK_INT(tick(1500), CLIP_DO_WIFI_OFF | CLIP_DO_CHARGE_FAST);
    // unplugged and plugged in again: once more
    usb(false);
    usb(true);
    record_and_stop();
    up();
    wifi(true);
    uploaded(CLIP_UP_SAVED, 0, -1);
    CHECK_INT(tick(1500), CLIP_DO_CHECK_UPDATE);
    // without an answer it stays there (the app's check has its own timeout)
    CHECK_INT(tick(10 * MINUTE), 0);
    CHECK_INT(sm.state, CLIP_UPDATING);
}

// ---- the charger ---------------------------------------------------------------------------------

static void fast_charge_only_on_usb_with_wifi_off(void)
{
    start(false);
    CHECK(!sm.charge_fast);
    record_and_stop(); // on the battery: never
    up();
    CHECK(!sm.charge_fast);
    CHECK_INT(usb(true), 0); // plugged in while connecting: not yet
    wifi(true);
    CHECK(!sm.charge_fast);
    uploaded(CLIP_UP_SAVED, 0, -1);
    CHECK(!sm.charge_fast); // Wi-Fi is still up behind the green flash
    CHECK_INT(tick(1500), CLIP_DO_CHECK_UPDATE);
    CHECK(!sm.charge_fast); // and during the firmware check
    CHECK_INT(send((clip_event_t){.kind = CLIP_EV_UPDATE_CHECKED}), CLIP_DO_WIFI_OFF | CLIP_DO_CHARGE_FAST);
    // the next upload releases it before Wi-Fi starts
    CHECK_INT(record_and_stop(), CLIP_DO_RECORD_STOP | CLIP_DO_CHARGE_SLOW | CLIP_DO_WIFI_ON);
    CHECK(CLIP_DO_CHARGE_SLOW < CLIP_DO_WIFI_ON && CLIP_DO_WIFI_OFF < CLIP_DO_CHARGE_FAST);
    CHECK(CLIP_DO_CHARGE_SLOW < CLIP_DO_SLEEP);
    up();
    CHECK_INT(wifi(false), CLIP_DO_WIFI_OFF | CLIP_DO_CHARGE_FAST);
    // unplugged: released, then asleep
    tick(3000);
    CHECK_INT(usb(false), CLIP_DO_CHARGE_SLOW);
    CHECK_INT(tick(50), CLIP_DO_SLEEP);
    // and after a wake it is only driven again once USB is seen
    wake(MINUTE, 4000, 1, 10 * MINUTE);
    CHECK(!sm.charge_fast);
    CHECK_INT(usb(true), CLIP_DO_CHARGE_FAST);
}

// Thousands of events in no sensible order: apply() checks every step.
static void fast_charge_and_wifi_are_never_on_together(void)
{
    uint32_t seed = 12345;
    int sleeps = 0, uploads = 0, fast = 0, records = 0, updates = 0;
    start(true);
    for (int i = 0; i < 200000; i++) {
        seed = seed * 1664525u + 1013904223u;
        uint32_t r = seed >> 8;
        if (sm.state == CLIP_ASLEEP) {
            sleeps++;
            wake(r % 100000, 3200 + (int)(r % 1000), (int)(r % 3), (int64_t)(r % 5) * 20000 - 1);
            continue;
        }
        switch (r % 16) {
        case 0:
            down();
            break;
        case 1:
            up();
            break;
        case 2:
            usb(r & 0x100);
            break;
        case 3:
            battery(3200 + (int)((r >> 4) % 1000));
            break;
        case 4:
        case 5:
            // the app only reports what it was asked for
            if (sm.wifi == CLIP_WIFI_CONNECTING) {
                wifi(r & 0x100);
            } else if (sm.wifi == CLIP_WIFI_UP && !sm.upload_busy && (r & 0x300) == 0) {
                wifi(false);
            }
            break;
        case 6:
        case 7:
            if (sm.upload_busy) {
                uploads++;
                uploaded((clip_upload_t)((r >> 8) % 6), (int)((r >> 12) % 3), (int64_t)((r >> 16) % 4) * 20000);
            }
            break;
        case 9:
            if (sm.state == CLIP_UPDATING) {
                updates++;
                send((clip_event_t){.kind = CLIP_EV_UPDATE_CHECKED});
            }
            break;
        case 8:
            if (sm.state == CLIP_PRESSED || sm.state == CLIP_RECORDING) {
                send((clip_event_t){.kind = CLIP_EV_RECORD_FAILED, .queued = 1});
            }
            break;
        default:
            tick((int64_t)(r >> 4) % 2000);
            break;
        }
        fast += sm.charge_fast;
        records += (out.actions & CLIP_DO_RECORD_START) != 0;
    }
    // the walk really went everywhere
    CHECK(sleeps > 100);
    CHECK(uploads > 100);
    CHECK(fast > 100);
    CHECK(records > 100);
    CHECK(updates > 20);
}

int main(void)
{
    RUN(a_press_records_a_new_note);
    RUN(a_hold_adds_to_the_last_note);
    RUN(a_hold_with_no_last_note_is_a_new_note);
    RUN(the_second_press_stops_and_the_note_is_sent_and_saved);
    RUN(asleep_nothing_happens_until_the_wake);
    RUN(a_recording_stops_at_fifteen_minutes);
    RUN(a_wake_by_the_button_times_the_hold_from_the_press);
    RUN(a_repeated_button_event_changes_nothing);
    RUN(with_no_wifi_the_note_waits_and_is_tried_later);
    RUN(the_offline_wait_doubles_up_to_an_hour);
    RUN(a_wake_before_anything_is_due_sleeps_again);
    RUN(a_new_recording_tries_wifi_at_once);
    RUN(the_queue_is_sent_one_after_the_other);
    RUN(wifi_lost_between_two_uploads);
    RUN(upload_results);
    RUN(the_recorder_failing_blinks_red);
    RUN(a_press_during_an_upload_records_at_once);
    RUN(a_recording_that_ends_during_an_upload_waits_for_it);
    RUN(a_press_while_connecting_records_at_once);
    RUN(a_press_during_a_shown_result_records);
    RUN(on_usb_it_stays_awake);
    RUN(plugging_in_sends_the_queue);
    RUN(on_usb_a_waiting_note_is_tried_when_it_is_due);
    RUN(usb_overrides_the_battery_reading);
    RUN(a_low_battery_records_but_does_not_upload);
    RUN(an_empty_battery_does_not_record);
    RUN(a_battery_that_runs_out_ends_the_recording);
    RUN(a_timer_wake_does_not_blink_the_warning);
    RUN(the_firmware_check_is_once_per_time_on_usb);
    RUN(fast_charge_only_on_usb_with_wifi_off);
    RUN(fast_charge_and_wifi_are_never_on_together);
    return unit_done(__FILE__);
}
