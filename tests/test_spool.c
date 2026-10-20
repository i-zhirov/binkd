/*
 *  test_spool.c -- the spool corpus runner for the C code
 *
 *  test_spool.c is a part of binkd project
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 2 of the License, or
 *  (at your option) any later version. See COPYING.
 *
 *  A case is a file in tests/golden/spool/.  The lines before the `---`
 *  line set up a spool and run the operations.  The lines after `---` are
 *  the expected output.  The Rust test reads the same files.
 */

#include <stdio.h>
#include <stdlib.h>
#include <stdarg.h>
#include <string.h>
#include <ctype.h>
#include <errno.h>
#include <dirent.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <utime.h>

#include "sys.h"
#include "readcfg.h"
#include "ftnq.h"
#include "ftnnode.h"
#include "ftnaddr.h"
#include "tools.h"
#include "harness.h"

/* `ftnq.c` does not declare `process_hld()` in `ftnq.h`. */
void process_hld (FTN_ADDR *fa, char *path, BINKD_CONFIG *config);

#define MAX_LINE 4096
#define MAX_LINES 1024
#define MAX_FIELDS 16
#define MAX_AKAS 16

static char root[MAXPATHLEN + 1];
static BINKD_CONFIG cfg;
static FTN_ADDR akas[MAX_AKAS];
static int nakas = 0;
static FTNQ *q = NULL;
static char out[MAX_LINES][MAX_LINE];
static int nout = 0;

static void emit (const char *line)
{
  if (nout < MAX_LINES)
    strnzcpy (out[nout++], line, MAX_LINE);
}

static void emitf (const char *fmt, ...)
{
  va_list ap;
  char buf[MAX_LINE];

  va_start (ap, fmt);
  vsnprintf (buf, sizeof (buf), fmt, ap);
  va_end (ap);
  emit (buf);
}

/* Returns the path relative to the case root. */
static char *rel (const char *path)
{
  static char buf[4][MAXPATHLEN + 1];
  static int n = 0;
  char *b = buf[n++ & 3];
  size_t rl = strlen (root);

  if (!strncmp (path, root, rl) && path[rl] == '/')
    strnzcpy (b, path + rl + 1, MAXPATHLEN);
  else
    strnzcpy (b, path, MAXPATHLEN);
  return b;
}

/* Creates all the directories of a path. */
static void mkdirs (const char *path)
{
  char buf[MAXPATHLEN + 1];
  char *s;

  strnzcpy (buf, path, sizeof (buf));
  for (s = buf + 1; *s; ++s)
  {
    if (*s == '/')
    {
      *s = 0;
      MKDIR (buf);
      *s = '/';
    }
  }
  MKDIR (buf);
}

static void mkfile (const char *path, const char *content, long size, long mtime)
{
  char full[MAXPATHLEN + 1];
  FILE *f;
  struct utimbuf utb;

  snprintf (full, sizeof (full), "%s/%s", root, path);
  {
    /* create the parent directories, not the file itself */
    char *slash = strrchr (full, '/');
    if (slash)
    {
      *slash = 0;
      mkdirs (full);
      *slash = '/';
    }
  }
  if ((f = fopen (full, "wb")) != NULL)
  {
    long i;
    if (content)
      fwrite (content, 1, strlen (content), f);
    else
      for (i = 0; i < size; ++i)
        fputc (0, f);
    fclose (f);
  }
  if (mtime > 0)
  {
    utb.actime = utb.modtime = mtime;
    utime (full, &utb);
  }
}

