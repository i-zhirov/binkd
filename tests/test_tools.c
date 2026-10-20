/*
 *  test_tools.c -- unit tests for the pure functions of tools.c
 *
 *  test_tools.c is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

#include "sys.h"
#include "tools.h"
#include "harness.h"

static void test_case (void)
{
  char buf[64];

  strcpy (buf, "AbC");
  CHECK_STR (strlower (buf), "abc");
  strcpy (buf, "AbC");
  CHECK_STR (strupper (buf), "ABC");

  CHECK_INT (o_stricmp ("AbC", "abc"), 0);
  CHECK (o_stricmp ("abc", "abd") < 0);
  CHECK_INT (o_stricmp ("", ""), 0);

  CHECK_INT (o_strnicmp ("abc", "abd", 2), 0);
  CHECK_INT (o_strnicmp ("abc", "abd", 3), -1);
  CHECK_INT (o_strnicmp ("abc", "abd", 0), 0);

  CHECK_INT (o_memicmp ("ABC", "abc", 3), 0);
  CHECK (o_memicmp ("ABC", "abd", 3) < 0);
}

static void test_strnz (void)
{
  char buf[10];

  /* strnzcpy always terminates the string. */
  strnzcpy (buf, "abcdefghij", sizeof (buf));
  CHECK_STR (buf, "abcdefghi");

  strnzcpy (buf, "abc", sizeof (buf));
  CHECK_STR (buf, "abc");

  /* strnzcat appends as much as fits. */
  strnzcat (buf, "de", sizeof (buf));
  CHECK_STR (buf, "abcde");
  strnzcat (buf, "fghij", sizeof (buf));
  CHECK_STR (buf, "abcdefghi");

  /* strnzcat does nothing when no space is left. */
  strnzcat (buf, "x", sizeof (buf));
  CHECK_STR (buf, "abcdefghi");
}

static void test_quote (void)
{
  char *q;

  q = strquote ("plain", 0);
  CHECK_STR (q, "plain");
  free (q);

  /* Control characters. */
  q = strquote ("a\tb", SQ_CNTRL);
  CHECK_STR (q, "a\\x09b");
  free (q);

  /* Spaces. */
  q = strquote ("a b", SQ_SPACE);
  CHECK_STR (q, "a\\x20b");
  free (q);
  q = strquote ("a b", 0);
  CHECK_STR (q, "a b");
  free (q);

  /* The backslash itself is always quoted. */
  q = strquote ("a\\b", 0);
  CHECK_STR (q, "a\\x5cb");
  free (q);
}

static void test_dequote (void)
{
  char *d;

  d = strdequote ("plain");
  CHECK_STR (d, "plain");
  free (d);

  d = strdequote ("a\\x09b");
  CHECK_STR (d, "a\tb");
  free (d);

  /* The short form accepts two hex digits. */
  d = strdequote ("a\\09b");
  CHECK_STR (d, "a\tb");
  free (d);

  /* The short form takes the first two digits of three. */
  d = strdequote ("a\\101b");
  CHECK_STR (d, "a\x10" "1b");
  free (d);

  /* An invalid escape stays as is. */
  d = strdequote ("a\\xzz");
  CHECK_STR (d, "a\\xzz");
  free (d);
}

static void test_wipe (void)
{
  char buf[64];

  strcpy (buf, "a/b:c\\d e");
  CHECK_STR (strwipe (buf), "a_b_c_d e");

  /* Control characters are wiped. */
  strcpy (buf, "a\tb");
  CHECK_STR (strwipe (buf), "a_b");
}

static void test_ed (void)
{
  char *r;
  char s[] = "abc";
  char buf[8];
  size_t sz;

  /* Replace one character. */
  r = ed ("abc", "b", "X", NULL);
  CHECK_STR (r, "aXc");
  free (r);

  /* The match ignores case. */
  r = ed ("aBc", "b", "X", NULL);
  CHECK_STR (r, "aXc");
  free (r);

  /* Replace with a longer string. */
  r = ed ("a.b", ".", "................", NULL);
  CHECK_STR (r, "a................b");
  free (r);

  /* No match: the string stays. */
  r = ed ("abc", "z", "X", NULL);
  CHECK_STR (r, "abc");
  free (r);

  /* An empty pattern returns the source pointer. */
  CHECK (ed (s, "", "x", NULL) == s);

  /* NULL source gives NULL. */
  CHECK_NULL (ed (NULL, "a", "b", NULL));

  /* The size form uses the caller's buffer. */
  strcpy (buf, "abc");
  sz = sizeof (buf);
  r = ed (buf, "b", "XYZ", &sz);
  CHECK_STR (r, "aXYZc");
  if (r != buf)
    free (r);
}

