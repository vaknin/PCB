// capture-clip (D-024, D-025 Phase D.3): press, speak, press; the note appears on the phone and
// the laptop. This file ties the hardware (hw.h, app.h) to the logic in firmware/components:
// clip (state machine, queue, upload) and capture (the Capture client). The state machine
// decides; this loop feeds it events and carries out what it returns.
#include <stdarg.h>
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include <sys/time.h>
#include "app.h"
#include "board.h"
#include "board_pins.h"
#include "esp_app_desc.h"
#include "esp_attr.h"
#include "esp_heap_caps.h"
#include "esp_random.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/queue.h"
#include "freertos/task.h"
#include "hw.h"
#include "provision.h"

#define TICK_MS 20
#define BUTTON_SAMPLE_MS 10
// Who goes first. The loop that feeds the state machine is above the two tasks that work for
// seconds at a time: below them (it was at 1, where app_main starts) a press was acted on only
// when they paused, and the recording it starts would lose its first words. It only does short
// things while a recording runs. The simulators' console is above it: SIM lines are the pins.
#define PRIO_LOOP 7
#define PRIO_WORKER 5 // the recorder is at 6 (recorder.c): sound is never kept waiting by an upload
#define PRIO_CONSOLE (BOARD_IS_REAL ? 4 : 8)
#define BATTERY_EVERY_MS 30000
// From the button going down to app_main after a deep-sleep wake (INFERRED 100-250 ms,
// research/2026-09-29-esp32-firmware.md §8; Phase D.5 measures it). The press is dated back by
// this much, so a 1 s hold is a hold.
#define WAKE_BOOT_MS 150
#define CLOCK_SET_S 1767225600 // 2026-01-01: before this the clock was never set
#define KEEP_MAGIC 0xC11B0001u

extern const char prompt_new[] asm("_binary_system_prompt_txt_start");
extern const char prompt_append[] asm("_binary_system_prompt_append_txt_start");
extern const char response_schema[] asm("_binary_response_schema_json_start");

// Kept through deep sleep (RTC memory): the state machine with its retry times, Gemini's rate
// gate, and the last thing that went wrong. Lost, as it should be, when power is.
static RTC_DATA_ATTR struct {
    uint32_t magic, boot;
    clip_sm_t sm;
    cap_gate_t gate;
    char error[CLIP_STATUS_ERROR_LEN];
} keep;

typedef enum { MSG_EVENT, MSG_UPDATE_READY, MSG_RECORD_FAILED, MSG_SELFTEST } msg_kind_t;
typedef struct {
    msg_kind_t kind;
    clip_event_t event;
} msg_t;

typedef enum { JOB_WIFI_ON, JOB_WIFI_OFF, JOB_UPLOAD, JOB_UPDATE, JOB_CHARGE_FAST } job_t;

typedef struct {
    bool down;
    int64_t at_ms;
} edge_t;

static QueueHandle_t inbox, jobs, edges;
static TaskHandle_t main_task, worker_task;
// Jobs given and not yet finished. Counted, not a flag: the loop gives while the worker finishes.
static atomic_int jobs_open;
#define worker_busy (atomic_load(&jobs_open) > 0)
static clip_meta_t recording; // the recording in progress
static bool recording_open;
static int rest_mv;           // the cell, last read with Wi-Fi off
static bool parts_ok;
// The charger's fast rate is wanted (the state machine's word). It is switched on by the worker,
// in turn after the job that switches the radio off, so the two never overlap even for a moment.
static volatile bool want_fast;
static portMUX_TYPE fast_lock = portMUX_INITIALIZER_UNLOCKED;

static void charge_slow(void)
{
    taskENTER_CRITICAL(&fast_lock);
    want_fast = false;
    taskEXIT_CRITICAL(&fast_lock);
    hw_charge_fast(false);
}

