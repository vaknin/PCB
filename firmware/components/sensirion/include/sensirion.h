// Sensirion sensor data (pure C, no ESP-IDF calls, so scripts/fw-test.sh tests it on the laptop).
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// CRC-8 over each 2-byte word Sensirion sensors send: polynomial 0x31, init 0xFF
// (SHT4x datasheet §4.4; its example: CRC(0xBEEF) = 0x92).
uint8_t sensirion_crc8(const uint8_t *data, size_t len);

// An SHT4x measurement answer (6 bytes: T MSB, T LSB, CRC, RH MSB, RH LSB, CRC) in °C and
// %RH (SHT4x datasheet §4.6). False if either CRC is wrong; the outputs are then unchanged.
bool sht4x_convert(const uint8_t r[6], float *temp_c, float *rh);