/* Lists the files of a directory and its subdirectories, sorted. */
static void walk (const char *dir, const char *prefix)
{
  DIR *dp = opendir (dir);
  struct dirent *de;
  char names[256][256];   /* local: walk() recurses */
  int n = 0, i;

  if (!dp)
    return;
  while ((de = readdir (dp)) != NULL && n < 256)
  {
    if (!strcmp (de->d_name, ".") || !strcmp (de->d_name, ".."))
      continue;
    strnzcpy (names[n++], de->d_name, 256);
  }
  closedir (dp);

  for (i = 0; i < n; ++i)
  {
    int j;
    for (j = i + 1; j < n; ++j)
    {
      if (strcmp (names[j], names[i]) < 0)
      {
        char tmp[256];
        strcpy (tmp, names[i]);
        strcpy (names[i], names[j]);
        strcpy (names[j], tmp);
      }
    }
  }

  for (i = 0; i < n; ++i)
  {
    char full[MAXPATHLEN + 1], sub[MAXPATHLEN + 1];
    struct stat sb;

    snprintf (full, sizeof (full), "%s/%s", dir, names[i]);
    if (prefix[0])
      snprintf (sub, sizeof (sub), "%s/%s", prefix, names[i]);
    else
      strnzcpy (sub, names[i], sizeof (sub));

    if (stat (full, &sb) == 0 && (sb.st_mode & S_IFDIR))
      walk (full, sub);
    else
      emitf ("file\t%s\t%ju", sub, (uintmax_t) sb.st_size);
  }
}

static int collect_node (FTN_NODE *fn, void *arg)
{
  char addr[FTN_ADDR_SZ + 1], hold[16];

  (void) arg;
  ftnaddress_to_str (addr, &fn->fa);
  if (fn->hold_until == 0)
    strcpy (hold, "0");
  else if (fn->hold_until <= safe_time())
    strcpy (hold, "past");
  else
    strcpy (hold, "future");

  emitf ("node\t%s\t%c\t%c\t%c\t%s", addr,
          fn->mail_flvr ? fn->mail_flvr : '-',
          fn->files_flvr ? fn->files_flvr : '-',
          fn->busy ? fn->busy : '-', hold);
  return 0;
}

static int parse_addr (const char *s, FTN_ADDR *fa)
{
  char buf[FTN_ADDR_SZ + 1];

  strnzcpy (buf, s, sizeof (buf));
  return parse_ftnaddress (buf, fa, cfg.pDomains.first);
}

static FTN_DOMAIN *domain_last = NULL;
static SHARED_CHAIN *share_last = NULL;

static void add_domain (const char *name, const char *dir, int z0)
{
  FTN_DOMAIN **last = &domain_last;
  FTN_DOMAIN *d = xalloc (sizeof (FTN_DOMAIN));

  memset (d, 0, sizeof (*d));
  strnzcpy (d->name, name, sizeof (d->name));
  d->path = xstrdup (root);
  d->dir = xstrdup (dir);
  d->z[0] = z0;
  d->z[1] = -1;
  if (*last)
    (*last)->next = d;
  else
    cfg.pDomains.first = d;
  *last = d;
}

static void add_config_node (const char *addr, const char *hosts, const char *obox)
{
  FTN_ADDR fa;
  FTN_NODE *pn;

  if (!parse_addr (addr, &fa))
  {
    fprintf (stderr, "bad node address `%s'\n", addr);
    exit (1);
  }
  {
    static char obox_path[MAXPATHLEN + 1];
    if (!strcmp (obox, "-"))
      obox_path[0] = 0;
    else
      snprintf (obox_path, sizeof (obox_path), "%s/%s", root, obox);
    pn = add_node (&fa, strcmp (hosts, "<null>") ? (char *) hosts : NULL,
                  NULL, NULL, NULL, '-',
                  obox_path[0] ? obox_path : NULL, NULL,
                  NR_USE_OLD, ND_USE_OLD, MD_USE_OLD, RIP_USE_OLD,
                  HC_USE_OLD, NP_USE_OLD, NULL, AF_USE_OLD, &cfg);
    pn->listed = NL_NODE;
  }
}

