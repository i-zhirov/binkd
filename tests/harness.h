/*
 *  harness.h -- minimal unit-test harness for the binkd C code
 *
 *  harness.h is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 *
 *  The tests link the real binkd modules against stubs (stubs.c).
 *  They pin the behaviour that the Rust port must reproduce.
 */

#ifndef BINKD_TEST_HARNESS_H
#define BINKD_TEST_HARNESS_H

#include <stdio.h>
#include <string.h>

extern int test_checks;
extern int test_failures;

/*
 * Silence the binkd logger and reset the counters.
 * Each test program calls test_init() first.
 */
void test_init (void);

/*
 * Print the summary. Returns the process exit code: 0 == success.
 */
int test_finish (const char *name);

#define CHECK(cond)                                                     \
  do {                                                                  \
    ++test_checks;                                                      \
    if (!(cond)) {                                                      \
      ++test_failures;                                                  \
      fprintf (stderr, "%s:%d: FAIL: %s\n", __FILE__, __LINE__, #cond); \
    }                                                                   \
  } while (0)

#define CHECK_INT(got, want)                                            \
  do {                                                                  \
    long g_ = (long) (got);                                             \
    long w_ = (long) (want);                                            \
    ++test_checks;                                                      \
    if (g_ != w_) {                                                      \
      ++test_failures;                                                  \
      fprintf (stderr, "%s:%d: FAIL: %s == %ld, want %ld\n",             \
               __FILE__, __LINE__, #got, g_, w_);                         \
    }                                                                   \
  } while (0)

#define CHECK_STR(got, want)                                            \
  do {                                                                  \
    const char *g_ = (got);                                             \
    const char *w_ = (want);                                            \
    ++test_checks;                                                      \
    if (g_ == NULL || strcmp (g_, w_) != 0) {                             \
      ++test_failures;                                                  \
      fprintf (stderr, "%s:%d: FAIL: %s == \"%s\", want \"%s\"\n",         \
               __FILE__, __LINE__, #got, g_ ? g_ : "(null)", w_);         \
    }                                                                   \
  } while (0)

#define CHECK_NULL(got)                                                 \
  do {                                                                  \
    const void *g_ = (got);                                             \
    ++test_checks;                                                      \
    if (g_ != NULL) {                                                     \
      ++test_failures;                                                  \
      fprintf (stderr, "%s:%d: FAIL: %s != NULL\n",                       \
               __FILE__, __LINE__, #got);                                 \
    }                                                                   \
  } while (0)

#define CHECK_NOT_NULL(got)                                             \
  do {                                                                  \
    const void *g_ = (got);                                             \
    ++test_checks;                                                      \
    if (g_ == NULL) {                                                     \
      ++test_failures;                                                  \
      fprintf (stderr, "%s:%d: FAIL: %s == NULL\n",                       \
               __FILE__, __LINE__, #got);                                 \
    }                                                                   \
  } while (0)

#endif
