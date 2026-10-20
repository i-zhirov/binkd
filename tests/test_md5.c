/*
 *  test_md5.c -- unit tests for md5b.c
 *
 *  test_md5.c is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "sys.h"
#include "iphdr.h"
#include "protoco2.h"
#include "md5b.h"
#include "harness.h"

/*
 * MD_buildDigest() builds an HMAC-MD5 digest of the CRAM challenge
 * with the password as the key.  The test data comes from RFC 2202.
 */
static void check_digest (const char *pw, const unsigned char *data, int data_len,
                          const char *want)
{
  unsigned char challenge[256];
  char *got;

  CHECK (data_len <= 255);
  challenge[0] = (unsigned char) data_len;
  memcpy (challenge + 1, data, data_len);
  got = MD_buildDigest ((char *) pw, challenge);
  CHECK_NOT_NULL (got);
  CHECK_STR (got, want);
  if (got)
    free (got);
}

static void test_hmac (void)
{
  unsigned char data[128];
  int i;

  /* RFC 2202, test case 1. */
  memset (data, 0x0b, 16);
  check_digest ("\x0b\x0b\x0b\x0b\x0b\x0b\x0b\x0b"
                "\x0b\x0b\x0b\x0b\x0b\x0b\x0b\x0b",
                (const unsigned char *) "Hi There", 8,
                "CRAM-MD5-9294727a3638bb1c13f48ef8158bfc9d");

  /* RFC 2202, test case 2. */
  check_digest ("Jefe", (const unsigned char *) "what do ya want for nothing?", 28,
                "CRAM-MD5-750c783e6ab0b503eaa86e310a5db738");

  /* RFC 2202, test case 3. */
  memset (data, 0xdd, 50);
  check_digest ("\xaa\xaa\xaa\xaa\xaa\xaa\xaa\xaa"
                "\xaa\xaa\xaa\xaa\xaa\xaa\xaa\xaa",
                data, 50,
                "CRAM-MD5-56be34521d144c88dbb8c733f0e8b3f6");

  /* RFC 2202, test case 6: a key longer than the block size. */
  for (i = 0; i < 80; ++i)
    data[i] = 0xaa;
  {
    char key[81];
    for (i = 0; i < 80; ++i)
      key[i] = (char) 0xaa;
    key[80] = 0;
    check_digest (key,
                  (const unsigned char *) "Test Using Larger Than Block-Size Key - Hash Key First",
                  54, "CRAM-MD5-6b1ab7fe4bd7bf8f0b62e6ce61b9d0cd");
    check_digest (key,
                  (const unsigned char *) "Test Using Larger Than Block-Size Key and Larger Than One Block-Size Data",
                  73, "CRAM-MD5-6f630fad67cda0ee1fb1f562db3aa53e");
  }

  /* NULL arguments give NULL. */
  CHECK_NULL (MD_buildDigest (NULL, data));
  CHECK_NULL (MD_buildDigest ((char *) "pw", NULL));
}

static void test_to_string (void)
{
  unsigned char digest[MD5_DIGEST_LEN];
  char buf[MD5_DIGEST_LEN * 2 + 10];
  int i;

  for (i = 0; i < MD5_DIGEST_LEN; ++i)
    digest[i] = (unsigned char) i;
  MD_toString (buf, MD5_DIGEST_LEN, digest);
  CHECK_STR (buf, "CRAM-MD5-000102030405060708090a0b0c0d0e0f");

  /* NULL output is ignored. */
  MD_toString (NULL, MD5_DIGEST_LEN, digest);

  /* A shorter digest gives a shorter string. */
  MD_toString (buf, 2, digest);
  CHECK_STR (buf, "CRAM-MD5-0001");
}

static void test_get_challenge (void)
{
  unsigned char *ch;
  STATE state;

  memset (&state, 0, sizeof (state));

  /* The challenge parses a CRAM-MD5 value. */
  ch = MD_getChallenge ("CRAM-MD5-000102030405060708090a0b0c0d0e0f", NULL);
  CHECK_NOT_NULL (ch);
  if (ch)
  {
    CHECK_INT (ch[0], 16);
    CHECK_INT (ch[1], 0x00);
    CHECK_INT (ch[2], 0x01);
    CHECK_INT (ch[16], 0x0f);
    free (ch);
  }

  /* The hex digits can be uppercase. */
  ch = MD_getChallenge ("CRAM-MD5-0A0B", NULL);
  CHECK_NOT_NULL (ch);
  if (ch)
  {
    CHECK_INT (ch[0], 2);
    CHECK_INT (ch[1], 0x0a);
    CHECK_INT (ch[2], 0x0b);
    free (ch);
  }

  /* An odd number of digits: the last digit is cut off. */
  ch = MD_getChallenge ("CRAM-MD5-012", NULL);
  CHECK_NOT_NULL (ch);
  if (ch)
  {
    CHECK_INT (ch[0], 1);
    CHECK_INT (ch[1], 0x01);
    free (ch);
  }

  /* The CRAM and MD5 words are found anywhere in the string. */
  ch = MD_getChallenge ("xxCRAMyyMD5-42", NULL);
  CHECK_NOT_NULL (ch);
  if (ch)
  {
    CHECK_INT (ch[0], 1);
    CHECK_INT (ch[1], 0x42);
    free (ch);
  }

  /* Invalid values give NULL. */
  CHECK_NULL (MD_getChallenge ("BOGUS", NULL));
  CHECK_NULL (MD_getChallenge ("CRAM-XXX", NULL));
  CHECK_NULL (MD_getChallenge ("CRAM-MD5", NULL));
  CHECK_NULL (MD_getChallenge ("CRAM-MD5-", NULL));

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
  test_hmac ();
  test_to_string ();
  test_get_challenge ();
  return test_finish ("test_md5");
}
