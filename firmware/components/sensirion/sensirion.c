#include "sensirion.h"

uint8_t sensirion_crc8(const uint8_t *data, size_t len)
{
    uint8_t crc = 0xFF;
    for (size_t i = 0; i < len; i++) {
        crc ^= data[i];
        for (int b = 0; b < 8; b++) {
            crc = crc & 0x80 ? (uint8_t)(crc << 1) ^ 0x31 : (uint8_t)(crc << 1);
        }
    }
    return crc;
}

bool sht4x_convert(const uint8_t r[6], float *temp_c, float *rh)
{
    if (sensirion_crc8(r, 2) != r[2] || sensirion_crc8(r + 3, 2) != r[5]) {
        return false;
    }
    *temp_c = -45 + 175 * ((r[0] << 8) | r[1]) / 65535.0f;
    *rh = -6 + 125 * ((r[3] << 8) | r[4]) / 65535.0f;
    return true;
}