static void test_parse_args (void)
{
  char args[] = "cmd arg1 arg2";
  char args2[] = "cmd arg1";
  char args3[] = "  cmd\targ1  arg2  ";
  char *argv[3];

  CHECK_NOT_NULL (parse_args (3, argv, args, "TEST"));
  CHECK_STR (argv[0], "cmd");
  CHECK_STR (argv[1], "arg1");
  CHECK_STR (argv[2], "arg2");

  /* Too few words: an error. */
  CHECK_NULL (parse_args (3, argv, args2, "TEST"));

  /* Leading and repeated separators are skipped. */
  CHECK_NOT_NULL (parse_args (3, argv, args3, "TEST"));
  CHECK_STR (argv[0], "cmd");
  CHECK_STR (argv[1], "arg1");
  CHECK_STR (argv[2], "arg2");
}

static void test_print_args (void)
{
  char buf[64];
  char *argv[] = { "cmd", "a b", "", NULL };

  print_args (buf, sizeof (buf), argv);
  CHECK_STR (buf, " cmd \"a b\" \"\"");
}

static void test_mkargv (void)
{
  char *argv[] = { "a", "b", NULL };
  char **p;

  p = mkargv (2, argv);
  CHECK_NOT_NULL (p);
  CHECK_STR (p[0], "a");
  CHECK_STR (p[1], "b");
  CHECK_NULL (p[2]);
  free (p[0]);
  free (p[1]);
  free (p);

  /* A negative count means a NULL-terminated vector. */
  p = mkargv (-1, argv);
  CHECK_NOT_NULL (p);
  CHECK_STR (p[0], "a");
  CHECK_STR (p[1], "b");
  CHECK_NULL (p[2]);
  free (p[0]);
  free (p[1]);
  free (p);

  CHECK_NULL (mkargv (2, NULL));
}

static void test_safe_atol (void)
{
  char *msg;

  msg = NULL;
  CHECK_INT (safe_atol ("123", &msg), 123);
  CHECK_NULL (msg);

  msg = NULL;
  CHECK_INT (safe_atol ("-5", &msg), -5);
  CHECK_NULL (msg);

  msg = NULL;
  CHECK_INT (safe_atol ("0", &msg), 0);
  CHECK_NULL (msg);

  /* NULL input: an error. */
  msg = NULL;
  CHECK_INT (safe_atol (NULL, &msg), 0);
  CHECK_NOT_NULL (msg);

  /* A value out of range: an error. */
  msg = NULL;
  safe_atol ("99999999999999999999999999", &msg);
  CHECK_NOT_NULL (msg);
}

static void test_paths (void)
{
  CHECK_STR (last_slash ("/a/b/c"), "/c");
  CHECK_NULL (last_slash ("abc"));
  CHECK_STR (last_slash ("a\\b\\c"), "\\c");

  CHECK_STR (extract_filename ("/a/b/c"), "c");
  CHECK_STR (extract_filename ("abc"), "abc");
  CHECK_STR (extract_filename ("c:\\dir\\file.txt"), "file.txt");
  /* The colon separator wins over the slashes. */
  CHECK_STR (extract_filename ("c:file"), "file");
}

static void test_netnames (void)
{
  CHECK_INT (ispkt ("12345.pkt"), 1);
  CHECK_INT (ispkt ("12345.pkt.bad"), 0);
  CHECK_INT (ispkt ("12345.req"), 0);

  CHECK_INT (isreq ("12345.req"), 1);
  CHECK_INT (isreq ("12345.pkt"), 0);

  CHECK_INT (istic ("12345.tic"), 1);
  CHECK_INT (istic ("12345.ticx"), 0);
  CHECK_INT (istic ("12345.pkt"), 0);

  CHECK_INT (isarcmail ("12345.su1"), 1);
  CHECK_INT (isarcmail ("12345.mo0"), 1);
  CHECK_INT (isarcmail ("12345.th9"), 1);
  CHECK_INT (isarcmail ("12345.pkt"), 0);
}

static void test_inbound_case (void)
{
  char buf[64];

  strcpy (buf, "MiXeD.txt");
  CHECK_STR (makeinboundcase (buf, INB_UPPER), "MIXED.TXT");

  strcpy (buf, "MiXeD.txt");
  CHECK_STR (makeinboundcase (buf, INB_LOWER), "mixed.txt");

  strcpy (buf, "MiXeD.txt");
  CHECK_STR (makeinboundcase (buf, INB_MIXED), "Mixed.Txt");

  strcpy (buf, "MiXeD.txt");
  CHECK_STR (makeinboundcase (buf, INB_SAVE), "MiXeD.txt");
}

