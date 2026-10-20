/*
 *  test_corpus.c -- golden corpus runner for the binkd C code
 *
 *  test_corpus.c is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 *
 *  The runner reads the case files in tests/golden/, calls the C code,
 *  and compares the result with the expected value.  The Rust port reads
 *  the same files, so the corpus is the equivalence oracle.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include <errno.h>

#include "sys.h"
#include "iphdr.h"
#include "tools.h"
#include "getw.h"
#include "ftndom.h"
#include "ftnaddr.h"
#include "crypt.h"
#include "protoco2.h"
#include "md5b.h"
#include "readflo.h"
#include "harness.h"

#define MAX_FIELDS 16
#define MAX_LINE 65536

/* The fixed domain table.  The Rust port uses the same table. */
static FTN_DOMAIN dom_fidonet;
static FTN_DOMAIN dom_othernet;
static FTN_DOMAIN dom_aliasnet;

/* The fixed AKA.  The Rust port uses the same AKA. */
static FTN_ADDR aka[1];

static void init_tables (void)
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
  dom_othernet.idomain = "other.net";
  dom_othernet.next = &dom_aliasnet;

  memset (&dom_aliasnet, 0, sizeof (dom_aliasnet));
  strcpy (dom_aliasnet.name, "aliasnet");
  dom_aliasnet.path = "/tmp/fido";
  dom_aliasnet.dir = "outb";
  dom_aliasnet.z[0] = -1;
  dom_aliasnet.z[1] = -1;
  dom_aliasnet.alias4 = &dom_fidonet;
  dom_aliasnet.next = NULL;

  FA_ZERO (&aka[0]);
  aka[0].z = 2;
  aka[0].net = 5047;
  aka[0].node = 13;
  aka[0].p = 0;
  strcpy (aka[0].domain, "fidonet");

  /* The %VAR% cases of getwordx2 read these values. */
  setenv ("BINKD_TEST_VAR", "value", 1);
  setenv ("BINKD_TEST_LOWER", "lower", 1);
}

static int hexval (int c)
{
  if (c >= '0' && c <= '9')
    return c - '0';
  if (c >= 'a' && c <= 'f')
    return c - 'a' + 10;
  if (c >= 'A' && c <= 'F')
    return c - 'A' + 10;
  return -1;
}

/*
 * Decode a corpus field.  The escapes are:
 *   \\  backslash
 *   \t  tab
 *   \n  newline
 *   \r  carriage return
 *   \xHH  byte HH
 * Returns NULL on an unknown escape.
 */
static char *field_decode (const char *s)
{
  size_t n = strlen (s);
  size_t i = 0, o = 0;
  char *out = malloc (n + 1);

  if (!out)
    return NULL;

  while (i < n)
  {
    if (s[i] != '\\')
    {
      out[o++] = s[i++];
      continue;
    }
    ++i;
    switch (s[i])
    {
      case '\\': out[o++] = '\\'; ++i; break;
      case 't':  out[o++] = '\t'; ++i; break;
      case 'n':  out[o++] = '\n'; ++i; break;
      case 'r':  out[o++] = '\r'; ++i; break;
      case 'x':
      {
        int hi = hexval (s[i + 1]);
        int lo = hi < 0 ? -1 : hexval (s[i + 2]);
        if (lo < 0)
        {
          free (out);
          return NULL;
        }
        out[o++] = (char) (hi * 16 + lo);
        i += 3;
        break;
      }
      default:
        free (out);
        return NULL;
    }
  }
  out[o] = 0;
  return out;
}

/* Encode a string for an error message. */
static char *field_encode (const char *s)
{
  return strquote ((char *) s, SQ_CNTRL | SQ_SPACE);
}

/* Decode a hex string into a buffer of the given size. */
static int hex_decode (const char *s, unsigned char *out, size_t size)
{
  size_t i;

  if (strlen (s) != size * 2)
    return 0;
  for (i = 0; i < size; ++i)
  {
    int hi = hexval (s[i * 2]);
    int lo = hexval (s[i * 2 + 1]);
    if (hi < 0 || lo < 0)
      return 0;
    out[i] = (unsigned char) (hi * 16 + lo);
  }
  return 1;
}

