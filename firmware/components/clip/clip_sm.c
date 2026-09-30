// The clip's behaviour as one function of (state, event, time): what the button does, when it
// records, connects, uploads, shows a colour and sleeps (spec.md R2-R8). It owns no hardware; the
// app carries out the actions it returns. A recording always wins: a press during a connect or
// an upload starts recording at once, and the upload in flight finishes behind it.
#include "clip.h"

static void set_led(clip_sm_t *sm, clip_led_t led, int64_t now, clip_output_t *out)
{
    // a pattern that ends by itself starts over when it is asked for again
    if (sm->led != led || clip_led_patterns[led].total_ms != 0) {
        sm->led = led;
        sm->led_since_ms = now;
        out->actions |= CLIP_DO_LED;
    }
}

static void enter(clip_sm_t *sm, clip_state_t state, clip_led_t led, int64_t now, clip_output_t *out)
{
    sm->state = state;
    set_led(sm, led, now, out);
}

static void show(clip_sm_t *sm, clip_led_t led, int64_t now, clip_output_t *out)
{
    enter(sm, CLIP_SHOWING, led, now, out);
}

// On USB the board runs from the port, whatever the cell reads.
static bool can_upload(const clip_sm_t *sm)
{
    return sm->usb || sm->battery == CLIP_BATTERY_OK;
}

static bool can_record(const clip_sm_t *sm)
{
    return sm->usb || sm->battery != CLIP_BATTERY_EMPTY;
}

static void wifi_off(clip_sm_t *sm, clip_output_t *out)
{
    if (sm->wifi != CLIP_WIFI_OFF) {
        sm->wifi = CLIP_WIFI_OFF;
        out->actions |= CLIP_DO_WIFI_OFF;
    }
}

int64_t clip_offline_backoff_ms(int tries)
{
    const int64_t max = 60 * 60 * 1000;
    if (tries <= 0) {
        return 0;
    }
    return tries > 6 ? max : INT64_C(60000) << (tries - 1);
}

static void offline(clip_sm_t *sm, int64_t now, clip_output_t *out)
{
    wifi_off(sm, out);
    sm->offline_tries++;
    sm->wifi_retry_ms = now + clip_offline_backoff_ms(sm->offline_tries);
}

static void set_queue(clip_sm_t *sm, const clip_event_t *event, int64_t now)
{
    sm->queued = event->queued;
    sm->due_ms = now + (event->wait_ms > 0 ? event->wait_ms : 0);
}

// Nothing is being recorded or shown: upload, wait for the next upload, stay idle, or sleep.
static void next_step(clip_sm_t *sm, int64_t now, clip_output_t *out)
{
    if (sm->upload_busy) {
        enter(sm, CLIP_UPLOADING, CLIP_LED_SENDING, now, out);
        return;
    }
    bool work = sm->queued > 0 && can_upload(sm);
    int64_t at = sm->due_ms > sm->wifi_retry_ms ? sm->due_ms : sm->wifi_retry_ms;
    if (work && at <= now) {
        if (sm->wifi == CLIP_WIFI_UP) {
            out->actions |= CLIP_DO_UPLOAD;
            sm->upload_busy = true;
            enter(sm, CLIP_UPLOADING, CLIP_LED_SENDING, now, out);
            return;
        }
        if (sm->wifi == CLIP_WIFI_OFF) {
            sm->wifi = CLIP_WIFI_CONNECTING;
            out->actions |= CLIP_DO_WIFI_ON;
        }
        enter(sm, CLIP_CONNECTING, CLIP_LED_SENDING, now, out);
        return;
    }
    if (work && sm->wifi == CLIP_WIFI_UP && at - now <= CLIP_LINGER_MS) {
        enter(sm, CLIP_UPLOADING, CLIP_LED_SENDING, now, out);
        return;
    }
    if (sm->usb && sm->wifi == CLIP_WIFI_UP && !sm->update_checked) {
        // on USB power with Wi-Fi up anyway: the moment to look for new firmware
        sm->update_checked = true;
        out->actions |= CLIP_DO_CHECK_UPDATE;
        enter(sm, CLIP_UPDATING, CLIP_LED_OFF, now, out);
        return;
    }
    wifi_off(sm, out);
    if (sm->usb || sm->button_down) {
        // on USB the console is served; with the button down a sleep would end at once (it wakes on low)
        enter(sm, CLIP_IDLE, CLIP_LED_OFF, now, out);
        return;
    }
    if (sm->user_active && sm->battery != CLIP_BATTERY_OK) {
        sm->user_active = false;
        show(sm, CLIP_LED_LOW_BATTERY, now, out);
        return;
    }
    out->actions |= CLIP_DO_SLEEP;
    out->wake_after_ms = work ? at - now : 0;
    enter(sm, CLIP_ASLEEP, CLIP_LED_OFF, now, out);
}

// The recording ends and is queued. The user is here and may have walked back into range, so
// Wi-Fi is tried at once whatever the backoff said.
static void stop(clip_sm_t *sm, int64_t now, clip_output_t *out)
{
    out->actions |= CLIP_DO_RECORD_STOP;
    sm->queued++;
    sm->due_ms = now;
    sm->has_last_note = true;
    sm->offline_tries = 0;
    sm->wifi_retry_ms = now;
}

static void stop_and_send(clip_sm_t *sm, int64_t now, clip_output_t *out)
{
    stop(sm, now, out);
    if (can_upload(sm)) {
        next_step(sm, now, out);
    } else {
        show(sm, CLIP_LED_WAITING, now, out);
    }
}

void clip_sm_init(clip_sm_t *sm, bool has_last_note)
{
    *sm = (clip_sm_t){.state = CLIP_IDLE, .has_last_note = has_last_note};
}

