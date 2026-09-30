// When the status file is worth a commit: only when something in it is news (clip.h).
#include "clip.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MARK_NAME "status"
#define MARK_HEAD "clip-status 1\n"

static void copy_clean(char *out, size_t size, const char *text)
{
    size_t at = 0;
    for (const char *p = text ? text : ""; *p && at < size - 1; p++) {
        out[at++] = (unsigned char)*p < 0x20 ? ' ' : *p; // one line
    }
    out[at] = 0;
}

void clip_status_mark(const clip_status_t *status, clip_status_mark_t *mark)
{
    *mark = (clip_status_mark_t){
        .percent = clip_battery_percent(status->cell_mv),
        .level = clip_battery_level(CLIP_BATTERY_OK, status->cell_mv),
        .time_s = status->time_s,
    };
    copy_clean(mark->firmware, sizeof mark->firmware, status->firmware);
    copy_clean(mark->error, sizeof mark->error, status->error);
}

bool clip_status_due(const clip_status_mark_t *last, const clip_status_t *status)
{
    clip_status_mark_t now;
    if (!last) {
        return true;
    }
    clip_status_mark(status, &now);
    if (strcmp(now.firmware, last->firmware) != 0 || strcmp(now.error, last->error) != 0) {
        return true;
    }
    if (now.time_s < last->time_s || now.time_s - last->time_s >= CLIP_STATUS_MAX_AGE_S) {
        return true;
    }
    // On USB the cell's voltage is the charger's, and it climbs for hours: only the steps count.
    int moved = now.percent > last->percent ? now.percent - last->percent : last->percent - now.percent;
    if (moved >= CLIP_STATUS_PERCENT_STEP) {
        return true;
    }
    return !status->usb && (now.level == CLIP_BATTERY_OK) != (last->level == CLIP_BATTERY_OK);
}

bool clip_status_mark_save(const clip_store_t *store, const clip_status_mark_t *mark)
{
    char text[96 + sizeof mark->firmware + sizeof mark->error];
    int len = snprintf(text, sizeof text, MARK_HEAD "percent=%d\nlevel=%d\ntime=%lld\nfirmware=%s\nerror=%s\n", mark->percent,
                       (int)mark->level, (long long)mark->time_s, mark->firmware, mark->error);
    // written whole under another name and renamed, like every small file (clip_queue.c)
    return store->write(store->ctx, MARK_NAME ".new", text, (size_t)len) &&
           store->rename(store->ctx, MARK_NAME ".new", MARK_NAME);
}

bool clip_status_mark_load(const clip_store_t *store, clip_status_mark_t *mark)
{
    char *text = NULL;
    size_t len = 0, head = strlen(MARK_HEAD);
    *mark = (clip_status_mark_t){0};
    if (!store->read(store->ctx, MARK_NAME, &text, &len)) {
        return false;
    }
    bool ok = len > head && memcmp(text, MARK_HEAD, head) == 0 && text[len - 1] == '\n';
    int seen = 0;
    for (char *line = ok ? text + head : NULL; line && *line;) {
        char *end = strchr(line, '\n');
        *end = 0;
        if (strncmp(line, "percent=", 8) == 0) {
            mark->percent = atoi(line + 8);
            seen |= 1;
        } else if (strncmp(line, "level=", 6) == 0) {
            int level = atoi(line + 6);
            mark->level = level == CLIP_BATTERY_LOW ? CLIP_BATTERY_LOW : level == CLIP_BATTERY_EMPTY ? CLIP_BATTERY_EMPTY : CLIP_BATTERY_OK;
            seen |= 2;
        } else if (strncmp(line, "time=", 5) == 0) {
            mark->time_s = strtoll(line + 5, NULL, 10);
            seen |= 4;
        } else if (strncmp(line, "firmware=", 9) == 0) {
            copy_clean(mark->firmware, sizeof mark->firmware, line + 9);
            seen |= 8;
        } else if (strncmp(line, "error=", 6) == 0) {
            copy_clean(mark->error, sizeof mark->error, line + 6);
            seen |= 16;
        }
        line = end + 1;
    }
    free(text);
    // a file cut short reads as no report yet: the next one is then written
    if (!ok || seen != 31) {
        *mark = (clip_status_mark_t){0};
        return false;
    }
    return true;
}
