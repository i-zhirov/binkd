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

#include <limits.h>

#include "sys.h"
#include "tools.h"
#include "readcfg.h"
#include "protoco2.h"
#include "srif.h"

/* common.h: the logger tests the console mode. */
int inetd_flag = 0;

/* sys.h: PID() expands to this value without threads. */
int mypid = 0;

/* server.h: the CRAM challenge generator mixes this value in. */
int ext_rand = 0;

/*
 * common.h: the free space of a directory. The tests assume enough space
 * for the `minfree` cases, and the C code uses `ULONG_MAX` for a failed
 * query.
 */
unsigned long getfree (char *path)
{
  (void) path;
  return ULONG_MAX;
}

/*
 * readcfg.h: the node table resolves an unknown `defnode` node with DNS.
 * The tests run without `defnode`, so the stub never runs.
 */
int get_host_and_port (int n, char *host, char *port, char *src,
                       FTN_ADDR *fa, BINKD_CONFIG *config)
{
  (void) n;
  (void) host;
  (void) port;
  (void) src;
  (void) fa;
  (void) config;
  return 0;
}

/*
 * srv_gai.h: the DNS SRV lookup of `srv_gai.c`. The tests run without
 * `defnode`, so the stub never runs.
 */
int srv_getaddrinfo (const char *node, const char *service,
                     const struct addrinfo *hints, struct addrinfo **res)
{
  (void) node;
  (void) service;
  (void) hints;
  (void) res;
  return EAI_FAIL;
}

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

/* `srif.h`: the tests run without event flags. */
int evt_test (EVTQ **eq, char *filename, EVT_FLAG *evt_flags)
{
  (void) eq;
  (void) filename;
  (void) evt_flags;
  return 0;
}

FTNQ *evt_run (FTNQ *q, char *filename0, int imm_freq,
               STATE *st, BINKD_CONFIG *config)
{
  (void) filename0;
  (void) imm_freq;
  (void) st;
  (void) config;
  return q;
}

