// The chip's side of simcmd: its clock (esp_timer).
#include "simcmd.h"

#include "esp_timer.h"

static int64_t uptime_ms(void)
{
    return esp_timer_get_time() / 1000;
}

void simcmd_start(simcmd_button_fn on_button, void *ctx)
{
    simcmd_init(uptime_ms, on_button, ctx);
}