static void add_share (char *args[], int n)
{
  SHARED_CHAIN *chn = xalloc (sizeof (SHARED_CHAIN));
  FTN_ADDR_CHAIN *last = NULL;
  int i;

  memset (chn, 0, sizeof (*chn));
  if (!parse_addr (args[0], &chn->sha))
  {
    fprintf (stderr, "bad share address `%s'\n", args[0]);
    exit (1);
  }
  for (i = 1; i < n; ++i)
  {
    FTN_ADDR_CHAIN *c = xalloc (sizeof (FTN_ADDR_CHAIN));
    memset (c, 0, sizeof (*c));
    if (!parse_addr (args[i], &c->fa))
    {
      fprintf (stderr, "bad share address `%s'\n", args[i]);
      exit (1);
    }
    if (last)
      last->next = c;
    else
      chn->sfa.first = c;
    last = c;
  }
  if (share_last)
    share_last->next = chn;
  else
    cfg.shares.first = chn;
  share_last = chn;
}

static void run_op (char *args[], int n)
{
  const char *op = args[0];

  (void) n;
  if (!strcmp (op, "scan"))
  {
    if (q && q != SCAN_LISTED)
      q_free (q, &cfg);
    q = q_scan (NULL, &cfg);
  }
  else if (!strcmp (op, "scan_listed"))
  {
    q = q_scan (SCAN_LISTED, &cfg);
  }
  else if (!strcmp (op, "reset_listed"))
  {
    q_free (SCAN_LISTED, &cfg);
    q = NULL;
  }
  else if (!strcmp (op, "sort"))
  {
    q = q_sort (q, cfg.pAddr, cfg.nAddr, &cfg);
  }
  else if (!strcmp (op, "select"))
  {
    FTN_ADDR fa;
    FTNQ *f;

    if (!parse_addr (args[1], &fa))
      exit (1);
    f = select_next_file (q, &fa, cfg.nAddr);
    emitf ("select\t%s", f ? rel (f->path) : "-");
  }
  else if (!strcmp (op, "sizes"))
  {
    uintmax_t ns, fs;

    q_get_sizes (q, &ns, &fs);
    emitf ("sizes\t%ju\t%ju", ns, fs);
  }
  else if (!strcmp (op, "freq"))
  {
    emitf ("freq\t%d", q_freq_num (q));
  }
  else if (!strcmp (op, "dump"))
  {
    FTNQ *c;

    for (c = q; c && c != SCAN_LISTED; c = c->next)
      emitf ("queue\t%s\t%c\t%c\t%c\t%ju\t%c",
              rel (c->path), c->flvr,
              c->action ? c->action : '-',
              c->type ? c->type : '-',
              (uintmax_t) c->size, c->sent ? '+' : '-');
  }
  else if (!strcmp (op, "nodes"))
  {
    foreach_node (collect_node, NULL, &cfg);
  }
  else if (!strcmp (op, "files"))
  {
    walk (root, "");
  }
  else if (!strcmp (op, "hex") || !strcmp (op, "text") || !strcmp (op, "exists"))
  {
    char full[MAXPATHLEN + 1];
    FILE *f;
    char data[MAX_LINE];
    size_t len;

    snprintf (full, sizeof (full), "%s/%s", root, args[1]);
    f = fopen (full, "rb");
    if (!strcmp (op, "exists"))
    {
      emitf ("exists\t%s\t%d", args[1], f != NULL);
      if (f)
        fclose (f);
      return;
    }
    if (!f)
    {
      emitf ("%s\t%s\t-", op, args[1]);
      return;
    }
    len = fread (data, 1, sizeof (data) - 1, f);
    fclose (f);
    data[len] = 0;
    if (!strcmp (op, "hex"))
    {
      char hex[MAX_LINE];
      size_t i;
      for (i = 0; i < len; ++i)
        sprintf (hex + i * 2, "%02x", (unsigned char) data[i]);
      hex[len * 2] = 0;
      emitf ("hex\t%s\t%s", args[1], hex);
    }
    else
    {
      char *quoted = strquote (data, SQ_CNTRL | SQ_SPACE);
      emitf ("text\t%s\t%s", args[1], quoted);
      free (quoted);
    }
  }
  else if (!strcmp (op, "read_try"))
  {
    FTN_ADDR fa;
    unsigned nok = 0, nbad = 0;

    if (!parse_addr (args[1], &fa))
      exit (1);
    read_try (&fa, &nok, &nbad, &cfg);
    emitf ("try\t%s\t%u\t%u", args[1], nok, nbad);
  }
  else if (!strcmp (op, "write_try"))
  {
    FTN_ADDR fa;
    unsigned nok, nbad;

    if (!parse_addr (args[1], &fa))
      exit (1);
    nok = (unsigned) atoi (args[2]);
    nbad = (unsigned) atoi (args[3]);
    write_try (&fa, &nok, &nbad, args[4] ? args[4] : "", &cfg);
  }
  else if (!strcmp (op, "bad_try"))
  {
    FTN_ADDR fa;

    if (!parse_addr (args[1], &fa))
      exit (1);
    bad_try (&fa, args[2], atoi (args[3]), &cfg);
  }
  else if (!strcmp (op, "good_try"))
  {
    FTN_ADDR fa;

    if (!parse_addr (args[1], &fa))
      exit (1);
    good_try (&fa, args[2] ? args[2] : "", &cfg);
  }
  else if (!strcmp (op, "remove_try"))
  {
    FTN_ADDR fa;

    if (!parse_addr (args[1], &fa))
      exit (1);
    remove_try (&fa, &cfg);
  }
  else if (!strcmp (op, "hold"))
  {
    FTN_ADDR fa;

    if (!parse_addr (args[1], &fa))
      exit (1);
    hold_node (&fa, (time_t) strtoumax (args[2], NULL, 10), &cfg);
  }
  else if (!strcmp (op, "process_hld"))
  {
    FTN_ADDR fa;
    char buf[MAXPATHLEN + 1];

    if (!parse_addr (args[1], &fa))
      exit (1);
    ftnaddress_to_filename (buf, &fa, &cfg);
    if (*buf)
      strnzcat (buf, ".hld", sizeof (buf));
    process_hld (&fa, buf, &cfg);
  }
  else
  {
    fprintf (stderr, "unknown op `%s'\n", op);
    exit (1);
  }
}