void say(const char *event, const char *fmt, ...)
{
    char line[256];
    va_list args;
    va_start(args, fmt);
    vsnprintf(line, sizeof line, fmt, args);
    va_end(args);
    printf("CLIP {\"ev\":\"%s\",\"t\":%lld%s%s}\n", event, (long long)hw_uptime_ms(), line[0] ? "," : "", line);
}

static void post(const clip_event_t *event)
{
    msg_t msg = {.kind = MSG_EVENT, .event = *event};
    xQueueSend(inbox, &msg, portMAX_DELAY);
}

static void post_kind(msg_kind_t kind)
{
    msg_t msg = {.kind = kind};
    xQueueSend(inbox, &msg, portMAX_DELAY);
}

static int64_t wall_ms(void)
{
    struct timeval now;
    gettimeofday(&now, NULL);
    return now.tv_sec < CLOCK_SET_S ? 0 : (int64_t)now.tv_sec * 1000 + now.tv_usec / 1000;
}

static void clock_now(void *ctx, clip_clock_t *now)
{
    (void)ctx;
    *now = (clip_clock_t){.wall_ms = wall_ms(), .boot = keep.boot, .uptime_ms = hw_uptime_ms()};
}

static void set_error(const char *what)
{
    char time[CAP_ISO_LEN] = "";
    if (wall_ms()) {
        cap_iso_format(wall_ms() / 1000, time);
    }
    snprintf(keep.error, sizeof keep.error, "%s%s%s", time, time[0] ? " " : "", what);
    say("error", "\"text\":\"%s\"", what);
}

// The queue as it is on flash, as the event that tells the state machine.
static void queue_event(clip_event_t *event, clip_event_kind_t kind)
{
    clip_queue_status_t status = {0};
    clip_queue_status(store_get(), &keep.gate, wall_ms(), &status);
    *event = (clip_event_t){.kind = kind, .queued = status.queued, .wait_ms = status.wait_ms};
}

// ---- the worker: everything that waits for the network -------------------------------------------

static bool status_now(void *ctx, clip_upload_t result, clip_status_t *status)
{
    (void)ctx;
    if (result == CLIP_UP_GAVE_UP) {
        set_error("Gemini would not take a recording; it is kept on the clip");
    }
    *status = (clip_status_t){
        .cell_mv = rest_mv,
        .usb = hw_usb(),
        .charging = hw_charging(),
        .firmware = esp_app_get_description()->version,
        .time_s = wall_ms() / 1000,
        .error = keep.error,
    };
    clip_status_mark_t last;
    bool have = clip_status_mark_load(store_get(), &last);
    return clip_status_due(have ? &last : NULL, status);
}

static void status_done(void *ctx, const clip_status_t *status, bool ok)
{
    (void)ctx;
    clip_status_mark_t mark;
    clip_status_mark(status, &mark);
    if (ok) {
        clip_status_mark_save(store_get(), &mark);
    }
    say("status_file", "\"ok\":%s,\"percent\":%d", ok ? "true" : "false", mark.percent);
}

static void wifi_lost(void)
{
    post(&(clip_event_t){.kind = CLIP_EV_WIFI, .on = false});
}

static void job_wifi_on(void)
{
    char why[96] = "";
    charge_slow(); // the state machine already said so; the radio never starts without it
    bool ok = http_load_keys(why, sizeof why) && net_up(why, sizeof why);
    say("net", "\"up\":%s,\"why\":\"%s\"", ok ? "true" : "false", why);
    if (!ok) {
        if (strstr(why, "provisioned")) {
            set_error(why);
        }
        net_down();
    } else if (!hw_usb()) {
        // The cell under the radio's load: when it sags too far the state machine gets a "low"
        // and turns Wi-Fi off again; the queue waits for a charge.
        int loaded = hw_sim_loaded_mv() ? hw_sim_loaded_mv() : hw_battery_mv();
        if (clip_battery_loaded_mv(loaded) < CLIP_UPLOAD_MIN_MV) {
            say("battery", "\"loaded_mv\":%d,\"sags\":true", loaded);
            post(&(clip_event_t){.kind = CLIP_EV_BATTERY, .cell_mv = clip_battery_loaded_mv(loaded)});
        }
    }
    post(&(clip_event_t){.kind = CLIP_EV_WIFI, .on = ok});
}

