/*
 *  test_ftnaddr.c -- unit tests for ftnaddr.c and ftndom.c
 *
 *  test_ftnaddr.c is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 */

#include <stdio.h>
#include <string.h>

#include "sys.h"
#include "tools.h"
#include "iphdr.h"
#include "ftndom.h"
#include "ftnaddr.h"
#include "harness.h"

static FTN_DOMAIN dom_fidonet;
static FTN_DOMAIN dom_othernet;
static FTN_DOMAIN dom_aliasnet;

static void init_domains (void)
{
  memset (&dom_fidonet, 0, sizeof (dom_fidonet));
  strcpy (dom_fidonet.name, "fidonet");
  dom_fidonet.path = "/tmp/fido";
  dom_fidonet.dir = "outb";
  dom_fidonet.z[0] = 2;
  dom_fidonet.z[1] = -1;
  dom_fidonet.next = &dom_othernet;

  memset (&dom_othernet, 0, sizeof (dom_othernet));
  strcpy (dom_othernet.name, "othernet");
  dom_othernet.path = "/tmp/other";
  dom_othernet.dir = "outb";
  dom_othernet.z[0] = 3;
  dom_othernet.z[1] = -1;
  dom_othernet.next = &dom_aliasnet;

  /* aliasnet is a 4D alias of fidonet. */
  memset (&dom_aliasnet, 0, sizeof (dom_aliasnet));
  strcpy (dom_aliasnet.name, "aliasnet");
  dom_aliasnet.path = "/tmp/fido";
  dom_aliasnet.dir = "outb";
  dom_aliasnet.z[0] = -1;
  dom_aliasnet.z[1] = -1;
  dom_aliasnet.alias4 = &dom_fidonet;
  dom_aliasnet.next = NULL;
}

static int parse (const char *s, FTN_ADDR *fa)
{
  char buf[FTN_ADDR_SZ];

  if (!s)
    return parse_ftnaddress (NULL, fa, &dom_fidonet);
  strnzcpy (buf, s, sizeof (buf));
  return parse_ftnaddress (buf, fa, &dom_fidonet);
}

static void test_parse (void)
{
  FTN_ADDR fa;

  /* 5D address. */
  CHECK_INT (parse ("2:5047/13.1@fidonet", &fa), 1);
  CHECK_INT (fa.z, 2);
  CHECK_INT (fa.net, 5047);
  CHECK_INT (fa.node, 13);
  CHECK_INT (fa.p, 1);
  CHECK_STR (fa.domain, "fidonet");

  /* 4D address, the point is zero. */
  CHECK_INT (parse ("2:5047/13", &fa), 1);
  CHECK_INT (fa.z, 2);
  CHECK_INT (fa.net, 5047);
  CHECK_INT (fa.node, 13);
  CHECK_INT (fa.p, 0);
  CHECK_STR (fa.domain, "");

  /* 3D address, the zone is unknown. */
  CHECK_INT (parse ("5047/13", &fa), 1);
  CHECK_INT (fa.z, -1);
  CHECK_INT (fa.net, 5047);
  CHECK_INT (fa.node, 13);
  CHECK_INT (fa.p, 0);

  /* 3D address with a point, the zone is unknown. */
  CHECK_INT (parse ("5047/13.1", &fa), 1);
  CHECK_INT (fa.z, -1);
  CHECK_INT (fa.p, 1);

  /* The domain lookup is case-insensitive. */
  CHECK_INT (parse ("2:5047/13@FIDONET", &fa), 1);
  CHECK_STR (fa.domain, "fidonet");

  /* A 4D alias resolves to the main domain name. */
  CHECK_INT (parse ("2:5047/13@aliasnet", &fa), 1);
  CHECK_STR (fa.domain, "fidonet");

  /* An unknown domain is kept as is. */
  CHECK_INT (parse ("2:5047/13@unknown", &fa), 1);
  CHECK_STR (fa.domain, "unknown");

  /* Zeroes are valid numbers. */
  CHECK_INT (parse ("0:0/0", &fa), 1);
  CHECK_INT (fa.z, 0);
  CHECK_INT (fa.net, 0);
  CHECK_INT (fa.node, 0);

  /* A node-only address parses. */
  CHECK_INT (parse ("5047", &fa), 1);
  CHECK_INT (fa.z, -1);
  CHECK_INT (fa.net, -1);
  CHECK_INT (fa.node, 5047);
  CHECK_INT (fa.p, 0);

  /* Invalid addresses. */
  CHECK_INT (parse ("", &fa), 0);
  CHECK_INT (parse ("@fidonet", &fa), 0);
  CHECK_INT (parse ("2:5047", &fa), 0);
  CHECK_INT (parse ("2:5047/", &fa), 0);
  CHECK_INT (parse ("2:5047/13.", &fa), 0);
  CHECK_INT (parse ("2:5047/13.1.2", &fa), 0);
  CHECK_INT (parse ("x:5047/13", &fa), 0);
  CHECK_INT (parse ("2:x/13", &fa), 0);
  CHECK_INT (parse ("2:5047/13@", &fa), 0);
  CHECK_INT (parse ("2:5047/13@1", &fa), 0);
  CHECK_INT (parse (NULL, &fa), 0);
}

