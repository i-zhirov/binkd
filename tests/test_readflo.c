/*
 *  test_readflo.c -- unit tests for readflo.c
 *
 *  test_readflo.c is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "Config.h"
#include "sys.h"
#include "tools.h"
#include "readflo.h"
#include "harness.h"

static void test_read (void)
{
  FILE *flo;
  char buf[MAXPATHLEN + 1];
  int action;

  flo = tmpfile ();
  CHECK_NOT_NULL (flo);
  if (!flo)
    return;

  fputs ("/tmp/fido/file.pkt\n", flo);
  fputs ("~ignored\n", flo);
  fputs ("\n", flo);
  fputs ("^/tmp/fido/del.pkt\n", flo);
  fputs ("#/tmp/fido/trunc.pkt\n", flo);
  fputs ("/tmp/fido/space.pkt   \n", flo);
  rewind (flo);

  action = -1;
  CHECK_INT (read_flo_line (buf, &action, flo), 1);
  CHECK_STR (buf, "/tmp/fido/file.pkt");
  CHECK_INT (action, 0);

  /* '~' and empty lines are skipped. */
  action = -1;
  CHECK_INT (read_flo_line (buf, &action, flo), 1);
  CHECK_STR (buf, "/tmp/fido/del.pkt");
  CHECK_INT (action, 'd');

  action = -1;
  CHECK_INT (read_flo_line (buf, &action, flo), 1);
  CHECK_STR (buf, "/tmp/fido/trunc.pkt");
  CHECK_INT (action, 't');

  /* Trailing spaces are removed. */
  action = -1;
  CHECK_INT (read_flo_line (buf, &action, flo), 1);
  CHECK_STR (buf, "/tmp/fido/space.pkt");
  CHECK_INT (action, 0);

  /* End of file. */
  CHECK_INT (read_flo_line (buf, &action, flo), 0);

  fclose (flo);
}

static void test_trans (void)
{
  RF_RULE rule;
  char *r;

  /* No rules: NULL. */
  CHECK_NULL (trans_flo_line ("/tmp/fido/file.pkt", NULL));

  rule.next = NULL;
  rule.from = "/old";
  rule.to = "/new";

  r = trans_flo_line ("/old/file.pkt", &rule);
  CHECK_STR (r, "/new/file.pkt");
  free (r);

  /* The match ignores case. */
  r = trans_flo_line ("/OLD/file.pkt", &rule);
  CHECK_STR (r, "/new/file.pkt");
  free (r);

  /* No match: the string stays. */
  r = trans_flo_line ("/other/file.pkt", &rule);
  CHECK_STR (r, "/other/file.pkt");
  free (r);
}

int main (void)
{
  test_init ();
  test_read ();
  test_trans ();
  return test_finish ("test_readflo");
}
