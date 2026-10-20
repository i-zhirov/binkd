/*
 *  harness.c -- minimal unit-test harness for the binkd C code
 *
 *  harness.c is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 */

#include "sys.h"
#include "tools.h"
#include "harness.h"

int test_checks = 0;
int test_failures = 0;

void test_init (void)
{
  test_checks = 0;
  test_failures = 0;
  /* loglevel 0, conlog 0: the tests stay silent. */
  InitLog (0, 0, "", NULL);
}

int test_finish (const char *name)
{
  if (test_failures)
  {
    fprintf (stderr, "FAIL: %s: %d of %d checks failed\n",
             name, test_failures, test_checks);
    return 1;
  }
  fprintf (stderr, "ok: %s: %d checks passed\n", name, test_checks);
  return 0;
}