// Why the queue's recordings are still waiting: the newest reason any of them keeps.
static void each_error(void *arg, const char *name)
{
    char id[CLIP_ID_LEN];
    clip_meta_t meta;
    size_t len = strlen(name);
    if (len == CLIP_ID_LEN - 1 + 5 && strcmp(name + len - 5, ".meta") == 0) {
        memcpy(id, name, CLIP_ID_LEN - 1);
        id[CLIP_ID_LEN - 1] = 0;
        if (clip_meta_load(store_get(), id, &meta) && meta.attempts.last_error[0]) {
            snprintf(arg, CLIP_STATUS_ERROR_LEN, "%.95s", meta.attempts.last_error);
        }
    }
}

static void job_upload(void)
{
    static const char *const names[] = {"saved", "nothing_added", "later", "offline", "gave_up", "stuck"};
    cap_github_t github = {.repo = http_repo(), .branch = http_branch(), .send = http_github};
    clip_uploader_t up = {
        .store = store_get(),
        .github = &github,
        .gemini = http_gemini,
        .prompt = prompt_new,
        .prompt_append = prompt_append,
        .schema = response_schema,
        .gate = &keep.gate,
        .status = status_now,
        .status_done = status_done,
        .clock = clock_now,
    };
    clip_queue_status_t after = {0};
    clip_upload_t result = clip_upload_next(&up, &after);
    if (result == CLIP_UP_STUCK) {
        set_error("GitHub refuses the clip (token or repo), or its storage is in trouble");
    }
    char why[CLIP_STATUS_ERROR_LEN] = "";
    if (result != CLIP_UP_SAVED) {
        store_get()->list(NULL, each_error, why);
        for (char *p = why; *p; p++) {
            if (*p == '"' || *p == '\\' || (unsigned char)*p < 0x20) {
                *p = ' ';
            }
        }
    }
    say("upload", "\"result\":\"%s\",\"queued\":%d,\"failed\":%d,\"wait_ms\":%lld,\"why\":\"%s\"", names[result],
        after.queued, after.failed, (long long)after.wait_ms, why);
    post(&(clip_event_t){.kind = CLIP_EV_UPLOADED, .upload = result, .queued = after.queued, .wait_ms = after.wait_ms});
}

static void job_update(void)
{
    char detail[128];
    update_result_t result = update_check(detail, sizeof detail);
    say("update", "\"step\":\"%s\",\"detail\":\"%s\"", result == UPDATE_READY ? "ready" : result == UPDATE_NONE ? "none" : "failed",
        detail);
    if (result == UPDATE_READY) {
        post_kind(MSG_UPDATE_READY);
    } else {
        post(&(clip_event_t){.kind = CLIP_EV_UPDATE_CHECKED});
    }
}

static void worker(void *arg)
{
    (void)arg;
    for (;;) {
        job_t job;
        xQueueReceive(jobs, &job, portMAX_DELAY);
        switch (job) {
        case JOB_WIFI_ON:
            job_wifi_on();
            break;
        case JOB_WIFI_OFF:
            net_down();
            say("net", "\"up\":false,\"why\":\"off\"");
            break;
        case JOB_UPLOAD:
            job_upload();
            break;
        case JOB_UPDATE:
            job_update();
            break;
        case JOB_CHARGE_FAST:
            // only if it is still wanted, and the radio really is off
            if (want_fast && !net_is_up()) {
                hw_charge_fast(true);
                if (!want_fast) {
                    hw_charge_fast(false); // taken back while it was being switched
                }
            }
            break;
        }
        atomic_fetch_sub(&jobs_open, 1);
    }
}