static void test_to_str (void)
{
  FTN_ADDR fa;
  char buf[FTN_ADDR_SZ];

  FA_ZERO (&fa);
  fa.z = 2;
  fa.net = 5047;
  fa.node = 13;
  fa.p = 1;
  strcpy (fa.domain, "fidonet");
  xftnaddress_to_str (buf, &fa, 0);
  CHECK_STR (buf, "2:5047/13.1@fidonet");

  /* force_point adds a zero point. */
  fa.p = 0;
  xftnaddress_to_str (buf, &fa, 1);
  CHECK_STR (buf, "2:5047/13.0@fidonet");
  xftnaddress_to_str (buf, &fa, 0);
  CHECK_STR (buf, "2:5047/13@fidonet");

  /* No zone, no domain. */
  FA_ZERO (&fa);
  fa.net = 5047;
  fa.node = 13;
  fa.p = 0;
  xftnaddress_to_str (buf, &fa, 0);
  CHECK_STR (buf, "5047/13");

  /* No node: the point is still printed. */
  FA_ZERO (&fa);
  fa.z = 2;
  fa.net = 5047;
  fa.p = 0;
  xftnaddress_to_str (buf, &fa, 0);
  CHECK_STR (buf, "2:5047.0");
}

static void test_cmp (void)
{
  FTN_ADDR a, b;

  FA_ZERO (&a);
  FA_ZERO (&b);

  /* NULL pointers sort before defined addresses. */
  CHECK_INT (ftnaddress_cmp (NULL, NULL), 0);
  CHECK_INT (ftnaddress_cmp (NULL, &b), -1);
  CHECK_INT (ftnaddress_cmp (&a, NULL), 1);

  /* Equal addresses. */
  CHECK_INT (ftnaddress_cmp (&a, &b), 0);

  /* The domain compare ignores case. */
  a.z = b.z = 2;
  a.net = b.net = 5047;
  a.node = b.node = 13;
  a.p = b.p = 1;
  strcpy (a.domain, "fidonet");
  strcpy (b.domain, "FIDONET");
  CHECK_INT (ftnaddress_cmp (&a, &b), 0);

  b.domain[0] = 0;
  CHECK (ftnaddress_cmp (&a, &b) > 0);
  strcpy (b.domain, "a");
  CHECK (ftnaddress_cmp (&a, &b) > 0);
  strcpy (b.domain, "fidonet");

  b.z = 3;
  CHECK (ftnaddress_cmp (&a, &b) < 0);
  b.z = 2;
  b.net = 5048;
  CHECK (ftnaddress_cmp (&a, &b) < 0);
  b.net = 5047;
  b.node = 14;
  CHECK (ftnaddress_cmp (&a, &b) < 0);
  b.node = 13;
  b.p = 0;
  CHECK (ftnaddress_cmp (&a, &b) > 0);
}

static void test_is_macros (void)
{
  FTN_ADDR fa;

  FA_ZERO (&fa);
  CHECK_INT (FA_ISNULL (&fa), 1);
  CHECK_INT (is4D (&fa), 0);
  CHECK_INT (is5D (&fa), 0);

  fa.z = 2;
  fa.net = 5047;
  fa.node = 13;
  fa.p = 0;
  CHECK_INT (is4D (&fa), 1);
  CHECK_INT (is5D (&fa), 0);

  strcpy (fa.domain, "fidonet");
  CHECK_INT (is5D (&fa), 1);
}

static void test_exp (void)
{
  FTN_ADDR aka[1];
  FTN_ADDR fa;

  FA_ZERO (&aka[0]);
  aka[0].z = 2;
  aka[0].net = 5047;
  aka[0].node = 13;
  aka[0].p = 0;
  strcpy (aka[0].domain, "fidonet");

  /* The unknown fields come from the main AKA. */
  FA_ZERO (&fa);
  fa.net = 5047;
  fa.node = 14;
  exp_ftnaddress (&fa, aka, 1, &dom_fidonet);
  CHECK_INT (fa.z, 2);
  CHECK_INT (fa.net, 5047);
  CHECK_INT (fa.node, 14);
  CHECK_STR (fa.domain, "fidonet");

  /* A defined domain wins over the main AKA zone. */
  FA_ZERO (&fa);
  strcpy (fa.domain, "othernet");
  fa.net = 5047;
  fa.node = 13;
  exp_ftnaddress (&fa, aka, 1, &dom_fidonet);
  CHECK_INT (fa.z, 3);
  CHECK_STR (fa.domain, "othernet");

  /* The zone is known: the domain comes from the zone table. */
  FA_ZERO (&fa);
  fa.z = 3;
  fa.net = 5047;
  fa.node = 13;
  exp_ftnaddress (&fa, aka, 1, &dom_fidonet);
  CHECK_INT (fa.z, 3);
  CHECK_STR (fa.domain, "othernet");
}

