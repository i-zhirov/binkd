/*
 *  test_md5.c -- invariant tests for md5b.c
 *
 *  test_md5.c is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 *
 *  The value cases live in tests/golden/cram.tsv.
 *  This file holds the cases that a table cannot express.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "sys.h"
#include "iphdr.h"
#include "protoco2.h"
#include "md5b.h"
#include "harness.h"

static void test_digest (void)
{
  unsigned char challenge[MD_CHALLENGE_LEN + 1];

  memset (challenge, 0, sizeof (challenge));

  /* NULL arguments give NULL. */
  CHECK_NULL (MD_buildDigest (NULL, challenge));
  CHECK_NULL (MD_buildDigest ((char *) "pw", NULL));
}

static void test_get_challenge (void)
{
  unsigned char *ch;
  STATE state;

  memset (&state, 0, sizeof (state));

  /* A new challenge has the fixed length. */
  ch = MD_getChallenge (NULL, &state);
  CHECK_NOT_NULL (ch);
  if (ch)
  {
    CHECK_INT (ch[0], MD_CHALLENGE_LEN);
    free (ch);
  }
}

int main (void)
{
  test_init ();
  test_digest ();
  test_get_challenge ();
  return test_finish ("test_md5");
}