/* Decode the first size bytes of a hex string. */
static int hex_decode_prefix (const char *s, unsigned char *out, size_t size)
{
  size_t i;

  if (strlen (s) < size * 2)
    return 0;
  for (i = 0; i < size; ++i)
  {
    int hi = hexval (s[i * 2]);
    int lo = hexval (s[i * 2 + 1]);
    if (hi < 0 || lo < 0)
      return 0;
    out[i] = (unsigned char) (hi * 16 + lo);
  }
  return 1;
}

static char *hex_encode (const unsigned char *data, size_t size)
{
  static const char digits[] = "0123456789abcdef";
  char *out = malloc (size * 2 + 1);
  size_t i;

  if (!out)
    return NULL;
  for (i = 0; i < size; ++i)
  {
    out[i * 2] = digits[data[i] >> 4];
    out[i * 2 + 1] = digits[data[i] & 0x0f];
  }
  out[size * 2] = 0;
  return out;
}

static int arg_is_null (const char *s)
{
  return strcmp (s, "<null>") == 0;
}

static long arg_int (const char *s)
{
  return strtol (s, NULL, 10);
}

static void addr_zero (FTN_ADDR *fa)
{
  FA_ZERO (fa);
}

static int addr_build (FTN_ADDR *fa, const char *z, const char *net,
                        const char *node, const char *p, const char *domain)
{
  addr_zero (fa);
  fa->z = (int) arg_int (z);
  fa->net = (int) arg_int (net);
  fa->node = (int) arg_int (node);
  fa->p = (int) arg_int (p);
  if (*domain)
    strnzcpy (fa->domain, domain, sizeof (fa->domain));
  return 1;
}

static char *addr_string (FTN_ADDR *fa)
{
  char *out = malloc (FTN_ADDR_SZ);
  char z[32], net[32], node[32], p[32];

  if (!out)
    return NULL;
  sprintf (z, "%d", fa->z);
  sprintf (net, "%d", fa->net);
  sprintf (node, "%d", fa->node);
  sprintf (p, "%d", fa->p);
  sprintf (out, "%s:%s:%s:%s:%s", z, net, node, p, fa->domain);
  return out;
}

/*
 * Run one case.  Returns the malloc'ed result, or NULL on an error.
 * The caller frees the result.
 */
