// SOURCES: boards/capture-clip/firmware/main/pins_seq.c
// The pin rules of board.toml against a made-up chip that watches every operation: CHG_FAST is
// never driven high, CHRG never has a pull-up, STDBY has one only while USB is in, the mic never
// sees a clock without power, and everything is off and held before a sleep.
#include "board_pins.h"
#include "clip.h"
#include "pins_seq.h"
#include "unit.h"

#define PINS 49

typedef struct {
    pin_mode_t mode[PINS];
    int level[PINS];
    bool held[PINS];
    bool i2s;
    int waited_ms; // since the I2S clocks last started
    bool broken;   // a rule was broken at some point
    char why[96];
} chip_t;

static chip_t chip;

static void broke(const char *why)
{
    if (!chip.broken) {
        chip.broken = true;
        snprintf(chip.why, sizeof chip.why, "%s", why);
    }
}

// The rules that must hold after every single operation, not only at the end of a sequence.
static void check(void)
{
    if (chip.mode[PIN_CHG_FAST] == PIN_OUTPUT || chip.mode[PIN_CHG_FAST] == PIN_INPUT_PULLUP) {
        broke("CHG_FAST driven push-pull or pulled up");
    }
    if (chip.mode[PIN_CHG_FAST] == PIN_OPEN_DRAIN && chip.level[PIN_CHG_FAST] != 0) {
        broke("CHG_FAST open drain and not low");
    }
    if (chip.mode[PIN_CHRG] != PIN_INPUT) {
        broke("CHRG is not a plain input");
    }
    if (chip.mode[PIN_VBUS_SENSE] != PIN_INPUT || chip.mode[PIN_BAT_ADC] != PIN_INPUT || chip.mode[PIN_BUTTON] != PIN_INPUT ||
        chip.mode[PIN_I2S_SD] != PIN_INPUT) {
        broke("an input has a pull or is driven");
    }
    if (chip.mode[PIN_STDBY] == PIN_OUTPUT || chip.mode[PIN_STDBY] == PIN_OPEN_DRAIN) {
        broke("STDBY driven");
    }
    bool powered = chip.mode[PIN_MIC_PWR] == PIN_OUTPUT && chip.level[PIN_MIC_PWR] == 1;
    if (chip.i2s && !powered) {
        broke("I2S clocks into an unpowered mic");
    }
    if (!powered && !chip.i2s) {
        // with the clocks stopped the pins are plain outputs again: they must be low
        for (int pin = PIN_I2S_SCK; pin <= PIN_I2S_WS; pin++) {
            if (chip.mode[pin] == PIN_OUTPUT && chip.level[pin] != 0) {
                broke("SCK or WS high at an unpowered mic");
            }
        }
    }
}

static void op_mode(void *ctx, int pin, pin_mode_t mode)
{
    (void)ctx;
    if (chip.held[pin]) {
        return; // a held pin ignores it, as the chip does
    }
    chip.mode[pin] = mode;
    check();
}

static void op_level(void *ctx, int pin, int level)
{
    (void)ctx;
    if (chip.held[pin]) {
        return;
    }
    chip.level[pin] = level;
    check();
}

static void op_hold(void *ctx, int pin, bool on)
{
    (void)ctx;
    chip.held[pin] = on;
    check();
}

static void op_i2s(void *ctx, bool run)
{
    (void)ctx;
    chip.i2s = run;
    chip.waited_ms = 0;
    check();
}

static void op_delay(void *ctx, int ms)
{
    (void)ctx;
    chip.waited_ms += ms;
}

static const pin_ops_t ops = {op_mode, op_level, op_hold, op_i2s, op_delay, NULL};

// A chip straight out of reset: every pin a floating input.
static void reset(void)
{
    chip = (chip_t){0};
}

static bool led_on(int pin)
{
    return chip.mode[pin] == PIN_OUTPUT && chip.level[pin] == 0;
}

static bool fast(void)
{
    return chip.mode[PIN_CHG_FAST] == PIN_OPEN_DRAIN && chip.level[PIN_CHG_FAST] == 0;
}

static void asleep_is_right(void)
{
    CHECK(!fast());
    CHECK_INT(chip.mode[PIN_CHG_FAST], PIN_INPUT);
    CHECK(!chip.held[PIN_CHG_FAST]);
    CHECK_INT(chip.mode[PIN_STDBY], PIN_INPUT);
    CHECK_INT(chip.mode[PIN_CHRG], PIN_INPUT);
    CHECK(!chip.i2s);
    static const int low[] = {PIN_MIC_PWR, PIN_I2S_SCK, PIN_I2S_WS}, high[] = {PIN_LED_R, PIN_LED_G, PIN_LED_B};
    for (int i = 0; i < 3; i++) {
        CHECK_INT(chip.mode[low[i]], PIN_OUTPUT);
        CHECK_INT(chip.level[low[i]], 0);
        CHECK(chip.held[low[i]]);
        CHECK_INT(chip.mode[high[i]], PIN_OUTPUT);
        CHECK_INT(chip.level[high[i]], 1);
        CHECK(chip.held[high[i]]);
    }
}

static void after_reset_everything_is_off(void)
{
    reset();
    pins_awake(&ops);
    CHECK(!chip.broken);
    CHECK(!fast());
    CHECK(!led_on(PIN_LED_R) && !led_on(PIN_LED_G) && !led_on(PIN_LED_B));
    CHECK_INT(chip.mode[PIN_MIC_PWR], PIN_OUTPUT);
    CHECK_INT(chip.level[PIN_MIC_PWR], 0);
    CHECK_INT(chip.mode[PIN_STDBY], PIN_INPUT);
    for (int pin = 0; pin < PINS; pin++) {
        CHECK(!chip.held[pin]);
    }
    // the light's pins were never low on the way: no flash at boot
    CHECK_STR(chip.why, "");
}

