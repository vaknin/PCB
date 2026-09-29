// A tiny test runner for firmware logic on the laptop (layer 1, D-025; scripts/fw-test.sh).
// One binary per test file; its main() calls RUN() per test and returns unit_done(), which
// prints the line fw-test.sh reads:  UNIT {"file":"test_x.c","pass":12,"fail":0}
#pragma once

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static int unit_pass, unit_fail, unit_test_failed;

#define CHECK(cond)                                                                                      \
    do {                                                                                                 \
        if (!(cond)) {                                                                                   \
            fprintf(stderr, "%s:%d: CHECK(%s) failed\n", __FILE__, __LINE__, #cond);                     \
            unit_test_failed = 1;                                                                        \
        }                                                                                                \
    } while (0)

#define CHECK_INT(a, b)                                                                                  \
    do {                                                                                                 \
        long long unit_a = (long long)(a), unit_b = (long long)(b);                                      \
        if (unit_a != unit_b) {                                                                          \
            fprintf(stderr, "%s:%d: %s is %lld, expected %lld\n", __FILE__, __LINE__, #a, unit_a, unit_b); \
            unit_test_failed = 1;                                                                        \
        }                                                                                                \
    } while (0)

#define CHECK_STR(a, b)                                                                                  \
    do {                                                                                                 \
        const char *unit_a = (a), *unit_b = (b);                                                         \
        if (!unit_a || strcmp(unit_a, unit_b) != 0) {                                                    \
            fprintf(stderr, "%s:%d: %s is \"%s\", expected \"%s\"\n", __FILE__, __LINE__, #a,           \
                    unit_a ? unit_a : "(null)", unit_b);                                                 \
            unit_test_failed = 1;                                                                        \
        }                                                                                                \
    } while (0)

#define RUN(fn)                                                                                          \
    do {                                                                                                 \
        unit_test_failed = 0;                                                                            \
        fn();                                                                                            \
        if (unit_test_failed) {                                                                          \
            unit_fail++;                                                                                 \
            fprintf(stderr, "FAIL %s\n", #fn);                                                           \
        } else {                                                                                         \
            unit_pass++;                                                                                 \
        }                                                                                                \
    } while (0)

static inline int unit_done(const char *file)
{
    const char *base = strrchr(file, '/');
    printf("UNIT {\"file\":\"%s\",\"pass\":%d,\"fail\":%d}\n", base ? base + 1 : file, unit_pass, unit_fail);
    fflush(stdout);
    return unit_fail != 0;
}

// Captures what the code under test prints to stdout, e.g. the SELFTEST lines.
static int unit_saved_fd = -1;
static FILE *unit_capture_file;

static inline void unit_capture_start(void)
{
    fflush(stdout);
    unit_capture_file = tmpfile();
    unit_saved_fd = dup(STDOUT_FILENO);
    dup2(fileno(unit_capture_file), STDOUT_FILENO);
}

// The captured text (malloc'd, NUL-terminated); stdout is restored.
static inline char *unit_capture_end(void)
{
    fflush(stdout);
    dup2(unit_saved_fd, STDOUT_FILENO);
    close(unit_saved_fd);
    long n = ftell(unit_capture_file);
    char *text = calloc(1, (size_t)n + 1);
    rewind(unit_capture_file);
    if (fread(text, 1, (size_t)n, unit_capture_file) != (size_t)n) {
        text[0] = 0;
    }
    fclose(unit_capture_file);
    return text;
}
