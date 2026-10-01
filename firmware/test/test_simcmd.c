// SOURCES: firmware/components/simcmd/simcmd.c
#include "simcmd.h"
#include "unit.h"

static int64_t now;
static int64_t clock_ms(void)
{
    return now;
}

static int changes, last_down = -1;
static void on_button(bool down, void *ctx)
{
    (void)ctx;
    changes++;
    last_down = down;
}

static char reply[96];

static void reset(void)
{
    now = 1000;
    changes = 0;
    last_down = -1;
    simcmd_init(clock_ms, on_button, NULL);
}

static const char *say(const char *line)
{
    strcpy(reply, "(none)");
    if (!simcmd_handle(line, reply, sizeof reply)) {
        return "(not SIM)";
    }
    return reply;
}

static void button_levels(void)
{
    reset();
    CHECK(!simcmd_button());
    CHECK_STR(say("SIM BUTTON 1"), "SIM OK BUTTON 1 @1000");
    CHECK(simcmd_button());
    CHECK_INT(changes, 1);
    CHECK_INT(last_down, 1);
    // the same level again is no change for the board
    CHECK_STR(say("SIM BUTTON 1\r\n"), "SIM OK BUTTON 1 @1000");
    CHECK_INT(changes, 1);
    now = 5000; // held: stays down however long
    CHECK(simcmd_button());
    CHECK_STR(say("SIM  BUTTON   0 "), "SIM OK BUTTON 0 @5000");
    CHECK(!simcmd_button());
    CHECK_INT(changes, 2);
    CHECK_INT(last_down, 0);
}

static void press_ends_by_the_clock(void)
{
    reset();
    CHECK_STR(say("SIM PRESS 300"), "SIM OK PRESS 300 @1000");
    CHECK(simcmd_button());
    now = 1299;
    CHECK(simcmd_button());
    CHECK_INT(changes, 1);
    now = 1300;
    CHECK(!simcmd_button());
    CHECK_INT(changes, 2);
    CHECK_INT(last_down, 0);
    // a press that is over ends at the next SIM line even if nobody polled
    CHECK_STR(say("SIM PRESS 50"), "SIM OK PRESS 50 @1300");
    now = 2000;
    CHECK_STR(say("SIM BUTTON 1"), "SIM OK BUTTON 1 @2000");
    CHECK_INT(changes, 5); // down, up (the press ended), down
    // BUTTON cancels a running press: it stays down
    CHECK_STR(say("SIM PRESS 10"), "SIM OK PRESS 10 @2000");
    CHECK_STR(say("SIM BUTTON 1"), "SIM OK BUTTON 1 @2000");
    now = 9000;
    CHECK(simcmd_button());
}

static void bad_lines(void)
{
    reset();
    const char *bad[][2] = {
        {"SIM BUTTON 2", "SIM ERR BUTTON 2"},
        {"SIM BUTTON", "SIM ERR BUTTON"},
        {"SIM BUTTON 1 2", "SIM ERR BUTTON 1 2"},
        {"SIM BUTTON x", "SIM ERR BUTTON x"},
        {"SIM PRESS 0", "SIM ERR PRESS 0"},
        {"SIM PRESS 60001", "SIM ERR PRESS 60001"},
        {"SIM PRESS -5", "SIM ERR PRESS -5"},
        {"SIM PRESS 30ms", "SIM ERR PRESS 30ms"},
        {"SIM button 1", "SIM ERR button 1"},
        {"SIM BOGUS 1", "SIM ERR BOGUS 1"},
        {"SIM ", "SIM ERR "},
        {"SIM VERYLONGVERBNAME 1", "SIM ERR VERYLONGVERBNAME 1"},
    };
    for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
        CHECK_STR(say(bad[i][0]), bad[i][1]);
    }
    CHECK(!simcmd_button());
    CHECK_INT(changes, 0);
    // a line too long is refused, cut, never acted on
    char lng[200] = "SIM BUTTON 1";
    memset(lng + 12, ' ', 100);
    strcpy(lng + 112, "x");
    CHECK(strncmp(say(lng), "SIM ERR BUTTON 1", 16) == 0);
    CHECK(!simcmd_button());
    // not SIM lines: other console traffic
    CHECK_STR(say("PROV LIST"), "(not SIM)");
    CHECK_STR(say("SIMBUTTON 1"), "(not SIM)");
    CHECK_STR(say("sim BUTTON 1"), "(not SIM)");
    CHECK_STR(say(""), "(not SIM)");
}

static int skipped_ms = -1;
static bool skip(const char *arg, void *ctx)
{
    (void)ctx;
    char *end;
    long v = strtol(arg, &end, 10);
    if (!*arg || *end) {
        return false;
    }
    skipped_ms = (int)v;
    return true;
}

static int timers;
static bool timer(const char *arg, void *ctx)
{
    timers += *(int *)ctx;
    return arg[0] == 0;
}

static void board_verbs(void)
{
    reset();
    static int one = 1;
    CHECK(simcmd_register("SKIP", skip, NULL));
    CHECK(simcmd_register("TIMER", timer, &one));
    CHECK(!simcmd_register("SKIP", skip, NULL));   // taken
    CHECK(!simcmd_register("BUTTON", skip, NULL)); // built in
    CHECK(!simcmd_register("PRESS", skip, NULL));
    CHECK(!simcmd_register("lower", skip, NULL));
    CHECK(!simcmd_register("", skip, NULL));
    CHECK(!simcmd_register("X", NULL, NULL));
    CHECK_STR(say("SIM SKIP 2500"), "SIM OK SKIP 2500 @1000");
    CHECK_INT(skipped_ms, 2500);
    CHECK_STR(say("SIM SKIP soon"), "SIM ERR SKIP soon");
    CHECK_STR(say("SIM TIMER"), "SIM OK TIMER @1000");
    CHECK_STR(say("SIM TIMER 1"), "SIM ERR TIMER 1");
    CHECK_INT(timers, 2);
    // the table holds SIMCMD_MAX_VERBS
    char name[16];
    for (int i = 2; i < SIMCMD_MAX_VERBS; i++) {
        snprintf(name, sizeof name, "V%d", i);
        CHECK(simcmd_register(name, skip, NULL));
    }
    CHECK(!simcmd_register("FULL", skip, NULL));
    // init forgets them
    reset();
    CHECK_STR(say("SIM SKIP 1"), "SIM ERR SKIP 1");
}

static void serve_prints_the_reply(void)
{
    reset();
    unit_capture_start();
    bool sim = simcmd_serve("SIM PRESS 200");
    bool other = simcmd_serve("STATUS");
    char *out = unit_capture_end();
    CHECK(sim && !other);
    CHECK_STR(out, "SIM OK PRESS 200 @1000\n");
    free(out);
}

int main(void)
{
    RUN(button_levels);
    RUN(press_ends_by_the_clock);
    RUN(bad_lines);
    RUN(board_verbs);
    RUN(serve_prints_the_reply);
    return unit_done(__FILE__);
}