static void the_light_shows_each_colour(void)
{
    reset();
    pins_awake(&ops);
    pins_led(&ops, CLIP_RED);
    CHECK(led_on(PIN_LED_R) && !led_on(PIN_LED_G) && !led_on(PIN_LED_B));
    pins_led(&ops, CLIP_AMBER);
    CHECK(led_on(PIN_LED_R) && led_on(PIN_LED_G) && !led_on(PIN_LED_B));
    pins_led(&ops, CLIP_BLUE);
    CHECK(!led_on(PIN_LED_R) && !led_on(PIN_LED_G) && led_on(PIN_LED_B));
    pins_led(&ops, 0);
    CHECK(!led_on(PIN_LED_R) && !led_on(PIN_LED_G) && !led_on(PIN_LED_B));
    CHECK(!chip.broken);
}

static void fast_charge_is_open_drain_only(void)
{
    reset();
    pins_awake(&ops);
    pins_charge_fast(&ops, true);
    CHECK(fast());
    pins_charge_fast(&ops, true);
    CHECK(fast());
    pins_charge_fast(&ops, false);
    CHECK_INT(chip.mode[PIN_CHG_FAST], PIN_INPUT);
    // a level left high by something else does not get driven when it is turned on
    chip.level[PIN_CHG_FAST] = 1;
    pins_charge_fast(&ops, true);
    CHECK(fast());
    CHECK(!chip.broken);
    CHECK_STR(chip.why, "");
}

static void the_stdby_pull_up_follows_usb(void)
{
    reset();
    pins_awake(&ops);
    pins_usb(&ops, true);
    CHECK_INT(chip.mode[PIN_STDBY], PIN_INPUT_PULLUP);
    CHECK_INT(chip.mode[PIN_CHRG], PIN_INPUT);
    pins_usb(&ops, false);
    CHECK_INT(chip.mode[PIN_STDBY], PIN_INPUT);
    CHECK(!chip.broken);
}

static void the_mic_gets_power_before_clocks_and_loses_clocks_first(void)
{
    reset();
    pins_awake(&ops);
    pins_mic(&ops, true);
    CHECK(chip.i2s);
    CHECK_INT(chip.level[PIN_MIC_PWR], 1);
    CHECK(chip.waited_ms >= 20); // the ICS-43434's wake-up
    pins_mic(&ops, false);
    CHECK(!chip.i2s);
    CHECK_INT(chip.level[PIN_MIC_PWR], 0);
    CHECK_INT(chip.mode[PIN_I2S_SCK], PIN_OUTPUT);
    CHECK_INT(chip.level[PIN_I2S_SCK], 0);
    CHECK_INT(chip.level[PIN_I2S_WS], 0);
    pins_mic(&ops, false); // twice is harmless
    CHECK(!chip.broken);
    CHECK_STR(chip.why, "");
}

static void sleep_from_any_state_and_wake_again(void)
{
    // every mix of light, charger, USB and mic, then sleep, then wake
    for (int state = 0; state < 64; state++) {
        reset();
        pins_awake(&ops);
        pins_usb(&ops, state & 1);
        pins_charge_fast(&ops, state & 2);
        pins_mic(&ops, state & 4);
        pins_led(&ops, (uint8_t)(state >> 3));
        pins_sleep(&ops);
        asleep_is_right();
        // the wake: the holds are still on until pins_awake lets them go
        pins_awake(&ops);
        CHECK(!fast());
        CHECK(!led_on(PIN_LED_R) && !led_on(PIN_LED_G) && !led_on(PIN_LED_B));
        CHECK_INT(chip.level[PIN_MIC_PWR], 0);
        for (int pin = 0; pin < PINS; pin++) {
            CHECK(!chip.held[pin]);
        }
        // and the pins work again
        pins_led(&ops, CLIP_GREEN);
        CHECK(led_on(PIN_LED_G));
        pins_mic(&ops, true);
        CHECK(chip.i2s);
        if (chip.broken) {
            fprintf(stderr, "state %d: %s\n", state, chip.why);
        }
        CHECK(!chip.broken);
    }
}

// The made-up chip itself must catch a broken rule, or the tests above prove nothing.
static void the_checker_catches_each_rule(void)
{
    reset();
    pins_awake(&ops);
    op_mode(NULL, PIN_CHRG, PIN_INPUT_PULLUP);
    CHECK(chip.broken);
    reset();
    pins_awake(&ops);
    op_mode(NULL, PIN_CHG_FAST, PIN_OUTPUT);
    CHECK(chip.broken);
    reset();
    pins_awake(&ops);
    op_level(NULL, PIN_CHG_FAST, 1);
    op_mode(NULL, PIN_CHG_FAST, PIN_OPEN_DRAIN);
    CHECK(chip.broken);
    reset();
    pins_awake(&ops);
    op_i2s(NULL, true);
    CHECK(chip.broken);
    reset();
    pins_awake(&ops);
    pins_mic(&ops, true);
    op_level(NULL, PIN_MIC_PWR, 0); // power off with the clocks running
    CHECK(chip.broken);
    reset();
    pins_awake(&ops);
    op_level(NULL, PIN_I2S_WS, 1);
    CHECK(chip.broken);
}

int main(void)
{
    RUN(after_reset_everything_is_off);
    RUN(the_light_shows_each_colour);
    RUN(fast_charge_is_open_drain_only);
    RUN(the_stdby_pull_up_follows_usb);
    RUN(the_mic_gets_power_before_clocks_and_loses_clocks_first);
    RUN(sleep_from_any_state_and_wake_again);
    RUN(the_checker_catches_each_rule);
    return unit_done(__FILE__);
}
