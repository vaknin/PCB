// Time for the Capture client, in whole seconds since 1970 (UTC). Own calendar maths and no libc
// time functions: the chip has no time zone database, and the laptop tests must give the same answer.
#include "capture.h"

#include <stdio.h>

#define DAY_S 86400

static int64_t floor_div(int64_t a, int64_t b)
{
    return a / b - (a % b < 0);
}

// Days since 1970-01-01 of a civil date (Howard Hinnant's days_from_civil).
static int64_t days_from_civil(int64_t y, int m, int d)
{
    y -= m <= 2;
    int64_t era = floor_div(y, 400);
    int64_t yoe = y - era * 400;
    int64_t doy = (153 * (m + (m > 2 ? -3 : 9)) + 2) / 5 + d - 1;
    int64_t doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146097 + doe - 719468;
}

static void civil_from_days(int64_t z, int64_t *y, int *m, int *d)
{
    z += 719468;
    int64_t era = floor_div(z, 146097);
    int64_t doe = z - era * 146097;
    int64_t yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    int64_t doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    int64_t mp = (5 * doy + 2) / 153;
    *d = (int)(doy - (153 * mp + 2) / 5 + 1);
    *m = (int)(mp < 10 ? mp + 3 : mp - 9);
    *y = yoe + era * 400 + (*m <= 2);
}

void cap_iso_format(int64_t epoch_s, char out[CAP_ISO_LEN])
{
    int64_t days = floor_div(epoch_s, DAY_S);
    int second = (int)(epoch_s - days * DAY_S);
    int64_t y;
    int m, d;
    civil_from_days(days, &y, &m, &d);
    snprintf(out, CAP_ISO_LEN, "%04d-%02d-%02dT%02d:%02d:%02dZ", (int)y, m, d, second / 3600, second / 60 % 60,
             second % 60);
}

// n decimal digits at *p, which moves past them; -1 when they are not all digits.
static int digits(const char **p, int n)
{
    int v = 0;
    for (int i = 0; i < n; i++) {
        char c = (*p)[i];
        if (c < '0' || c > '9') {
            return -1;
        }
        v = v * 10 + (c - '0');
    }
    *p += n;
    return v;
}

static bool literal(const char **p, char c)
{
    if (**p != c) {
        return false;
    }
    (*p)++;
    return true;
}

bool cap_iso_parse(const char *text, int64_t *epoch_s)
{
    static const int month_days[] = {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};
    const char *p = text;
    int y = digits(&p, 4);
    if (y < 0 || !literal(&p, '-')) {
        return false;
    }
    int m = digits(&p, 2);
    if (m < 1 || m > 12 || !literal(&p, '-')) {
        return false;
    }
    int d = digits(&p, 2);
    bool leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    if (d < 1 || d > month_days[m - 1] + (m == 2 && leap) || (*p != 'T' && *p != 't')) {
        return false;
    }
    p++;
    int hour = digits(&p, 2);
    if (hour < 0 || hour > 23 || !literal(&p, ':')) {
        return false;
    }
    int minute = digits(&p, 2);
    if (minute < 0 || minute > 59) {
        return false;
    }
    int second = 0;
    if (literal(&p, ':')) {
        second = digits(&p, 2);
        if (second < 0 || second > 59) {
            return false;
        }
        if (literal(&p, '.')) {
            int n = 0;
            while (*p >= '0' && *p <= '9') {
                p++;
                n++;
            }
            if (n > 9) {
                return false;
            }
        }
    }
    int64_t offset = 0;
    if (*p == 'Z' || *p == 'z') {
        p++;
    } else if (*p == '+' || *p == '-') {
        int sign = *p++ == '-' ? -1 : 1;
        int oh = digits(&p, 2);
        if (oh < 0 || oh > 18 || !literal(&p, ':')) {
            return false;
        }
        int om = digits(&p, 2);
        int os = 0;
        if (om < 0 || om > 59) {
            return false;
        }
        if (literal(&p, ':')) {
            os = digits(&p, 2);
            if (os < 0 || os > 59) {
                return false;
            }
        }
        if (oh == 18 && (om || os)) { // ZoneOffset ends at 18:00
            return false;
        }
        offset = sign * (oh * 3600 + om * 60 + os);
    } else {
        return false;
    }
    if (*p) {
        return false;
    }
    *epoch_s = days_from_civil(y, m, d) * DAY_S + hour * 3600 + minute * 60 + second - offset;
    return true;
}

// The nth Sunday of a month, as days since 1970.
static int64_t nth_sunday(int64_t year, int month, int nth)
{
    int64_t first = days_from_civil(year, month, 1);
    int weekday = (int)((first % 7 + 11) % 7); // 0 is Sunday; 1970-01-01 was a Thursday
    return first + (7 - weekday) % 7 + 7 * (nth - 1);
}

// Pacific time's offset from UTC, by the US rule since 2007: daylight time from 02:00 on the
// second Sunday of March (10:00 UTC) to 02:00 on the first Sunday of November (09:00 UTC).
static int64_t pacific_offset_s(int64_t utc_s)
{
    int64_t year;
    int m, d;
    civil_from_days(floor_div(utc_s, DAY_S), &year, &m, &d);
    int64_t start = nth_sunday(year, 3, 2) * DAY_S + 10 * 3600;
    int64_t end = nth_sunday(year, 11, 1) * DAY_S + 9 * 3600;
    return utc_s >= start && utc_s < end ? -7 * 3600 : -8 * 3600;
}

int64_t cap_until_quota_reset_ms(int64_t now_ms)
{
    int64_t now_s = floor_div(now_ms, 1000);
    int64_t tomorrow = floor_div(now_s + pacific_offset_s(now_s), DAY_S) + 1;
    // Midnight is never inside a clock change (those are at 02:00), and 08:00 UTC of that day is
    // on the same side of one as its Pacific midnight (07:00 or 08:00 UTC).
    int64_t reset_s = tomorrow * DAY_S - pacific_offset_s(tomorrow * DAY_S + 8 * 3600);
    return reset_s * 1000 - now_ms + 60000;
}
