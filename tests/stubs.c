/*
 *  stubs.c -- stubs for the unit tests
 *
 *  stubs.c is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 *
 *  The unit tests link the real binkd modules against this file.
 *  The stubs replace the parts of binkd that need the full daemon:
 *  the config, the network, and the process managers.
 */

#include "sys.h"
#include "tools.h"
#include "readcfg.h"

/* common.h: the logger tests the console mode. */
int inetd_flag = 0;

/* sys.h: PID() expands to this value without threads. */
int mypid = 0;

/* server.h: the CRAM challenge generator mixes this value in. */
int ext_rand = 0;

/*
 * readcfg.h: the logger filters messages with nolog masks.
 * The tests run without masks.
 */
char *mask_test (char *s, struct maskchain *chain)
{
  (void) s;
  (void) chain;
  return NULL;
}