static void test_to_domain (void)
{
  FTN_ADDR fa;
  char buf[BINKD_FQDNLEN];

  FA_ZERO (&fa);
  fa.z = 2;
  fa.net = 5047;
  fa.node = 13;
  fa.p = 1;
  strcpy (fa.domain, "fidonet");
  ftnaddress_to_domain (buf, &fa, &dom_fidonet, "binkp.net");
  CHECK_STR (buf, "p1.f13.n5047.z2.binkp.net");

  fa.p = 0;
  ftnaddress_to_domain (buf, &fa, &dom_fidonet, "binkp.net");
  CHECK_STR (buf, "f13.n5047.z2.binkp.net");
}

static void test_to_filename (void)
{
  FTN_ADDR fa;
  char buf[MAXPATHLEN];

  FA_ZERO (&fa);
  fa.z = 2;
  fa.net = 5047;
  fa.node = 13;
  fa.p = 0;
  strcpy (fa.domain, "fidonet");
  ftnaddress_to_filename_ (buf, &fa, &dom_fidonet);
  CHECK_STR (buf, "/tmp/fido/outb/13b7000d");

  /* A different zone adds the hex zone suffix. */
  fa.z = 3;
  ftnaddress_to_filename_ (buf, &fa, &dom_fidonet);
  CHECK_STR (buf, "/tmp/fido/outb.003/13b7000d");

  /* A point adds the .pnt directory. */
  fa.z = 2;
  fa.p = 1;
  ftnaddress_to_filename_ (buf, &fa, &dom_fidonet);
  CHECK_STR (buf, "/tmp/fido/outb/13b7000d.pnt/00000001");

  /* An unknown domain gives an empty path. */
  FA_ZERO (&fa);
  fa.z = 2;
  fa.net = 5047;
  fa.node = 13;
  strcpy (fa.domain, "unknown");
  ftnaddress_to_filename_ (buf, &fa, &dom_fidonet);
  CHECK_STR (buf, "");
}

static void test_mask (void)
{
  FTN_ADDR fa[1];

  FA_ZERO (&fa[0]);
  fa[0].z = 2;
  fa[0].net = 5047;
  fa[0].node = 13;
  fa[0].p = 0;
  strcpy (fa[0].domain, "fidonet");

  CHECK_INT (ftnamask_cmpm ("2:5047/*", 1, fa), 0);
  CHECK_INT (ftnamask_cmpm ("2:5047/13.0@fidonet", 1, fa), 0);
  CHECK_INT (ftnamask_cmpm ("2:5047/*@fidonet", 1, fa), 0);
  /* The string form always has a point, so the mask needs one too. */
  CHECK_INT (ftnamask_cmpm ("2:5047/13@fidonet", 1, fa), -1);
  CHECK_INT (ftnamask_cmpm ("2:5048/*", 1, fa), -1);
  CHECK_INT (ftnamask_cmpm ("*@fidonet", 1, fa), 0);
  CHECK_INT (ftnamask_cmpm ("*@othernet", 1, fa), -1);
}

static void test_get_domain_info (void)
{
  CHECK_NOT_NULL (get_domain_info ("fidonet", &dom_fidonet));
  CHECK_NOT_NULL (get_domain_info ("FIDONET", &dom_fidonet));
  CHECK_NULL (get_domain_info ("nosuchnet", &dom_fidonet));
  CHECK_STR (get_domain_info ("fidonet", &dom_fidonet)->name, "fidonet");
}

static void test_get_matched_domain (void)
{
  FTN_ADDR aka[1];

  FA_ZERO (&aka[0]);
  aka[0].z = 3;
  aka[0].net = 100;
  aka[0].node = 1;
  aka[0].p = 0;
  strcpy (aka[0].domain, "othernet");

  /* The default zone of a domain. */
  CHECK_STR (get_matched_domain (2, aka, 1, &dom_fidonet), "fidonet");
  CHECK_STR (get_matched_domain (3, aka, 1, &dom_fidonet), "othernet");

  /* No match: the main AKA domain is the guess. */
  CHECK_STR (get_matched_domain (9, aka, 1, &dom_fidonet), "othernet");

  /* No AKA: the first domain is the guess. */
  CHECK_STR (get_matched_domain (9, NULL, 0, &dom_fidonet), "fidonet");
}

int main (void)
{
  test_init ();
  init_domains ();
  test_parse ();
  test_to_str ();
  test_cmp ();
  test_is_macros ();
  test_exp ();
  test_to_domain ();
  test_to_filename ();
  test_mask ();
  test_get_domain_info ();
  test_get_matched_domain ();
  return test_finish ("test_ftnaddr");
}
