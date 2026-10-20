/*
 *  test_pmatch.c -- unit tests for pmatch.c
 *
 *  test_pmatch.c is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 */

#include <stdio.h>

#include "sys.h"
#include "tools.h"
#include "harness.h"

struct pmatch_case
{
  const char *pattern;
  const char *string;
  int ncase;
  int want;
};

static const struct pmatch_case cases[] =
{
  /* Literal strings. */
  { "abc",   "abc",   0, 1 },
  { "abc",   "abd",   0, 0 },
  { "abc",   "ab",    0, 0 },
  { "abc",   "abcd",  0, 0 },
  { "",      "",      0, 1 },
  { "",      "a",     0, 0 },
  { "abc",   "",      0, 0 },

  /* '?' matches exactly one character. */
  { "a?c",   "abc",   0, 1 },
  { "a?c",   "ac",    0, 0 },
  { "a?c",   "a/c",   0, 1 },
  { "a?c",   "a?c",   0, 1 },
  { "???",   "abc",   0, 1 },

  /* '*' matches zero or more characters. */
  { "*",     "",      0, 1 },
  { "*",     "abc",   0, 1 },
  { "a*c",   "ac",    0, 1 },
  { "a*c",   "abc",   0, 1 },
  { "a*c",   "abbbc", 0, 1 },
  { "a*c",   "abd",   0, 0 },
  { "a*b*c", "aXbYc", 0, 1 },
  { "a*b*c", "aXcYb", 0, 0 },
  { "abc*",  "abc",   0, 1 },
  { "abc*",  "abcd",  0, 1 },
  { "*abc",  "abc",   0, 1 },
  { "*abc",  "xabc",  0, 1 },
  { "*abc",  "abcx",  0, 0 },
  { "*a*",   "bbb",   0, 0 },

  /* Bracket expressions. */
  { "[abc]d",  "bd",    0, 1 },
  { "[abc]d",  "dd",    0, 0 },
  { "[a-z]d",  "md",    0, 1 },
  { "[a-z]d",  "Md",    0, 0 },
  { "[!a-z]d", "md",    0, 0 },
  { "[!a-z]d", "1d",    0, 1 },
  { "[abc",    "[abc",  0, 1 },
  { "[abc",    "abc",   0, 0 },

  /* Bracket ranges ignore the ncase flag. */
  { "[a-z]d", "Md", 1, 0 },

  /* Case-insensitive matching. */
  { "ABC", "abc", 1, 1 },
  { "abc", "ABC", 1, 1 },
  { "ABC", "abc", 0, 0 },
  { "a*C", "abc", 1, 1 },
  { "*.PKT", "12345.pkt", 1, 1 },
  { "*.PKT", "12345.pkt", 0, 0 },

  /* The backslash quotes the next character. */
  { "a\\*c", "a*c", 0, 1 },
  { "a\\*c", "abc", 0, 0 },
  { "a\\?c", "a?c", 0, 1 },
  { "a\\?c", "abc", 0, 0 },
  { "a\\\\c", "a\\c", 0, 1 },

  /* Netname patterns from tools.c. */
  { "*.pkt",   "12345.pkt", 1, 1 },
  { "*.pkt",   "12345.pkt.bad", 1, 0 },
  { "*.req",   "x.req",     1, 1 },
  { "*.?ic",   "x.tic",     1, 1 },
  { "*.?ic",   "x.ticx",    1, 0 },
  { "*.su?",   "abc.su1",   1, 1 },
  { "*.su?",   "abc.sux",   1, 1 },
  { "*.su?",   "abc.pkt",   1, 0 },
};

static void run_cases (void)
{
  size_t i;

  for (i = 0; i < sizeof (cases) / sizeof (cases[0]); ++i)
    CHECK_INT (xpmatch ((char *) cases[i].pattern, (char *) cases[i].string,
                        cases[i].ncase),
               cases[i].want);
}

int main (void)
{
  test_init ();
  run_cases ();
  return test_finish ("test_pmatch");
}
