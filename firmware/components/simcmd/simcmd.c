// SIM console commands: parsing and the simulated button (pure C with an injected clock, so
// scripts/fw-test.sh tests it on the laptop: firmware/test/test_simcmd.c). Protocol: simcmd.h.
#include "simcmd.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static struct {
    simcmd_clock_fn now;
    simcmd_button_fn on_button;
    void *ctx;
    // set by the console task, read by others: each fits one aligned 32-bit word
    volatile bool down;
    volatile int32_t release_at; // ms; 0 = no PRESS running
    struct {
        char name[SIMCMD_MAX_VERB + 1];
        simcmd_verb_fn fn;
        void *ctx;
    } verbs[SIMCMD_MAX_VERBS];
    size_t n_verbs;
} s;

static int64_t no_clock(void)
{
    return 0;
}

void simcmd_init(simcmd_clock_fn now_ms, simcmd_button_fn on_button, void *ctx)
{
    memset(&s, 0, sizeof s);
    s.now = now_ms ? now_ms : no_clock;
    s.on_button = on_button;
    s.ctx = ctx;
}

static bool is_verb(const char *v)
{
    size_t n = strlen(v);
    if (n == 0 || n > SIMCMD_MAX_VERB || v[0] < 'A' || v[0] > 'Z') {
        return false;
    }
    for (size_t i = 0; i < n; i++) {
        char c = v[i];
        if (!((c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_')) {
            return false;
        }
    }
    return true;
}

bool simcmd_register(const char *verb, simcmd_verb_fn fn, void *ctx)
{
    if (!fn || !is_verb(verb) || s.n_verbs == SIMCMD_MAX_VERBS || !strcmp(verb, "BUTTON") || !strcmp(verb, "PRESS")) {
        return false;
    }
    for (size_t i = 0; i < s.n_verbs; i++) {
        if (!strcmp(s.verbs[i].name, verb)) {
            return false;
        }
    }
    strcpy(s.verbs[s.n_verbs].name, verb);
    s.verbs[s.n_verbs].fn = fn;
    s.verbs[s.n_verbs].ctx = ctx;
    s.n_verbs++;
    return true;
}

static void set_button(bool down)
{
    bool was = s.down;
    s.down = down;
    if (was != down && s.on_button) {
        s.on_button(down, s.ctx);
    }
}

bool simcmd_button(void)
{
    int32_t at = s.release_at;
    if (at && s.now() >= at) {
        s.release_at = 0;
        set_button(false);
    }
    return s.down;
}

// A whole decimal number in [lo, hi].
static bool number(const char *a, long lo, long hi, long *out)
{
    if (!*a) {
        return false;
    }
    char *end;
    long v = strtol(a, &end, 10);
    if (*end || v < lo || v > hi) {
        return false;
    }
    *out = v;
    return true;
}

static bool builtin(const char *verb, const char *arg, bool *ok)
{
    long v;
    if (!strcmp(verb, "BUTTON")) {
        *ok = number(arg, 0, 1, &v);
        if (*ok) {
            s.release_at = 0;
            set_button(v == 1);
        }
        return true;
    }
    if (!strcmp(verb, "PRESS")) {
        *ok = number(arg, 1, SIMCMD_MAX_PRESS_MS, &v);
        if (*ok) {
            int32_t at = (int32_t)(s.now() + v);
            s.release_at = at ? at : 1;
            set_button(true);
        }
        return true;
    }
    return false;
}

bool simcmd_handle(const char *line, char *reply, size_t len)
{
    if (strncmp(line, "SIM ", 4) != 0) {
        return false;
    }
    simcmd_button(); // a PRESS that is over ends before anything else happens
    // the rest, without the line end and outer spaces
    char rest[64];
    const char *r = line + 4;
    while (*r == ' ') {
        r++;
    }
    size_t n = strcspn(r, "\r\n");
    while (n > 0 && r[n - 1] == ' ') {
        n--;
    }
    bool too_long = n >= sizeof rest;
    if (too_long) {
        n = sizeof rest - 1;
    }
    memcpy(rest, r, n);
    rest[n] = 0;

    // VERB, then at most one word
    char verb[SIMCMD_MAX_VERB + 2] = "", arg[SIMCMD_MAX_ARG + 2] = "";
    const char *p = rest;
    size_t vn = strcspn(p, " ");
    bool ok = !too_long && vn > 0 && vn <= SIMCMD_MAX_VERB;
    if (ok) {
        memcpy(verb, p, vn);
        p += vn;
        while (*p == ' ') {
            p++;
        }
        size_t an = strcspn(p, " ");
        ok = an <= SIMCMD_MAX_ARG && p[an] == 0 && is_verb(verb);
        if (ok) {
            memcpy(arg, p, an);
        }
    }
    if (ok && !builtin(verb, arg, &ok)) {
        ok = false;
        for (size_t i = 0; i < s.n_verbs; i++) {
            if (!strcmp(s.verbs[i].name, verb)) {
                ok = s.verbs[i].fn(arg, s.verbs[i].ctx);
                break;
            }
        }
    }
    if (ok) {
        snprintf(reply, len, "SIM OK %s%s%s @%lld", verb, arg[0] ? " " : "", arg, (long long)s.now());
    } else {
        snprintf(reply, len, "SIM ERR %s", rest);
    }
    return true;
}

bool simcmd_serve(const char *line)
{
    char reply[96];
    if (!simcmd_handle(line, reply, sizeof reply)) {
        return false;
    }
    printf("%s\n", reply);
    fflush(stdout);
    return true;
}