static void give(job_t job)
{
    atomic_fetch_add(&jobs_open, 1);
    xQueueSend(jobs, &job, portMAX_DELAY);
}

// ---- the recorder's side -------------------------------------------------------------------------

static void recorder_failed(void)
{
    post_kind(MSG_RECORD_FAILED);
}

static void record_start(void)
{
    uint8_t random[32];
    clip_clock_t now;
    esp_fill_random(random, sizeof random);
    clock_now(NULL, &now);
    recording_open = parts_ok && clip_queue_begin(store_get(), random, &now, &recording) &&
                     recorder_start(recording.id, recorder_failed);
    if (!recording_open) {
        if (recording.id[0]) {
            clip_queue_drop(store_get(), recording.id);
        }
        set_error("a recording could not start (storage full or in trouble)");
        post_kind(MSG_RECORD_FAILED);
    }
}

// Closes the recording and queues it; one with no audio is dropped.
static void record_close(void)
{
    if (!recording_open) {
        return;
    }
    recording_open = false;
    int64_t ms = recorder_stop();
    if (ms > 0 && clip_queue_finish(store_get(), &recording, ms)) {
        say("recorded", "\"id\":\"%s\",\"kind\":\"%s\",\"ms\":%lld", recording.id,
            recording.kind == CLIP_ADDITION ? "addition" : "new", (long long)ms);
    } else {
        clip_queue_drop(store_get(), recording.id);
        say("recorded", "\"id\":\"%s\",\"dropped\":true", recording.id);
    }
}

// ---- the state machine's actions -----------------------------------------------------------------

static const char *const state_names[] = {"idle", "pressed", "recording", "connecting", "uploading", "showing", "updating", "asleep"};
static const char *const led_names[] = {"off", "recording", "adding", "sending", "saved", "waiting", "error", "low_battery"};

static bool sleep_now;
static int64_t sleep_wake_after_ms;

static void say_state(void)
{
    say("state", "\"state\":\"%s\",\"led\":\"%s\",\"queued\":%d,\"battery\":%d,\"wifi\":%d,\"fast\":%s",
        state_names[keep.sm.state], led_names[keep.sm.led], keep.sm.queued, (int)keep.sm.battery, (int)keep.sm.wifi,
        keep.sm.charge_fast ? "true" : "false");
}

static void step_at(const clip_event_t *event, int64_t at_ms)
{
    clip_state_t state = keep.sm.state;
    clip_led_t led = keep.sm.led;
    clip_output_t out = clip_sm_step(&keep.sm, event, at_ms);
    unsigned a = out.actions;
    if (a & CLIP_DO_RECORD_STOP) {
        record_close();
        // the state machine counted it; this is what flash really holds
        clip_event_t queue;
        queue_event(&queue, CLIP_EV_QUEUE);
        post(&queue);
    }
    if (a & CLIP_DO_RECORD_START) {
        record_start();
    }
    if (a & CLIP_DO_RECORD_ADDITION) {
        bool added = recording_open && clip_queue_mark_addition(store_get(), &recording);
        say("addition", "\"marked\":%s", added ? "true" : "false");
    }
    if (a & CLIP_DO_CHARGE_SLOW) {
        charge_slow();
    }
    if (a & CLIP_DO_WIFI_OFF) {
        give(JOB_WIFI_OFF);
    }
    if (a & CLIP_DO_WIFI_ON) {
        give(JOB_WIFI_ON);
    }
    if (a & CLIP_DO_UPLOAD) {
        give(JOB_UPLOAD);
    }
    if (a & CLIP_DO_CHARGE_FAST) {
        want_fast = true;
        give(JOB_CHARGE_FAST);
    }
    if (a & CLIP_DO_CHECK_UPDATE) {
        give(JOB_UPDATE);
    }
    if (a & CLIP_DO_SLEEP) {
        sleep_now = true;
        sleep_wake_after_ms = out.wake_after_ms;
    }
    if (keep.sm.state != state || keep.sm.led != led) {
        say_state();
    }
}

