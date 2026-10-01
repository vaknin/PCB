// The button's debounce, with the time of each edge (clip.h).
#include "clip.h"

void clip_button_init(clip_button_t *button, bool down)
{
    *button = (clip_button_t){.down = down};
}

bool clip_button_sample(clip_button_t *button, bool down, int64_t now_ms, int64_t *at_ms)
{
    if (down == button->down) {
        button->changing = false; // a bounce, or nothing
        return false;
    }
    if (!button->changing) {
        button->changing = true;
        button->since_ms = now_ms;
    }
    if (now_ms - button->since_ms < CLIP_DEBOUNCE_MS) {
        return false;
    }
    button->down = down;
    button->changing = false;
    *at_ms = button->since_ms;
    return true;
}
