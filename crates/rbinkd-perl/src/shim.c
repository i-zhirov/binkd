/* The bridge of the Perl C API, the port of the macros and of the XS
 * functions of `perlhooks.c`.
 *
 * The C code of `perlhooks.c` holds the macros `dSP`, `PUSHMARK`, `SPAGAIN`,
 * and `POPs`, which a port cannot call, the lifecycle of the interpreter, and
 * the XS functions `Log`, `aeq`, `arm`, `msg_send`, and `binkd_warn`. The shim
 * holds the same calls for the crate of the FFI. The logic of the hooks and of
 * the helpers stays in Rust. */

#include <EXTERN.h>
#include <perl.h>
#include <XSUB.h>

#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The entry points of the Rust logic of the XS helpers. */
extern void rbinkd_perl_xs_log(int level, const char *text);
extern int rbinkd_perl_xs_aeq(const char *first, size_t count,
                             const char *const *others);
extern int rbinkd_perl_xs_arm(size_t count, const char *const *elements,
                             size_t arg_count, const char *const *args,
                             int *keep);
extern void rbinkd_perl_xs_msg_send(int kind, const char *text);
extern void rbinkd_perl_xs_warn(const char *text);

#ifndef PERLDL
XS(boot_DynaLoader);
#endif

static void rbinkd_xs_init(pTHX);

void rbinkd_perl_sys_init(void) {
  int argc = 1;
  char *args[] = {"rbinkd", NULL};
  char **argv = args;
  char **env = NULL;
  PERL_SYS_INIT3(&argc, &argv, &env);
}

PerlInterpreter *rbinkd_perl_alloc(void) { return perl_alloc(); }

void rbinkd_perl_construct(PerlInterpreter *perl) {
  PERL_SET_CONTEXT(perl);
  perl_construct(perl);
}

int rbinkd_perl_parse(PerlInterpreter *perl, int argc, char **argv, char **env) {
  PERL_SET_CONTEXT(perl);
  return perl_parse(perl, rbinkd_xs_init, argc, argv, env);
}

int rbinkd_perl_run(PerlInterpreter *perl) { return perl_run(perl); }

void rbinkd_perl_set_warn_hook(PerlInterpreter *perl) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (PL_warnhook) SvREFCNT_dec(PL_warnhook);
  if (PL_diehook) SvREFCNT_dec(PL_diehook);
  PL_warnhook = newRV_inc((SV *)get_cv("binkd_warn", TRUE));
  PL_diehook = newRV_inc((SV *)get_cv("binkd_warn", TRUE));
}

int rbinkd_perl_do_file(PerlInterpreter *perl, const char *path) {
  char *cmd;
  size_t len;
  SV *sv;
  int rc = 0;

  len = strlen(path) + 32;
  cmd = (char *)malloc(len);
  if (!cmd) return 1;
  snprintf(cmd, len, "do '%s'; $@ ? $@ : '';", path);
  PERL_SET_CONTEXT(perl);
  {
    dTHX;
    /* The C code passes TRUE and holds `binkd_warn` as the die hook. The port
     * passes FALSE, so a broken script answers an error and the caller
     * chooses the end, the port of the `perl-strict` rule. */
    sv = eval_pv(cmd, FALSE);
    if (!sv || !SvPOK(sv) || SvTRUE(sv)) rc = 1;
  }
  free(cmd);
  return rc;
}

void rbinkd_perl_free(PerlInterpreter *perl) {
  PERL_SET_CONTEXT(perl);
  {
    dTHX;
    PL_perl_destruct_level = 1;
  }
  perl_destruct(perl);
  perl_free(perl);
}

PerlInterpreter *rbinkd_perl_clone(PerlInterpreter *perl) {
  PerlInterpreter *clone;
  PERL_SET_CONTEXT(perl);
  clone = perl_clone(perl, 0);
  if (clone) {
    PERL_SET_CONTEXT(clone);
    /* The perl<5.6.1 hack of `perl_init_clone()` of `perlhooks.c`. */
    {
      dTHX;
      if (PL_scopestack_ix == 0) {
        ENTER;
      }
    }
  }
  return clone;
}

