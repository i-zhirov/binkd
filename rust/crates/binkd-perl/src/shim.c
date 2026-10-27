/* The bridge of the stack of Perl, the port of the macros of the stack of
 * `perlhooks.c`.
 *
 * The C code of `perlhooks.c` holds the macros `dSP`, `PUSHMARK`, `SPAGAIN`,
 * and `POPs`, which a port cannot call. The shim holds one function for the
 * call of a sub in scalar context and one for the error of the interpreter, so
 * the crate of the FFI calls four functions of the bridge. */

#include <EXTERN.h>
#include <perl.h>

PerlInterpreter *binkd_perl_alloc(void) { return perl_alloc(); }

void binkd_perl_construct(PerlInterpreter *perl) { perl_construct(perl); }

int binkd_perl_parse(PerlInterpreter *perl, void *xsinit, int argc, char **argv,
                     char **env) {
  return perl_parse(perl, (XSINIT_t)xsinit, argc, argv, env);
}

int binkd_perl_run(PerlInterpreter *perl) { return perl_run(perl); }

void binkd_perl_free(PerlInterpreter *perl) {
  perl_destruct(perl);
  perl_free(perl);
}

SV *binkd_perl_get_sv(PerlInterpreter *perl, const char *name, int create) {
  dTHX;
  PERL_SET_CONTEXT(perl);
  return get_sv(name, create);
}

void binkd_perl_set_string(SV *value, const char *text) { sv_setpv(value, text); }

void binkd_perl_set_int(SV *value, IV number) { sv_setiv(value, number); }

IV binkd_perl_int(SV *value) { return SvIV(value); }

int binkd_perl_ok(SV *value) { return SvOK(value) ? 1 : 0; }

char *binkd_perl_string(SV *value, STRLEN *len) { return SvPV(value, *len); }

SV *binkd_perl_call(PerlInterpreter *perl, const char *name, int flags) {
  dTHX;
  SV *result;
  PERL_SET_CONTEXT(perl);
  dSP;
  ENTER;
  SAVETMPS;
  PUSHMARK(SP);
  PUTBACK;
  perl_call_pv(name, flags);
  SPAGAIN;
  result = POPs;
  PUTBACK;
  FREETMPS;
  LEAVE;
  return result;
}

int binkd_perl_error(PerlInterpreter *perl) {
  dTHX;
  PERL_SET_CONTEXT(perl);
  return SvTRUE(ERRSV) ? 1 : 0;
}