static char *run_case (const char *func, int argc, char **argv)
{
  if (strcmp (func, "pmatch") == 0)
  {
    char *out = malloc (8);
    sprintf (out, "%d", xpmatch (argv[0], argv[1], (int) arg_int (argv[2])));
    return out;
  }
  if (strcmp (func, "getword") == 0)
  {
    char *w = getwordx2 (argv[0], (int) arg_int (argv[1]),
                         (int) arg_int (argv[2]), argv[3], argv[4]);
    return w ? w : xstrdup ("<null>");
  }
  if (strcmp (func, "crypt_keys") == 0)
  {
    unsigned long keys[3];
    char *out = malloc (64);

    init_keys (keys, argv[0]);
    sprintf (out, "%lx %lx %lx", keys[0],
             (unsigned long) (keys[1] & 0xffffffffUL), keys[2]);
    return out;
  }
  if (strcmp (func, "crypt_encrypt") == 0 || strcmp (func, "crypt_decrypt") == 0)
  {
    unsigned long keys[3];
    size_t len = strlen (argv[1]) / 2;
    char *data = malloc (len + 1);
    char *out;

    if (!hex_decode (argv[1], (unsigned char *) data, len))
    {
      free (data);
      return NULL;
    }
    init_keys (keys, argv[0]);
    if (func[6] == 'e')
      encrypt_buf (data, (unsigned int) len, keys);
    else
      decrypt_buf (data, (unsigned int) len, keys);
    out = hex_encode ((unsigned char *) data, len);
    free (data);
    return out;
  }
  if (strcmp (func, "cram_digest") == 0 || strcmp (func, "cram_digest_hex") == 0)
  {
    unsigned char challenge[256];
    char *key;
    size_t len;
    char *out;

    if (func[12] == 'h') /* cram_digest_hex */
    {
      len = strlen (argv[1]) / 2;
      key = malloc (strlen (argv[0]) / 2 + 1);
      if (!hex_decode (argv[1], challenge + 1, len) ||
          !hex_decode (argv[0], (unsigned char *) key, strlen (argv[0]) / 2))
      {
        free (key);
        return NULL;
      }
      key[strlen (argv[0]) / 2] = 0;
    }
    else
    {
      len = strlen (argv[1]);
      key = xstrdup (argv[0]);
      memcpy (challenge + 1, argv[1], len);
    }
    challenge[0] = (unsigned char) len;
    out = MD_buildDigest (key, challenge);
    free (key);
    return out;
  }
  if (strcmp (func, "cram_tostring") == 0)
  {
    unsigned char digest[256];
    size_t len = (size_t) arg_int (argv[1]);
    char *out = malloc (len * 2 + 10);

    if (!hex_decode_prefix (argv[0], digest, len))
    {
      free (out);
      return NULL;
    }
    MD_toString (out, (int) len, digest);
    return out;
  }
  if (strcmp (func, "cram_parse") == 0)
  {
    unsigned char *ch = MD_getChallenge (argv[0], NULL);
    char *out;

    if (!ch)
      return xstrdup ("<null>");
    out = malloc (ch[0] * 2 + 16);
    sprintf (out, "%d:", ch[0]);
    {
      char *hex = hex_encode (ch + 1, ch[0]);
      strcat (out, hex);
      free (hex);
    }
    free (ch);
    return out;
  }
  if (strcmp (func, "addr_parse") == 0)
  {
    FTN_ADDR fa;

    if (arg_is_null (argv[0]) ||
        !parse_ftnaddress (argv[0], &fa, &dom_fidonet))
      return xstrdup ("<error>");
    return addr_string (&fa);
  }
  if (strcmp (func, "addr_format") == 0)
  {
    FTN_ADDR fa;
    char *out = malloc (FTN_ADDR_SZ);

    addr_build (&fa, argv[0], argv[1], argv[2], argv[3], argv[4]);
    xftnaddress_to_str (out, &fa, (int) arg_int (argv[5]));
    return out;
  }
  if (strcmp (func, "addr_cmp") == 0)
  {
    FTN_ADDR a, b;
    int r;

    if (arg_is_null (argv[0]) && arg_is_null (argv[1]))
      return xstrdup ("=");
    if (arg_is_null (argv[0]))
      return xstrdup ("<");
    if (arg_is_null (argv[1]))
      return xstrdup (">");
    if (!parse_ftnaddress (argv[0], &a, &dom_fidonet) ||
        !parse_ftnaddress (argv[1], &b, &dom_fidonet))
      return NULL;
    r = ftnaddress_cmp (&a, &b);
    return xstrdup (r < 0 ? "<" : (r > 0 ? ">" : "="));
  }
  if (strcmp (func, "addr_filename") == 0)
  {
    FTN_ADDR fa;
    char *out = malloc (MAXPATHLEN);

    addr_build (&fa, argv[0], argv[1], argv[2], argv[3], argv[4]);
    ftnaddress_to_filename_ (out, &fa, &dom_fidonet);
    return out;
  }
  if (strcmp (func, "addr_domain") == 0)
  {
    FTN_ADDR fa;
    char *out = malloc (BINKD_FQDNLEN);

    addr_build (&fa, argv[0], argv[1], argv[2], argv[3], argv[4]);
    ftnaddress_to_domain (out, &fa, &dom_fidonet, argv[5]);
    return out;
  }
  if (strcmp (func, "addr_exp") == 0)
  {
    FTN_ADDR fa;

    addr_build (&fa, argv[0], argv[1], argv[2], argv[3], argv[4]);
    exp_ftnaddress (&fa, aka, 1, &dom_fidonet);
    return addr_string (&fa);
  }
  if (strcmp (func, "addr_get_domain") == 0)
  {
    FTN_DOMAIN *d = get_domain_info (argv[0], &dom_fidonet);
    return xstrdup (d ? d->name : "<null>");
  }
  if (strcmp (func, "addr_matched_domain") == 0)
  {
    return xstrdup (get_matched_domain ((int) arg_int (argv[0]), aka, 1,
                                        &dom_fidonet));
  }
  if (strcmp (func, "quote") == 0)
  {
    char *out = strquote (argv[1], (int) arg_int (argv[0]));
    return out;
  }
  if (strcmp (func, "dequote") == 0)
    return strdequote (argv[0]);
  if (strcmp (func, "wipe") == 0)
    return xstrdup (strwipe (argv[0]));
  if (strcmp (func, "ed") == 0)
  {
    char *src = xstrdup (argv[0]);
    char *out = ed (src, argv[1], argv[2], NULL);
    char *res = xstrdup (out);

    if (out != src)
      free (out);
    free (src);
    return res;
  }
  if (strcmp (func, "inboundcase") == 0)
    return xstrdup (makeinboundcase (argv[1], (enum inbcasetype) arg_int (argv[0])));
  if (strcmp (func, "safe_atol") == 0)
  {
    char *msg = NULL;
    long v;

    if (arg_is_null (argv[0]))
      v = safe_atol (NULL, &msg);
    else
      v = safe_atol (argv[0], &msg);
    if (msg)
      return xstrdup ("<error>");
    {
      char *out = malloc (32);
      sprintf (out, "%ld", v);
      return out;
    }
  }
  if (strcmp (func, "last_slash") == 0)
  {
    char *s = last_slash (argv[0]);
    return xstrdup (s ? s : "<null>");
  }
  if (strcmp (func, "extract_filename") == 0)
    return xstrdup (extract_filename (argv[0]));
  if (strcmp (func, "ispkt") == 0)
    return xstrdup (ispkt (argv[0]) ? "1" : "0");
  if (strcmp (func, "isreq") == 0)
    return xstrdup (isreq (argv[0]) ? "1" : "0");
  if (strcmp (func, "istic") == 0)
    return xstrdup (istic (argv[0]) ? "1" : "0");
  if (strcmp (func, "isarcmail") == 0)
    return xstrdup (isarcmail (argv[0]) ? "1" : "0");
  if (strcmp (func, "tz_off") == 0)
  {
    char *out = malloc (32);
    sprintf (out, "%d", tz_off (0, (int) arg_int (argv[0])));
    return out;
  }
  if (strcmp (func, "pkt_getaddr") == 0)
  {
    unsigned char raw[64];
    short oz, onet, onode, op, dz, dnet, dnode, dp;
    char *out;

    if (!hex_decode (argv[0], raw, sizeof (raw)))
      return NULL;
    if (!pkt_getaddr (raw, &oz, &onet, &onode, &op, &dz, &dnet, &dnode, &dp))
      return xstrdup ("<error>");
    out = malloc (128);
    sprintf (out, "%d %d %d %d %d %d %d %d", oz, onet, onode, op,
             dz, dnet, dnode, dp);
    return out;
  }
  if (strcmp (func, "pkt_setaddr") == 0)
  {
    unsigned char raw[64];

    if (!hex_decode (argv[0], raw, sizeof (raw)))
      return NULL;
    if (!pkt_setaddr (raw, (short) arg_int (argv[1]), (short) arg_int (argv[2]),
                       (short) arg_int (argv[3]), (short) arg_int (argv[4]),
                       (short) arg_int (argv[5]), (short) arg_int (argv[6]),
                       (short) arg_int (argv[7]), (short) arg_int (argv[8])))
      return xstrdup ("<error>");
    return hex_encode (raw, sizeof (raw));
  }
  if (strcmp (func, "flo_read") == 0)
  {
    FILE *flo = tmpfile ();
    char buf[MAXPATHLEN + 1];
    char *out;
    int action;
    size_t used = 0;

    if (!flo)
      return NULL;
    fputs (argv[0], flo);
    rewind (flo);
    out = malloc (strlen (argv[0]) * 2 + 32);
    out[0] = 0;
    while (read_flo_line (buf, &action, flo))
    {
      if (used)
        strcat (out, "|");
      sprintf (out + strlen (out), "%c:%s",
               action == 'd' ? 'd' : (action == 't' ? 't' : '-'), buf);
      used = 1;
    }
    fclose (flo);
    return out;
  }
  if (strcmp (func, "flo_trans") == 0)
  {
    RF_RULE *head = NULL, *prev = NULL;
    char *out;
    int i;

    for (i = 1; i + 1 < argc; i += 2)
    {
      RF_RULE *r = malloc (sizeof (RF_RULE));
      r->next = NULL;
      r->from = argv[i];
      r->to = argv[i + 1];
      if (prev)
        prev->next = r;
      else
        head = r;
      prev = r;
    }
    out = trans_flo_line (argv[0], head);
    if (!out)
      out = xstrdup ("<null>");
    while (head)
    {
      RF_RULE *next = head->next;
      free (head);
      head = next;
    }
    return out;
  }
  return NULL;
}