void rbinkd_perl_free_clone(PerlInterpreter *perl) {
  if (!perl) return;
  PERL_SET_CONTEXT(perl);
  {
    dTHX;
    PL_perl_destruct_level = 2;
  }
  perl_destruct(perl);
  perl_free(perl);
}

SV *rbinkd_perl_get_sv(PerlInterpreter *perl, const char *name, int create) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  return get_sv(name, create);
}

void rbinkd_perl_set_string(PerlInterpreter *perl, SV *value, const char *text) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  sv_setpv(value, text);
}

void rbinkd_perl_set_int(PerlInterpreter *perl, SV *value, IV number) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  sv_setiv(value, number);
}

IV rbinkd_perl_int(PerlInterpreter *perl, SV *value) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  return SvIV(value);
}

int rbinkd_perl_ok(PerlInterpreter *perl, SV *value) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  return SvOK(value) ? 1 : 0;
}

char *rbinkd_perl_string(PerlInterpreter *perl, SV *value, STRLEN *len) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  return SvPV(value, *len);
}

int rbinkd_perl_has_sub(PerlInterpreter *perl, const char *name) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  return get_cv(name, 0) ? 1 : 0;
}

int rbinkd_perl_call(PerlInterpreter *perl, const char *name, int flags,
                    IV *result) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  SV *svret;
  int rc;
  dSP;
  ENTER;
  SAVETMPS;
  PUSHMARK(SP);
  PUTBACK;
  perl_call_pv(name, flags);
  SPAGAIN;
  svret = POPs;
  if (SvOK(svret)) {
    *result = SvIV(svret);
    rc = 1;
  } else {
    rc = 0;
  }
  PUTBACK;
  FREETMPS;
  LEAVE;
  return rc;
}

int rbinkd_perl_error(PerlInterpreter *perl) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  return SvTRUE(ERRSV) ? 1 : 0;
}

int rbinkd_perl_call_void(PerlInterpreter *perl, const char *name) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  int error;
  dSP;
  ENTER;
  SAVETMPS;
  PUSHMARK(SP);
  PUTBACK;
  perl_call_pv(name, G_EVAL | G_VOID);
  SPAGAIN;
  PUTBACK;
  FREETMPS;
  LEAVE;
  error = SvTRUE(ERRSV) ? 1 : 0;
  return error;
}

char *rbinkd_perl_call_string(PerlInterpreter *perl, const char *name,
                             int *defined, STRLEN *len) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  SV *svret;
  char *out = NULL;
  STRLEN l = 0;
  int error;
  dSP;
  ENTER;
  SAVETMPS;
  PUSHMARK(SP);
  PUTBACK;
  perl_call_pv(name, G_EVAL | G_SCALAR);
  SPAGAIN;
  svret = POPs;
  error = SvTRUE(ERRSV) ? 1 : 0;
  if (!error && SvTRUE(svret)) {
    char *p = SvPV(svret, l);
    out = (char *)malloc(l + 1);
    if (out) {
      memcpy(out, p, l);
      out[l] = 0;
    }
  }
  PUTBACK;
  FREETMPS;
  LEAVE;
  *len = l;
  if (error) {
    *defined = -1;
    free(out);
    return NULL;
  }
  *defined = out ? 1 : 0;
  return out;
}

char *rbinkd_perl_error_string(PerlInterpreter *perl) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  STRLEN len = 0;
  char *p = SvPV(ERRSV, len);
  char *out = (char *)malloc(len + 1);
  if (!out) return NULL;
  memcpy(out, p, len);
  out[len] = 0;
  return out;
}

void rbinkd_perl_free_string(char *text) { free(text); }

SV *rbinkd_perl_get_av(PerlInterpreter *perl, const char *name, int create) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  return (SV *)get_av(name, create);
}

SV *rbinkd_perl_get_hv(PerlInterpreter *perl, const char *name, int create) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  return (SV *)get_hv(name, create);
}

void rbinkd_perl_hv_clear(PerlInterpreter *perl, SV *hash) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (hash) hv_clear((HV *)hash);
}

void rbinkd_perl_hv_store_int(PerlInterpreter *perl, SV *hash, const char *key,
                             IV value) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (hash) hv_store((HV *)hash, key, strlen(key), newSViv(value), 0);
}

