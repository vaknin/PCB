// SOURCES: firmware/components/sensirion/sensirion.c
#include <math.h>
#include "sensirion.h"
#include "unit.h"

// a 6-byte SHT4x answer for raw words t and rh, with correct CRCs
static void answer(uint8_t r[6], uint16_t t, uint16_t rh)
{
    r[0] = t >> 8, r[1] = t & 0xFF, r[2] = sensirion_crc8(r, 2);
    r[3] = rh >> 8, r[4] = rh & 0xFF, r[5] = sensirion_crc8(r + 3, 2);
}

static void crc_datasheet_example(void)
{
    CHECK_INT(sensirion_crc8((const uint8_t[]){0xBE, 0xEF}, 2), 0x92); // SHT4x datasheet §4.4
    CHECK_INT(sensirion_crc8(NULL, 0), 0xFF);
}

static void converts(void)
{
    uint8_t r[6];
    float t = 0, rh = 0;
    answer(r, 0x6666, 0x8000); // 0.4 and ~0.5 of full scale
    CHECK(sht4x_convert(r, &t, &rh));
    CHECK(fabsf(t - 25.0f) < 0.01f);
    CHECK(fabsf(rh - 56.5f) < 0.01f);
    answer(r, 0x0000, 0xFFFF); // the ends of the range
    CHECK(sht4x_convert(r, &t, &rh));
    CHECK(fabsf(t + 45.0f) < 0.001f);
    CHECK(fabsf(rh - 119.0f) < 0.001f);
}

static void rejects_bad_crc(void)
{
    uint8_t r[6];
    float t = 1, rh = 2;
    answer(r, 0x6666, 0x8000);
    r[2] ^= 1;
    CHECK(!sht4x_convert(r, &t, &rh));
    answer(r, 0x6666, 0x8000);
    r[4] ^= 0x80; // a flipped data bit, CRC unchanged
    CHECK(!sht4x_convert(r, &t, &rh));
    CHECK(t == 1 && rh == 2); // outputs untouched
}

int main(void)
{
    RUN(crc_datasheet_example);
    RUN(converts);
    RUN(rejects_bad_crc);
    return unit_done(__FILE__);
}