static void step(const clip_event_t *event)
{
    step_at(event, hw_uptime_ms());
}

// ---- the button ----------------------------------------------------------------------------------
// Sampled from the esp_timer task, which nothing of ours can keep waiting, so an edge gets the
// time it happened even when the loop is late: a short press is then never taken for a hold.
// (Only a flash write holds the timer task up, for one erase at most.)

static clip_button_t debounce;
static esp_timer_handle_t sampler;
static volatile bool edges_lost;

static void sample_button(void *arg)
{
    (void)arg;
    edge_t edge;
    if (clip_button_sample(&debounce, hw_button(), hw_uptime_ms(), &edge.at_ms)) {
        edge.down = debounce.down;
        if (xQueueSend(edges, &edge, 0) != pdTRUE) {
            edges_lost = true;
        }
    }
}

static void button_watch(bool down)
{
    clip_button_init(&debounce, down);
    xQueueReset(edges);
    edges_lost = false;
    esp_timer_start_periodic(sampler, BUTTON_SAMPLE_MS * 1000);
}

static void button_unwatch(void)
{
    esp_timer_stop(sampler);
}

// The edges since the last look, each at its own time. *down is the level the state machine has.
static void button_events(bool *down)
{
    edge_t edge;
    while (xQueueReceive(edges, &edge, 0) == pdTRUE) {
        *down = edge.down;
        step_at(&(clip_event_t){.kind = edge.down ? CLIP_EV_BUTTON_DOWN : CLIP_EV_BUTTON_UP}, edge.at_ms);
    }
    if (edges_lost) {
        // more edges than the queue holds while the loop was away: go by the pin as it is now
        edges_lost = false;
        if (debounce.down != *down) {
            *down = !*down;
            step(&(clip_event_t){.kind = *down ? CLIP_EV_BUTTON_DOWN : CLIP_EV_BUTTON_UP});
        }
    }
}

static void marks(void)
{
    say("marks", "\"heap_internal_min\":%u,\"heap_psram_min\":%u,\"stack_main\":%u,\"stack_worker\":%u,\"stack_recorder\":%u,"
                 "\"store_free\":%lld",
        (unsigned)heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL), (unsigned)heap_caps_get_minimum_free_size(MALLOC_CAP_SPIRAM),
        (unsigned)uxTaskGetStackHighWaterMark(main_task), (unsigned)uxTaskGetStackHighWaterMark(worker_task),
        recorder_stack_free(), (long long)store_free_bytes());
}

// ---- the console: provisioning, and a few commands of the app's own ------------------------------

static void console_line(const char *line, void *ctx)
{
    (void)ctx;
    if (!BOARD_IS_REAL && hw_sim_command(line)) {
        return;
    }
    if (strcmp(line, "STATUS") == 0) {
        clip_queue_status_t status = {0};
        clip_queue_status(store_get(), &keep.gate, wall_ms(), &status);
        say("status", "\"state\":\"%s\",\"queued\":%d,\"failed\":%d,\"battery_mv\":%d,\"usb\":%s,\"fw\":\"%s\",\"error\":\"%s\"",
            state_names[keep.sm.state], status.queued, status.failed, rest_mv, hw_usb() ? "true" : "false",
            esp_app_get_description()->version, keep.error);
        marks();
    } else if (strcmp(line, "RETRY") == 0) {
        say("retry", "\"recordings\":%d", clip_queue_retry_failed(store_get()));
        clip_event_t queue;
        queue_event(&queue, CLIP_EV_QUEUE);
        post(&queue);
    } else if (strcmp(line, "SELFTEST") == 0) {
        post_kind(MSG_SELFTEST);
    }
}

static void console(void *arg)
{
    (void)arg;
    provision_serve_with(0, console_line, NULL);
}

// ---- one time awake -------------------------------------------------------------------------------