void rbinkd_perl_av_push_hash(PerlInterpreter *perl, SV *array,
                             const char *file, IV size, IV time, IV sent,
                             const char *flvr, const char *action,
                             const char *type, const char *addr) {
  HV *hash;
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (!array) return;
  hash = newHV();
  hv_store(hash, "file", 4, newSVpv(file, 0), 0);
  hv_store(hash, "size", 4, newSViv(size), 0);
  hv_store(hash, "time", 4, newSViv(time), 0);
  hv_store(hash, "sent", 4, newSViv(sent), 0);
  hv_store(hash, "flvr", 4, newSVpv(flvr, 0), 0);
  hv_store(hash, "action", 6, newSVpv(action, 0), 0);
  hv_store(hash, "type", 4, newSVpv(type, 0), 0);
  hv_store(hash, "addr", 4, newSVpv(addr, 0), 0);
  av_push((AV *)array, newRV_noinc((SV *)hash));
}

void rbinkd_perl_av_clear(PerlInterpreter *perl, SV *array) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (array) av_clear((AV *)array);
}

void rbinkd_perl_av_undef(PerlInterpreter *perl, SV *array) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (array) av_undef((AV *)array);
}

void rbinkd_perl_av_push_string(PerlInterpreter *perl, SV *array,
                               const char *text) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (array) av_push((AV *)array, newSVpv(text, 0));
}

I32 rbinkd_perl_av_len(PerlInterpreter *perl, SV *array) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (!array) return -1;
  return av_len((AV *)array);
}

char *rbinkd_perl_av_fetch_string(PerlInterpreter *perl, SV *array, int index,
                                 int *defined, STRLEN *len) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  SV **svp;
  char *p;
  char *out;
  *defined = 0;
  *len = 0;
  if (!array) return NULL;
  svp = av_fetch((AV *)array, index, FALSE);
  if (!svp || !SvOK(*svp)) return NULL;
  p = SvPV(*svp, *len);
  out = (char *)malloc(*len + 1);
  if (!out) return NULL;
  memcpy(out, p, *len);
  out[*len] = 0;
  *defined = 1;
  return out;
}

SV *rbinkd_perl_av_fetch_hash(PerlInterpreter *perl, SV *array, int index) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  SV **svp;
  SV *value;
  if (!array) return NULL;
  svp = av_fetch((AV *)array, index, FALSE);
  if (!svp || !SvROK(*svp)) return NULL;
  value = SvRV(*svp);
  if (SvTYPE(value) != SVt_PVHV) return NULL;
  return value;
}

char *rbinkd_perl_hv_fetch_string(PerlInterpreter *perl, SV *hash,
                                 const char *key, int *defined, STRLEN *len) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  SV **svp;
  char *p;
  char *out;
  *defined = 0;
  *len = 0;
  if (!hash) return NULL;
  svp = hv_fetch((HV *)hash, key, strlen(key), FALSE);
  if (!svp || !SvOK(*svp)) return NULL;
  p = SvPV(*svp, *len);
  out = (char *)malloc(*len + 1);
  if (!out) return NULL;
  memcpy(out, p, *len);
  out[*len] = 0;
  *defined = 1;
  return out;
}

IV rbinkd_perl_hv_fetch_int(PerlInterpreter *perl, SV *hash, const char *key,
                           int *defined) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  SV **svp;
  *defined = 0;
  if (!hash) return 0;
  svp = hv_fetch((HV *)hash, key, strlen(key), FALSE);
  if (!svp || !SvOK(*svp)) return 0;
  *defined = 1;
  return SvIV(*svp);
}

SV *rbinkd_perl_hash_new(PerlInterpreter *perl) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  return (SV *)newHV();
}

SV *rbinkd_perl_array_new(PerlInterpreter *perl) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  return (SV *)newAV();
}

void rbinkd_perl_hv_store_string(PerlInterpreter *perl, SV *hash,
                                const char *key, const char *value) {
  SV *sv;
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (!hash) return;
  sv = newSVpv(value ? value : "", 0);
  SvREADONLY_on(sv);
  hv_store((HV *)hash, key, strlen(key), sv, 0);
}

