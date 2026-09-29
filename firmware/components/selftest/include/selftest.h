// Self-tests (D-023, D-025): every board runs the tests its board.toml lists
// ([firmware] self_test, BOARD_SELF_TESTS in board_pins.h) at bring-up and in simulation,
// and reports them as JSON lines that devctl and the pcbgen sim stage parse:
//   SELFTEST {"test":"sht40","result":"pass","detail":"23.1 C, 41 %RH"}
//   SELFTEST_DONE {"pass":3,"fail":0,"skip":1,"missing":0}
#pragma once

#include <stddef.h>

typedef enum { SELFTEST_PASS, SELFTEST_FAIL, SELFTEST_SKIP } selftest_result_t;

// A test writes a short human-readable detail (measured value, reason for a skip or failure).
typedef selftest_result_t (*selftest_fn)(char *detail, size_t len);

typedef struct {
    const char *name; // as in board.toml
    selftest_fn run;
} selftest_case_t;

// Runs every name in `wanted` (BOARD_SELF_TESTS) in order, finding each in `cases`. A listed
// test with no implementation counts as missing, which fails the run. Returns the number of
// failed plus missing tests; 0 marks the firmware good (board_mark_good).
int selftest_run(const char *const wanted[], size_t n_wanted, const selftest_case_t cases[], size_t n_cases);