static void awake(hw_wake_t wake, int64_t entry_ms)
{
    clip_event_t event;
    // from the first moment: a press while the queue is still being read is kept, with its time
    button_watch(wake == HW_WAKE_BUTTON);
    bool fresh = wake == HW_WAKE_RESET || keep.magic != KEEP_MAGIC;
    if (fresh) {
        clip_recovery_t recovered = {0};
        char last[CLIP_ID_LEN];
        bool ok = parts_ok && clip_queue_recover(store_get(), &recovered);
        say("recover", "\"ok\":%s,\"queued\":%d,\"dropped\":%d,\"rebuilt\":%d", ok ? "true" : "false", recovered.queued,
            recovered.dropped, recovered.rebuilt);
        char error[CLIP_STATUS_ERROR_LEN];
        snprintf(error, sizeof error, "%s", keep.magic == KEEP_MAGIC ? keep.error : "");
        memset(&keep, 0, sizeof keep);
        snprintf(keep.error, sizeof keep.error, "%s", error);
        keep.magic = KEEP_MAGIC;
        keep.boot = esp_random() | 1;
        keep.gate = (cap_gate_t){.min_interval_ms = CAP_GEMINI_MIN_INTERVAL_MS};
        clip_sm_init(&keep.sm, clip_last_note(store_get(), last));
    } else {
        clip_sm_wake(&keep.sm);
    }
    bool usb = hw_usb(), button = wake == HW_WAKE_BUTTON;
    bool selftest_wanted = false;
    int64_t battery_at = hw_uptime_ms();
    hw_usb_present(usb);
    rest_mv = hw_battery_mv();
    say("wake", "\"cause\":%d,\"usb\":%s,\"battery_mv\":%d", (int)wake, usb ? "true" : "false", rest_mv);
    step(&(clip_event_t){.kind = CLIP_EV_USB, .on = usb});
    step(&(clip_event_t){.kind = CLIP_EV_BATTERY, .cell_mv = rest_mv});
    queue_event(&event, CLIP_EV_QUEUE);
    step(&event);
    if (button) {
        // dated back to when the button went down, which started this wake
        clip_output_t out = clip_sm_step(&keep.sm, &(clip_event_t){.kind = CLIP_EV_BUTTON_DOWN}, entry_ms - WAKE_BOOT_MS);
        if (out.actions & CLIP_DO_RECORD_START) {
            record_start();
        }
    }
    say_state();
    uint8_t colours = 0;
    bool update_ready = false;
    sleep_now = false;
    while (!sleep_now) {
        msg_t msg;
        button_events(&button);
        if (sleep_now) {
            break;
        }
        if (xQueueReceive(inbox, &msg, pdMS_TO_TICKS(TICK_MS)) == pdTRUE) {
            if (msg.kind == MSG_EVENT) {
                step(&msg.event);
            } else if (msg.kind == MSG_UPDATE_READY) {
                update_ready = true;
            } else if (msg.kind == MSG_RECORD_FAILED) {
                // it stopped by itself: what it wrote is queued, and the light says so
                record_close();
                queue_event(&event, CLIP_EV_RECORD_FAILED);
                step(&event);
            } else if (msg.kind == MSG_SELFTEST) {
                selftest_wanted = true; // run when nothing else is going on, not dropped
            }
            continue;
        }
        if (selftest_wanted && keep.sm.state == CLIP_IDLE && !worker_busy) {
            selftest_wanted = false;
            button_unwatch(); // the button test reads the pin itself; its press is not a recording
            selftests_run();  // its Wi-Fi test took the charger to the slow rate
            hw_init();
            hw_usb_present(hw_usb());
            if (want_fast) {
                give(JOB_CHARGE_FAST);
            }
            button = hw_button();
            button_watch(button);
        }
        // the other pins
        if (hw_usb() != usb) {
            usb = !usb;
            hw_usb_present(usb);
            step(&(clip_event_t){.kind = CLIP_EV_USB, .on = usb});
        }
        int64_t now = hw_uptime_ms();
        if (keep.sm.wifi == CLIP_WIFI_OFF && !worker_busy && now - battery_at >= BATTERY_EVERY_MS) {
            battery_at = now;
            rest_mv = hw_battery_mv();
            step(&(clip_event_t){.kind = CLIP_EV_BATTERY, .cell_mv = rest_mv});
        }
        step(&(clip_event_t){.kind = CLIP_EV_TICK});
        uint8_t want = clip_led_colour(keep.sm.led, hw_uptime_ms() - keep.sm.led_since_ms);
        if (want != colours) {
            colours = want;
            hw_led(colours);
        }
        // A new firmware is waiting: switch when nothing is being recorded or sent.
        if (update_ready && !worker_busy && (keep.sm.state == CLIP_UPDATING || keep.sm.state == CLIP_IDLE)) {
            net_down();
            update_apply();
            update_ready = false; // it could not be switched to
            step(&(clip_event_t){.kind = CLIP_EV_UPDATE_CHECKED});
        }
    }
    button_unwatch();
    // to sleep: the radio off and the secrets out of memory first
    while (worker_busy) {
        vTaskDelay(pdMS_TO_TICKS(TICK_MS));
    }
    record_close();
    charge_slow();
    net_down();
    http_forget();
    marks();
    pins_sleep(hw_pin_ops());
}