void clip_sm_wake(clip_sm_t *sm)
{
    sm->state = CLIP_IDLE;
    sm->led = CLIP_LED_OFF;
    sm->led_since_ms = 0;
    sm->usb = sm->button_down = sm->upload_busy = sm->user_active = sm->charge_fast = sm->update_checked = false;
    sm->wifi = CLIP_WIFI_OFF;
}

clip_output_t clip_sm_step(clip_sm_t *sm, const clip_event_t *event, int64_t now)
{
    clip_output_t out = {0};
    if (sm->state == CLIP_ASLEEP) {
        return out;
    }
    bool recording = sm->state == CLIP_PRESSED || sm->state == CLIP_RECORDING;
    switch (event->kind) {
    case CLIP_EV_USB:
        sm->usb = event->on;
        sm->update_checked &= event->on;
        if (event->on) {
            // plugged in, usually at home: try the queue now
            sm->offline_tries = 0;
            sm->wifi_retry_ms = now;
        }
        break;
    case CLIP_EV_BATTERY:
        sm->battery = clip_battery_level(sm->battery, event->cell_mv);
        if (recording && !can_record(sm)) {
            stop(sm, now, &out);
            sm->user_active = false;
            show(sm, CLIP_LED_LOW_BATTERY, now, &out);
        }
        break;
    case CLIP_EV_QUEUE:
        set_queue(sm, event, now);
        break;
    case CLIP_EV_BUTTON_DOWN:
        if (sm->button_down) {
            break;
        }
        sm->button_down = true;
        sm->user_active = true;
        if (sm->state == CLIP_RECORDING) {
            stop_and_send(sm, now, &out);
        } else if (!can_record(sm)) {
            sm->user_active = false;
            show(sm, CLIP_LED_LOW_BATTERY, now, &out);
        } else {
            // the mic starts now, so no speech is lost while press and hold are told apart
            out.actions |= CLIP_DO_RECORD_START;
            sm->kind = CLIP_NEW;
            sm->pressed_ms = now;
            enter(sm, CLIP_PRESSED, CLIP_LED_RECORDING, now, &out);
        }
        break;
    case CLIP_EV_BUTTON_UP:
        sm->button_down = false;
        if (sm->state == CLIP_PRESSED) {
            sm->state = CLIP_RECORDING;
        }
        break;
    case CLIP_EV_TICK:
        if (sm->state == CLIP_PRESSED && now - sm->pressed_ms >= CLIP_HOLD_MS) {
            sm->state = CLIP_RECORDING;
            if (sm->has_last_note) {
                sm->kind = CLIP_ADDITION;
                out.actions |= CLIP_DO_RECORD_ADDITION;
                set_led(sm, CLIP_LED_ADDING, now, &out);
            }
        }
        if (recording) {
            if (now - sm->pressed_ms >= CLIP_RECORD_MAX_MS) {
                stop_and_send(sm, now, &out);
            }
        } else if (sm->state == CLIP_SHOWING) {
            if (clip_led_over(sm->led, now - sm->led_since_ms)) {
                next_step(sm, now, &out);
            }
        } else if (sm->state == CLIP_IDLE || (sm->state == CLIP_UPLOADING && !sm->upload_busy)) {
            next_step(sm, now, &out);
        }
        break;
    case CLIP_EV_WIFI:
        if (event->on) {
            sm->wifi = CLIP_WIFI_UP;
            sm->offline_tries = 0;
            if (sm->state == CLIP_CONNECTING) {
                next_step(sm, now, &out);
            }
        } else {
            sm->wifi = CLIP_WIFI_OFF;
            if (sm->state == CLIP_CONNECTING) {
                out.actions |= CLIP_DO_WIFI_OFF; // stop trying
                offline(sm, now, &out);
                show(sm, CLIP_LED_WAITING, now, &out);
            } else if (sm->state == CLIP_UPLOADING && !sm->upload_busy) {
                next_step(sm, now, &out);
            }
        }
        break;
    case CLIP_EV_UPLOADED:
        sm->upload_busy = false;
        set_queue(sm, event, now);
        if (event->upload == CLIP_UP_OFFLINE) {
            offline(sm, now, &out);
        } else {
            sm->offline_tries = 0;
        }
        if (recording) {
            break; // the red light stays; the queue is looked at when the recording stops
        }
        switch (event->upload) {
        case CLIP_UP_SAVED:
            show(sm, CLIP_LED_SAVED, now, &out);
            break;
        case CLIP_UP_LATER:
            if (sm->queued > 0 && can_upload(sm) && sm->due_ms - now <= CLIP_LINGER_MS) {
                next_step(sm, now, &out);
                break;
            }
            // fall through
        case CLIP_UP_OFFLINE:
            show(sm, CLIP_LED_WAITING, now, &out);
            break;
        default: // nothing added, given up, stuck
            show(sm, CLIP_LED_ERROR, now, &out);
            break;
        }
        break;
    case CLIP_EV_UPDATE_CHECKED:
        if (sm->state == CLIP_UPDATING) {
            next_step(sm, now, &out);
        }
        break;
    case CLIP_EV_RECORD_FAILED:
        if (recording) {
            set_queue(sm, event, now);
            show(sm, CLIP_LED_ERROR, now, &out);
        }
        break;
    }
    // never together with Wi-Fi, and never on the battery alone
    bool fast = sm->usb && sm->wifi == CLIP_WIFI_OFF && sm->state != CLIP_ASLEEP;
    if (fast != sm->charge_fast) {
        sm->charge_fast = fast;
        out.actions |= fast ? CLIP_DO_CHARGE_FAST : CLIP_DO_CHARGE_SLOW;
    }
    return out;
}