void rbinkd_perl_hv_store_int_ro(PerlInterpreter *perl, SV *hash,
                                const char *key, IV value) {
  SV *sv;
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (!hash) return;
  sv = newSViv(value);
  SvREADONLY_on(sv);
  hv_store((HV *)hash, key, strlen(key), sv, 0);
}

void rbinkd_perl_hv_store_undef(PerlInterpreter *perl, SV *hash,
                               const char *key) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (!hash) return;
  hv_store((HV *)hash, key, strlen(key), &PL_sv_undef, 0);
}

SV *rbinkd_perl_hv_store_new_hash(PerlInterpreter *perl, SV *hash,
                                 const char *key, int readonly) {
  HV *inner;
  SV *ref;
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (!hash) return NULL;
  inner = newHV();
  ref = newRV_noinc((SV *)inner);
  if (readonly) SvREADONLY_on(ref);
  hv_store((HV *)hash, key, strlen(key), ref, 0);
  return (SV *)inner;
}

SV *rbinkd_perl_hv_store_new_array(PerlInterpreter *perl, SV *hash,
                                  const char *key) {
  AV *inner;
  SV *ref;
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (!hash) return NULL;
  inner = newAV();
  ref = newRV_noinc((SV *)inner);
  hv_store((HV *)hash, key, strlen(key), ref, 0);
  return (SV *)inner;
}

void rbinkd_perl_av_push_string_ro(PerlInterpreter *perl, SV *array,
                                  const char *text) {
  SV *sv;
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (!array) return;
  sv = newSVpv(text ? text : "", 0);
  SvREADONLY_on(sv);
  av_push((AV *)array, sv);
}

SV *rbinkd_perl_av_push_new_hash(PerlInterpreter *perl, SV *array,
                                int readonly) {
  HV *inner;
  SV *ref;
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (!array) return NULL;
  inner = newHV();
  ref = newRV_noinc((SV *)inner);
  if (readonly) SvREADONLY_on(ref);
  av_push((AV *)array, ref);
  return (SV *)inner;
}

void rbinkd_perl_set_bytes(PerlInterpreter *perl, SV *value, const char *text,
                          STRLEN len) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  sv_setpvn(value, text, len);
}

void rbinkd_perl_set_undef(PerlInterpreter *perl, SV *value) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  sv_setsv(value, &PL_sv_undef);
}

void rbinkd_perl_set_readonly(PerlInterpreter *perl, SV *value, int on) {
  PERL_SET_CONTEXT(perl);
  dTHX;
  if (on)
    SvREADONLY_on(value);
  else
    SvREADONLY_off(value);
}

/* The port of `perl_Log()`. */
static XS(perl_Log) {
  dXSARGS;
  STRLEN len;
  char *str;
  int lvl;

  if (items != 1 && items != 2) {
    char msg[128];
    snprintf(msg, sizeof msg,
             "wrong params number to Log (need 1 or 2, exist %d)", (int)items);
    rbinkd_perl_xs_log(1, msg);
    XSRETURN_EMPTY;
  }
  if (items == 2) {
    lvl = (int)SvUV(ST(0));
    str = SvPV(ST(1), len);
  } else {
    lvl = 3; /* LL_LOG */
    str = SvPV(ST(0), len);
  }
  if (len == 0) str = "";
  rbinkd_perl_xs_log(lvl, str);
  XSRETURN_EMPTY;
}

/* The port of `perl_aeq()`. */
static XS(perl_aeq) {
  dXSARGS;
  STRLEN len;
  int i;
  int rc;
  const char *first;
  const char **others;

  if (items < 2) {
    char msg[128];
    snprintf(msg, sizeof msg,
             "aeq() requires 2 or more parameters, %d exist", (int)items);
    rbinkd_perl_xs_log(1, msg);
    XSRETURN_UNDEF;
  }
  first = SvPV(ST(0), len);
  others = (const char **)malloc(sizeof(char *) * (size_t)(items - 1));
  if (!others) XSRETURN_UNDEF;
  for (i = 1; i < items; i++) {
    others[i - 1] = SvPV(ST(i), len);
  }
  rc = rbinkd_perl_xs_aeq(first, (size_t)(items - 1), others);
  free(others);
  if (rc < 0) XSRETURN_UNDEF;
  XSRETURN_IV(rc);
}