void app_main(void)
{
    board_start(BOARD_NAME, BOARD_REVISION);
    int64_t entry_ms = hw_uptime_ms();
    hw_wake_t wake = hw_wake();
    hw_init();
    main_task = xTaskGetCurrentTaskHandle();
    vTaskPrioritySet(NULL, PRIO_LOOP);
    inbox = xQueueCreate(16, sizeof(msg_t));
    jobs = xQueueCreate(8, sizeof(job_t));
    edges = xQueueCreate(16, sizeof(edge_t));
    esp_timer_create(&(esp_timer_create_args_t){.callback = sample_button, .name = "button"}, &sampler);
    net_on_lost(wifi_lost);
    bool mounted = store_mount();
    parts_ok = recorder_init() && mounted;
    xTaskCreate(worker, "worker", 12 * 1024, NULL, PRIO_WORKER, &worker_task);

    // A reset with someone at the USB console is bring-up: the self-tests, as devctl expects.
    // Not after a deep-sleep wake (every press is one), and not after a restart the firmware
    // made itself (an update) or a crash.
    esp_reset_reason_t reason = esp_reset_reason();
    bool own = reason == ESP_RST_SW || reason == ESP_RST_PANIC || reason == ESP_RST_INT_WDT || reason == ESP_RST_TASK_WDT ||
               reason == ESP_RST_WDT || reason == ESP_RST_BROWNOUT || reason == ESP_RST_DEEPSLEEP;
    if (update_on_trial()) {
        update_trial(parts_ok);
    } else if (wake == HW_WAKE_RESET && !own && (!BOARD_IS_REAL || hw_usb())) {
        selftests_run();
        hw_init();
    }
    if (update_rolled_back()[0] && !strstr(keep.error, "rolled back")) {
        char text[80];
        snprintf(text, sizeof text, "firmware %s failed its check and was rolled back", update_rolled_back());
        if (keep.magic != KEEP_MAGIC) {
            memset(&keep, 0, sizeof keep);
            keep.magic = KEEP_MAGIC; // so awake() keeps the text
        }
        set_error(text);
    }
    provision_console_init();
    xTaskCreate(console, "console", 6 * 1024, NULL, PRIO_CONSOLE, NULL);
    for (;;) {
        awake(wake, entry_ms);
        wake = hw_sleep(sleep_wake_after_ms); // on the board: no return, the wake is a restart
        entry_ms = hw_uptime_ms() + WAKE_BOOT_MS;
        hw_init();
    }
}
