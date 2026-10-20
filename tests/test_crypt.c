/*
 *  test_crypt.c -- unit tests for crypt.c
 *
 *  test_crypt.c is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 */

#include <stdio.h>
#include <string.h>

#include "sys.h"
#include "crypt.h"
#include "harness.h"

static void test_crc_table (void)
{
  CHECK_INT (crc_32_tab[0], 0x00000000L);
  CHECK_INT (crc_32_tab[1], 0x77073096L);
  CHECK_INT (crc_32_tab[128], 0xedb88320L);
  CHECK_INT (crc_32_tab[255], 0x2d02ef8dL);
}

static void test_round_trip (void)
{
  static const char *passwords[] = { "", "x", "password", "very long password with spaces" };
  unsigned char orig[512];
  unsigned char buf[512];
  unsigned long keys[3];
  size_t p, i, len;

  for (p = 0; p < sizeof (passwords) / sizeof (passwords[0]); ++p)
  {
    for (len = 0; len <= sizeof (orig); len += 37)
    {
      for (i = 0; i < len; ++i)
        orig[i] = (unsigned char) (i * 7 + p);

      /* encrypt, then decrypt: the data comes back. */
      memcpy (buf, orig, len);
      init_keys (keys, passwords[p]);
      encrypt_buf ((char *) buf, (unsigned int) len, keys);
      init_keys (keys, passwords[p]);
      decrypt_buf ((char *) buf, (unsigned int) len, keys);
      CHECK_INT (memcmp (buf, orig, len), 0);

      /* decrypt, then encrypt: the data comes back. */
      memcpy (buf, orig, len);
      init_keys (keys, passwords[p]);
      decrypt_buf ((char *) buf, (unsigned int) len, keys);
      init_keys (keys, passwords[p]);
      encrypt_buf ((char *) buf, (unsigned int) len, keys);
      CHECK_INT (memcmp (buf, orig, len), 0);
    }
  }
}

static void test_key_stream (void)
{
  unsigned long keys_a[3], keys_b[3];
  unsigned char a[8], b[8];
  int i;

  /* Two runs with the same password give the same key stream. */
  init_keys (keys_a, "secret");
  init_keys (keys_b, "secret");
  for (i = 0; i < 8; ++i)
  {
    a[i] = 0;
    b[i] = 0;
  }
  encrypt_buf ((char *) a, sizeof (a), keys_a);
  encrypt_buf ((char *) b, sizeof (b), keys_b);
  CHECK_INT (memcmp (a, b, sizeof (a)), 0);

  /* A different password gives a different key stream. */
  init_keys (keys_b, "another");
  for (i = 0; i < 8; ++i)
    b[i] = 0;
  encrypt_buf ((char *) b, sizeof (b), keys_b);
  CHECK (memcmp (a, b, sizeof (a)) != 0);
}

int main (void)
{
  test_init ();
  test_crc_table ();
  test_round_trip ();
  test_key_stream ();
  return test_finish ("test_crypt");
}