/* The port of `perl_arm()`. */
static XS(perl_arm) {
  dXSARGS;
  STRLEN len;
  I32 n;
  int i;
  int j;
  int removed;
  int retained_count;
  AV *arr;
  SV **svp;
  SV **retained = NULL;
  const char **elements = NULL;
  const char **args = NULL;
  int *keep = NULL;
  size_t count;

  if (items < 2) {
    char msg[128];
    snprintf(msg, sizeof msg,
             "arm() requires 2 or more parameters, %d exist", (int)items);
    rbinkd_perl_xs_log(1, msg);
    XSRETURN_UNDEF;
  }
  if (!SvROK(ST(0)) || SvTYPE(SvRV(ST(0))) != SVt_PVAV) {
    rbinkd_perl_xs_log(1, "first parameter to arm() should be array reference");
    XSRETURN_UNDEF;
  }
  arr = (AV *)SvRV(ST(0));
  n = av_len(arr) + 1;
  if (n < 0) n = 0;
  count = (size_t)(n > 0 ? n : 1);
  elements = (const char **)malloc(sizeof(char *) * count);
  keep = (int *)malloc(sizeof(int) * count);
  retained = (SV **)malloc(sizeof(SV *) * count);
  args = (const char **)malloc(sizeof(char *) * (size_t)(items - 1));
  if (!elements || !keep || !retained || !args) goto out;
  for (j = 0; j < n; j++) {
    svp = av_fetch(arr, j, FALSE);
    elements[j] = (svp && SvOK(*svp)) ? SvPV(*svp, len) : NULL;
  }
  for (i = 1; i < items; i++) {
    args[i - 1] = SvPV(ST(i), len);
  }
  removed =
      rbinkd_perl_xs_arm((size_t)n, elements, (size_t)(items - 1), args, keep);
  if (removed < 0) goto out;
  /* Keep the SVs of the retained elements alive across `av_clear()`. */
  retained_count = 0;
  for (j = 0; j < n; j++) {
    if (!keep[j]) continue;
    svp = av_fetch(arr, j, FALSE);
    if (svp) {
      retained[retained_count++] = SvREFCNT_inc(*svp);
    } else {
      retained[retained_count++] = SvREFCNT_inc(&PL_sv_undef);
    }
  }
  av_clear(arr);
  for (j = 0; j < retained_count; j++) {
    av_push(arr, retained[j]);
  }
  free(elements);
  free(args);
  free(keep);
  free(retained);
  XSRETURN_IV(removed);

out:
  free(elements);
  free(args);
  free(keep);
  free(retained);
  XSRETURN_UNDEF;
}

/* The port of `perl_msg_send()`. */
static XS(perl_msg_send) {
  dXSARGS;
  STRLEN len;
  char *str;

  if (items != 2) {
    char msg[128];
    snprintf(msg, sizeof msg,
             "wrong params number to msg_send (needs 2, exist %d)", (int)items);
    rbinkd_perl_xs_log(1, msg);
    XSRETURN_EMPTY;
  }
  str = SvPV(ST(1), len);
  if (len == 0) str = "";
  rbinkd_perl_xs_msg_send((int)SvIV(ST(0)), str);
  XSRETURN_EMPTY;
}

/* The port of `perl_warn()`, the warn and die hook `binkd_warn`. */
static XS(perl_warn) {
  dXSARGS;
  STRLEN len;
  char *str;

  if (items == 1) {
    str = SvPV(ST(0), len);
    rbinkd_perl_xs_warn(str);
  }
  XSRETURN_EMPTY;
}

/* The port of `xs_init()`. */
static void rbinkd_xs_init(pTHX) {
  static char *file = __FILE__;
  dXSUB_SYS;
  newXS("DynaLoader::boot_DynaLoader", boot_DynaLoader, file);
  newXS("Log", perl_Log, file);
  newXS("aeq", perl_aeq, file);
  newXS("arm", perl_arm, file);
  newXS("msg_send", perl_msg_send, file);
  newXS("binkd_warn", perl_warn, file);
}