static void run_file (const char *path)
{
  FILE *f = fopen (path, "r");
  char line[MAX_LINE];
  char crate_name[64];
  int lineno = 0;


  if (!f)
  {
    fprintf (stderr, "FAIL: cannot open %s\n", path);
    ++test_failures;
    return;
  }

  while (fgets (line, sizeof (line), f))
  {
    char *fields[MAX_FIELDS];
    int nfields = 0;
    char *sep;
    char *func;
    char *want;
    char *got;
    int i;

    ++lineno;
    sep = strchr (line, '\n');
    if (sep)
      *sep = 0;
    if (line[0] == '#')
    {
      if (strncmp (line, "# crate:", 8) == 0)
      {
        char *name = line + 8;
        while (*name == ' ')
          ++name;
        strnzcpy (crate_name, name, sizeof (crate_name));
      }
      continue;
    }
    if (line[0] == 0)
      continue;

    for (i = 0; i < MAX_FIELDS; ++i)
      fields[i] = NULL;
    {
      char *p = line;
      while (nfields < MAX_FIELDS)
      {
        char *tab = strchr (p, '\t');
        if (tab)
          *tab = 0;
        fields[nfields++] = p;
        if (!tab)
          break;
        p = tab + 1;
      }
    }
    /* Skip empty lines. */
    if (nfields == 0 || fields[0][0] == 0)
      continue;

    /* Find the "=>" separator. */
    func = fields[0];
    for (i = 1; i < nfields; ++i)
      if (strcmp (fields[i], "=>") == 0)
        break;
    if (i >= nfields || i + 1 >= nfields)
    {
      fprintf (stderr, "%s:%d: FAIL: bad case\n", path, lineno);
      ++test_failures;
      continue;
    }

    {
      int argc = i - 1;
      char **argv = malloc ((argc + 1) * sizeof (char *));
      char *decoded[MAX_FIELDS];
      int j, bad = 0;

      argv[argc] = NULL;
      for (j = 0; j < argc; ++j)
      {
        decoded[j] = field_decode (fields[j + 1]);
        if (!decoded[j])
        {
          fprintf (stderr, "%s:%d: FAIL: bad escape in %s\n",
                   path, lineno, func);
          ++test_failures;
          bad = 1;
        }
        argv[j] = decoded[j];
      }
      want = field_decode (fields[i + 1]);
      if (!want)
      {
        fprintf (stderr, "%s:%d: FAIL: bad escape in want\n", path, lineno);
        ++test_failures;
      }
      else
      {
        got = bad ? NULL : run_case (func, argc, argv);
        if (!bad)
          ++test_checks;
        if (!got || strcmp (got, want) != 0)
        {
          char *g = field_encode (got ? got : "(null)");
          char *w = field_encode (want);

          ++test_failures;
          fprintf (stderr, "%s:%d: FAIL: %s: got \"%s\", want \"%s\"\n",
                   path, lineno, func, g, w);
          free (g);
          free (w);
        }
        free (got);
        free (want);
      }
      for (j = 0; j < argc; ++j)
        free (decoded[j]);
      free (argv);
    }
  }
  fclose (f);

  /* Every file belongs to a crate of the Rust workspace. */
  if (crate_name[0] == 0
      || (strcmp (crate_name, "binkd-ftn") != 0
          && strcmp (crate_name, "binkd-spool") != 0))
  {
    fprintf (stderr, "%s: FAIL: bad or missing crate header\n", path);
    ++test_failures;
  }
}

int main (int argc, char **argv)
{
  int i;

  test_init ();
  init_tables ();
  if (argc < 2)
  {
    fprintf (stderr, "usage: test_corpus <file>...\n");
    return 1;
  }
  for (i = 1; i < argc; ++i)
    run_file (argv[i]);
  return test_finish ("test_corpus");
}
