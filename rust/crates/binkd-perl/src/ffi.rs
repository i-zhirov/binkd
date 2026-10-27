//! The declarations of the C API of Perl, the port of the calls of
//! `perlhooks.c`.
//!
//! The C code includes `perl.h` and it holds the macros of the stack of Perl,
//! `dSP`, `PUSHMARK`, `SPAGAIN`, and `POPs`, which a port cannot call. The crate
//! holds the four functions of the bridge of `shim.c` for the stack, and the
//! declarations of the API here.

use std::os::raw::{c_char, c_int, c_void};

use crate::bridge::{Interpreter, Length, Value};

extern "C" {
    /// Creates an interpreter, the port of `perl_alloc()`.
    pub fn binkd_perl_alloc() -> *mut Interpreter;
    /// Builds an interpreter, the port of `perl_construct()`.
    pub fn binkd_perl_construct(perl: *mut Interpreter);
    /// Reads the arguments of an interpreter, the port of `perl_parse()`.
    pub fn binkd_perl_parse(
        perl: *mut Interpreter,
        xsinit: *mut c_void,
        argc: c_int,
        argv: *mut *mut c_char,
        env: *mut *mut c_char,
    ) -> c_int;
    /// Runs the body of a script, the port of `perl_run()`.
    pub fn binkd_perl_run(perl: *mut Interpreter) -> c_int;
    /// Ends an interpreter, the port of `perl_destruct()` and of `perl_free()`.
    pub fn binkd_perl_free(perl: *mut Interpreter);
    /// Returns a global value, the port of `perl_get_sv()`.
    pub fn binkd_perl_get_sv(
        perl: *mut Interpreter,
        name: *const c_char,
        create: c_int,
    ) -> *mut Value;
    /// Sets a string value, the port of `sv_setpv()`.
    pub fn binkd_perl_set_string(value: *mut Value, text: *const c_char);
    /// Sets an integer value, the port of `sv_setiv()`.
    pub fn binkd_perl_set_int(value: *mut Value, number: i64);
    /// Returns the integer of a value, the port of `SvIV()`.
    pub fn binkd_perl_int(value: *mut Value) -> i64;
    /// Returns true for a defined value, the port of `SvOK()`.
    pub fn binkd_perl_ok(value: *mut Value) -> c_int;
    /// Returns the string of a value, the port of `SvPV()`.
    pub fn binkd_perl_string(value: *mut Value, len: *mut Length) -> *mut c_char;
    /// Calls a sub in scalar context, the port of `perl_call_pv()` with
    /// `G_EVAL|G_SCALAR`, and it answers the value of the sub.
    pub fn binkd_perl_call(
        perl: *mut Interpreter,
        name: *const c_char,
        flags: c_int,
    ) -> *mut Value;
    /// Returns true when the interpreter holds an error, the port of
    /// `SvTRUE(ERRSV)`.
    pub fn binkd_perl_error(perl: *mut Interpreter) -> c_int;
}
