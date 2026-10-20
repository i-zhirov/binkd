/*
 *  test_getw.c -- unit tests for getw.c
 *
 *  test_getw.c is a part of binkd project
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
#include "getw.h"
#include "harness.h"

static void check_word (const char *src, int n, int flags,
                        const char *fldsep, const char *fldskip,
                        const char *want)
{
  char *got = getwordx2 (src, n, flags, (char *) fldsep, (char *) fldskip);

  if (want == NULL)
    CHECK_NULL (got);
  else
    CHECK_STR (got, want);
  if (got)
    free (got);
}

static void test_words (void)
{
  check_word ("hello world", 1, 0, " ", " ", "hello");
  check_word ("hello world", 2, 0, " ", " ", "world");
  check_word ("hello world", 3, 0, " ", " ", NULL);
  check_word ("  hello   world  ", 2, 0, " ", " ", "world");
  check_word ("a:b:c", 2, 0, ":", " ", "b");
  check_word (NULL, 1, 0, " ", " ", NULL);

  /* Newlines separate words. */
  check_word ("a\nb", 2, 0, DEF_FLDSEP, DEF_FLDSKIP, "b");

  /* GWX_HASH drops the rest of a line after '#'. */
  check_word ("hello # comment", 1, GWX_HASH, " ", " ", "hello");
  check_word ("#comment", 1, GWX_HASH, " ", " ", NULL);
  check_word ("hello # comment", 2, 0, " ", " ", "#");
}

static void test_quoting (void)
{
  check_word ("\"hello world\" tail", 1, 0, " ", " ", "hello world");
  check_word ("a \"b c\" d", 2, 0, " ", " ", "b c");
}

static void test_escapes (void)
{
  /* Backslash escapes. */
  check_word ("a\\tb", 1, 0, " ", " ", "a\tb");
  check_word ("a\\nb", 1, 0, " ", " ", "a\nb");
  check_word ("a\\\\b", 1, 0, " ", " ", "a\\b");
  check_word ("a\\ b", 1, 0, " ", " ", "a b");

  /* Hex escapes. */
  check_word ("\\x41\\x42", 1, 0, " ", " ", "AB");
  check_word ("\\x0a", 1, 0, " ", " ", "\n");

  /* Octal escapes. */
  check_word ("\\101", 1, 0, " ", " ", "A");

  /* GWX_NOESC keeps the backslash. */
  check_word ("a\\tb", 1, GWX_NOESC, " ", " ", "a\\tb");
}

static void test_subst (void)
{
  setenv ("BINKD_TEST_VAR", "value", 1);
  setenv ("BINKD_TEST_LOWER", "lower", 1);

  check_word ("%BINKD_TEST_VAR%", 1, GWX_SUBST, " ", " ", "value");
  check_word ("a%BINKD_TEST_VAR%b", 1, GWX_SUBST, " ", " ", "avalueb");

  /* The variable name is converted to uppercase. */
  check_word ("%binkd_test_lower%", 1, GWX_SUBST, " ", " ", "lower");

  /* An undefined variable gives an empty string. */
  check_word ("a%NO_SUCH_VAR%b", 1, GWX_SUBST, " ", " ", "ab");

  /* '%%' gives a literal percent sign. */
  check_word ("100%%", 1, GWX_SUBST, " ", " ", "100%");

  unsetenv ("BINKD_TEST_VAR");
  unsetenv ("binkd_test_lower");
}

int main (void)
{
  test_init ();
  test_words ();
  test_quoting ();
  test_escapes ();
  test_subst ();
  return test_finish ("test_getw");
}