static void run_file (const char *path)
{
  FILE *f = fopen (path, "r");
  static char lines[MAX_LINES][MAX_LINE];
  int nlines = 0, i;
  int expected;

  if (!f)
  {
    fprintf (stderr, "FAIL: cannot open %s\n", path);
    ++test_failures;
    return;
  }
  while (nlines < MAX_LINES && fgets (lines[nlines], MAX_LINE, f))
  {
    char *s = strchr (lines[nlines], '\n');
    if (s)
      *s = 0;
    ++nlines;
  }
  fclose (f);

  strnzcpy (root, "/tmp/binkd-spool-XXXXXX", sizeof (root));
  if (!mkdtemp (root))
  {
    fprintf (stderr, "FAIL: cannot create a case directory\n");
    ++test_failures;
    return;
  }

  memset (&cfg, 0, sizeof (cfg));
  q = NULL;
  nout = 0;
  nakas = 0;
  domain_last = NULL;
  share_last = NULL;

  /* The setup and the config. */
  for (i = 0; i < nlines; ++i)
  {
    char *fields[MAX_FIELDS];
    int nf = 0;
    char *s = lines[i];

    if (!*s || *s == '#')
      continue;
    if (!strncmp (s, "op\t", 3) || !strncmp (s, "---", 3))
      break;

    while (s && nf < MAX_FIELDS)
    {
      char *tab = strchr (s, '\t');
      if (tab)
        *tab = 0;
      fields[nf++] = s;
      s = tab ? tab + 1 : NULL;
    }
    if (nf < 2)
      continue;

    if (!strcmp (fields[0], "setup"))
    {
      if (nf < 3)
        continue;
      if (!strcmp (fields[1], "dir"))
      {
        char full[MAXPATHLEN + 1];
        snprintf (full, sizeof (full), "%s/%s", root, fields[2]);
        mkdirs (full);
      }
      else if (!strcmp (fields[1], "file"))
        mkfile (fields[2], NULL, atol (fields[3]), atol (fields[4]));
      else if (!strcmp (fields[1], "text"))
      {
        char *text = strdequote (fields[3]);
        mkfile (fields[2], text, 0, 0);
        free (text);
      }
    }
    else if (!strcmp (fields[0], "config"))
    {
      if (!strcmp (fields[1], "domain"))
        add_domain (fields[2], fields[3], atoi (fields[4]));
      else if (!strcmp (fields[1], "node"))
        add_config_node (fields[2], fields[3], fields[4]);
      else if (!strcmp (fields[1], "aka"))
      {
        if (nakas < MAX_AKAS && parse_addr (fields[2], &akas[nakas]))
          ++nakas;
      }
      else if (!strcmp (fields[1], "share"))
        add_share (fields + 2, nf - 2);
      else if (!strcmp (fields[1], "scalar"))
      {
        if (!strcmp (fields[2], "tries"))
          cfg.tries = atoi (fields[3]);
        else if (!strcmp (fields[2], "hold"))
          cfg.hold = atol (fields[3]);
        else if (!strcmp (fields[2], "remove_try_files"))
          cfg.remove_try_files = atoi (fields[3]);
        else if (!strcmp (fields[2], "send_if_pwd"))
          cfg.send_if_pwd = atoi (fields[3]);
        else if (!strcmp (fields[2], "havedefnode"))
          cfg.havedefnode = atoi (fields[3]);
        else if (!strcmp (fields[2], "kill_old_bsy"))
          cfg.kill_old_bsy = atol (fields[3]);
        else if (!strcmp (fields[2], "deleteablebox"))
          cfg.deleteablebox = atoi (fields[3]);
        else if (!strcmp (fields[2], "tfilebox"))
          snprintf (cfg.tfilebox, sizeof (cfg.tfilebox), "%s/%s", root, fields[3]);
        else if (!strcmp (fields[2], "bfilebox"))
          snprintf (cfg.bfilebox, sizeof (cfg.bfilebox), "%s/%s", root, fields[3]);
      }
    }
  }

  cfg.pAddr = akas;
  cfg.nAddr = nakas;

  /* The operations. */
  expected = nlines;
  for (; i < nlines; ++i)
  {
    char *fields[MAX_FIELDS];
    int nf = 0;
    char *s = lines[i];

    if (!*s || *s == '#')
      continue;
    if (!strncmp (s, "---", 3))
    {
      expected = i + 1;
      break;
    }
    while (s && nf < MAX_FIELDS)
    {
      char *tab = strchr (s, '\t');
      if (tab)
        *tab = 0;
      fields[nf++] = s;
      s = tab ? tab + 1 : NULL;
    }
    if (nf < 2)
      continue;
    run_op (fields + 1, nf - 1);
  }

  if (getenv ("BINKD_SPOOL_DUMP"))
  {
    int k;
    printf ("== %s\n", path);
    for (k = 0; k < nout; ++k)
      printf ("%s\n", out[k]);
  }

  test_checks++;
  if (nout != nlines - expected)
  {
    ++test_failures;
    fprintf (stderr, "%s: FAIL: got %d lines, want %d\n", path, nout,
             nlines - expected);
  }
  else
  {
    int k;
    for (k = 0; k < nout; ++k)
    {
      ++test_checks;
      if (strcmp (out[k], lines[expected + k]) != 0)
      {
        ++test_failures;
        fprintf (stderr, "%s: FAIL: line %d: got \"%s\", want \"%s\"\n",
                 path, k + 1, out[k], lines[expected + k]);
      }
    }
  }

  if (q && q != SCAN_LISTED)
    q_free (q, &cfg);
  free_nodes (&cfg);
}

int main (int argc, char **argv)
{
  int i;

  test_init ();
  if (argc < 2)
  {
    fprintf (stderr, "usage: test_spool <case>...\n");
    return 1;
  }
  for (i = 1; i < argc; ++i)
    run_file (argv[i]);
  return test_finish ("test_spool");
}