static void test_tz_off (void)
{
  /* An explicit offset is returned in minutes. */
  CHECK_INT (tz_off (0, 3600), 60);
  CHECK_INT (tz_off (0, -3600), -60);
  CHECK_INT (tz_off (0, 0), 0);
}

static void init_pkt (unsigned char *raw, int version, int type2plus)
{
  memset (raw, 0, 64);
  raw[18] = (unsigned char) version;
  if (type2plus)
  {
    raw[40] = 1;
    raw[41] = 1;
    raw[44] = 1;
    raw[45] = 1;
  }
}

static void test_pkt_addr (void)
{
  unsigned char raw[64];
  short oz, onet, onode, op, dz, dnet, dnode, dp;

  /* FTS-1 type-2 packet: the net and node fields only. */
  init_pkt (raw, 2, 0);
  CHECK_INT (pkt_setaddr (raw, 2, 5047, 13, -1, -1, -1, -1, -1), 1);
  CHECK_INT (pkt_getaddr (raw, &oz, &onet, &onode, &op, &dz, &dnet, &dnode, &dp), 1);
  CHECK_INT (oz, -1);
  CHECK_INT (onet, 5047);
  CHECK_INT (onode, 13);
  CHECK_INT (op, -1);
  CHECK_INT (dz, -1);
  CHECK_INT (dnet, 0);
  CHECK_INT (dnode, 0);
  CHECK_INT (dp, -1);

  /* qmail packets keep the zone at other offsets. */
  init_pkt (raw, 2, 0);
  raw[24] = 0x29;
  CHECK_INT (pkt_setaddr (raw, 2, 5047, 13, -1, 3, -1, -1, -1), 1);
  CHECK_INT (pkt_getaddr (raw, &oz, &onet, &onode, &op, &dz, &dnet, &dnode, &dp), 1);
  CHECK_INT (oz, 2);
  CHECK_INT (dz, 3);
  CHECK_INT (onet, 5047);
  CHECK_INT (onode, 13);

  /* Type-2 packets are point aware. */
  init_pkt (raw, 2, 0);
  raw[44] = 1;
  CHECK_INT (pkt_setaddr (raw, 2, 5047, 13, 1, 3, 5048, 14, 2), 1);
  CHECK_INT (pkt_getaddr (raw, &oz, &onet, &onode, &op, &dz, &dnet, &dnode, &dp), 1);
  CHECK_INT (oz, 2);
  CHECK_INT (onet, 5047);
  CHECK_INT (onode, 13);
  CHECK_INT (op, 1);
  CHECK_INT (dz, 3);
  CHECK_INT (dnet, 5048);
  CHECK_INT (dnode, 14);
  CHECK_INT (dp, 2);

  /* Type 2+ packets move the net field for the point. */
  init_pkt (raw, 2, 1);
  CHECK_INT (pkt_setaddr (raw, 2, 5047, 13, 1, 3, 5048, 14, 2), 1);
  CHECK_INT (pkt_getaddr (raw, &oz, &onet, &onode, &op, &dz, &dnet, &dnode, &dp), 1);
  CHECK_INT (onet, 5047);
  CHECK_INT (op, 1);
  CHECK_INT (dp, 2);

  /* A packet of another version is refused. */
  init_pkt (raw, 1, 0);
  CHECK_INT (pkt_setaddr (raw, 2, 5047, 13, -1, -1, -1, -1, -1), 0);
  CHECK_INT (pkt_getaddr (raw, &oz, &onet, &onode, &op, &dz, &dnet, &dnode, &dp), 0);
  CHECK_INT (oz, -1);
  CHECK_INT (onet, -1);
  CHECK_INT (onode, -1);
  CHECK_INT (op, -1);
  CHECK_INT (dz, -1);
  CHECK_INT (dnet, -1);
  CHECK_INT (dnode, -1);
  CHECK_INT (dp, -1);
}

int main (void)
{
  test_init ();
  test_case ();
  test_strnz ();
  test_quote ();
  test_dequote ();
  test_wipe ();
  test_ed ();
  test_parse_args ();
  test_print_args ();
  test_mkargv ();
  test_safe_atol ();
  test_paths ();
  test_netnames ();
  test_inbound_case ();
  test_tz_off ();
  test_pkt_addr ();
  return test_finish ("test_tools");
}
