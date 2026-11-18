//! The declarations of the C API of Perl, the port of the calls of
//! `perlhooks.c`.
//!
//! The C code includes `perl.h` and it holds the macros of the stack of Perl,
//! `dSP`, `PUSHMARK`, `SPAGAIN`, and `POPs`, which a port cannot call. The crate
//! holds the four functions of the bridge of `shim.c` for the stack, and the
//! declarations of the API here.

use std::os::raw::{c_char, c_int};

use crate::bridge::{Interpreter, Length, Value};

extern "C" {
    /// Starts the interpreter library, the port of `PERL_SYS_INIT3()`.
    pub fn binkd_perl_sys_init();
    /// Creates an interpreter, the port of `perl_alloc()`.
    pub fn binkd_perl_alloc() -> *mut Interpreter;
    /// Builds an interpreter, the port of `perl_construct()`.
    pub fn binkd_perl_construct(perl: *mut Interpreter);
    /// Reads the arguments of an interpreter and registers the XS functions,
    /// the port of `perl_parse()` with `xs_init`.
    pub fn binkd_perl_parse(
        perl: *mut Interpreter,
        argc: c_int,
        argv: *mut *mut c_char,
        env: *mut *mut c_char,
    ) -> c_int;
    /// Runs the body of a script, the port of `perl_run()`.
    pub fn binkd_perl_run(perl: *mut Interpreter) -> c_int;
    /// Sets the warn and die hooks to `binkd_warn`, the port of the two
    /// `PL_warnhook` and `PL_diehook` lines of `perl_init()`.
    pub fn binkd_perl_set_warn_hook(perl: *mut Interpreter);
    /// Runs a script as `do 'file'`, the port of the `perl_eval_pv()` of
    /// `perl_init()`. Answers nonzero on an error.
    pub fn binkd_perl_do_file(perl: *mut Interpreter, path: *const c_char) -> c_int;
    /// Ends an interpreter, the port of `perl_destruct()` and of `perl_free()`.
    pub fn binkd_perl_free(perl: *mut Interpreter);
    /// Clones an interpreter, the port of `perl_clone()` of
    /// `perl_init_clone()`.
    pub fn binkd_perl_clone(perl: *mut Interpreter) -> *mut Interpreter;
    /// Ends a clone, the port of `perl_done_clone()`.
    pub fn binkd_perl_free_clone(perl: *mut Interpreter);
    /// Returns true when the script defines a sub, the port of the
    /// `perl_get_cv()` scan of `perl_init()`.
    pub fn binkd_perl_has_sub(perl: *mut Interpreter, name: *const c_char) -> c_int;
    /// Returns a global value, the port of `perl_get_sv()`.
    pub fn binkd_perl_get_sv(
        perl: *mut Interpreter,
        name: *const c_char,
        create: c_int,
    ) -> *mut Value;
    /// Sets a string value, the port of `sv_setpv()`.
    pub fn binkd_perl_set_string(perl: *mut Interpreter, value: *mut Value, text: *const c_char);
    /// Sets an integer value, the port of `sv_setiv()`.
    pub fn binkd_perl_set_int(perl: *mut Interpreter, value: *mut Value, number: i64);
    /// Returns the integer of a value, the port of `SvIV()`.
    pub fn binkd_perl_int(perl: *mut Interpreter, value: *mut Value) -> i64;
    /// Returns true for a defined value, the port of `SvOK()`.
    pub fn binkd_perl_ok(perl: *mut Interpreter, value: *mut Value) -> c_int;
    /// Returns the string of a value, the port of `SvPV()`.
    pub fn binkd_perl_string(
        perl: *mut Interpreter,
        value: *mut Value,
        len: *mut Length,
    ) -> *mut c_char;
    /// Calls a sub in scalar context, the port of `perl_call_pv()` with
    /// `G_EVAL|G_SCALAR`. Answers 1 when the value is defined and 0 when it is
    /// undefined, and writes the integer of the value to `result`.
    pub fn binkd_perl_call(
        perl: *mut Interpreter,
        name: *const c_char,
        flags: c_int,
        result: *mut i64,
    ) -> c_int;
    /// Returns true when the interpreter holds an error, the port of
    /// `SvTRUE(ERRSV)`.
    pub fn binkd_perl_error(perl: *mut Interpreter) -> c_int;
}

extern "C" {
    /// Calls a sub in void context, the port of `perl_call_pv()` with
    /// `G_EVAL|G_VOID`. Answers nonzero on an error.
    pub fn binkd_perl_call_void(perl: *mut Interpreter, name: *const c_char) -> c_int;
    /// Calls a sub in scalar context and copies the true string of the value,
    /// the port of the `SvTRUE` and `SvPV` reads of `perl_on_handshake()`.
    ///
    /// `defined` is 1 for a true string, 0 for a false or undefined value, and
    /// -1 for an error. The caller frees the string with
    /// `binkd_perl_free_string()`.
    pub fn binkd_perl_call_string(
        perl: *mut Interpreter,
        name: *const c_char,
        defined: *mut c_int,
        len: *mut Length,
    ) -> *mut c_char;
    /// Returns the error of the interpreter as a string, the port of
    /// `SvPV(ERRSV, len)`. The caller frees it with `binkd_perl_free_string()`.
    pub fn binkd_perl_error_string(perl: *mut Interpreter) -> *mut c_char;
    /// Frees a string of `binkd_perl_call_string()` or
    /// `binkd_perl_error_string()`.
    pub fn binkd_perl_free_string(text: *mut c_char);
    /// Returns a global hash value, the port of `perl_get_hv()`.
    pub fn binkd_perl_get_hv(
        perl: *mut Interpreter,
        name: *const c_char,
        create: c_int,
    ) -> *mut Value;
    /// Clears a hash, the port of `hv_clear()`.
    pub fn binkd_perl_hv_clear(perl: *mut Interpreter, hash: *mut Value);
    /// Stores an integer in a hash, the port of `hv_store()` with `newSViv()`.
    pub fn binkd_perl_hv_store_int(
        perl: *mut Interpreter,
        hash: *mut Value,
        key: *const c_char,
        value: i64,
    );
    /// Appends a hash of the fields of a queue entry, the port of the loop of
    /// `setup_queue()`.
    #[allow(clippy::too_many_arguments)]
    pub fn binkd_perl_av_push_hash(
        perl: *mut Interpreter,
        array: *mut Value,
        file: *const c_char,
        size: i64,
        time: i64,
        sent: i64,
        flvr: *const c_char,
        action: *const c_char,
        type_: *const c_char,
        addr: *const c_char,
    );
    /// Creates a hash, the port of `newHV()`.
    pub fn binkd_perl_hash_new(perl: *mut Interpreter) -> *mut Value;
    /// Creates an array, the port of `newAV()`.
    pub fn binkd_perl_array_new(perl: *mut Interpreter) -> *mut Value;
    /// Stores a string in a hash, the port of the `VK_ADD_HASH_str` macro.
    pub fn binkd_perl_hv_store_string(
        perl: *mut Interpreter,
        hash: *mut Value,
        key: *const c_char,
        value: *const c_char,
    );
    /// Stores an integer in a hash with the read-only flag, the port of the
    /// `VK_ADD_HASH_intz` macro.
    pub fn binkd_perl_hv_store_int_ro(
        perl: *mut Interpreter,
        hash: *mut Value,
        key: *const c_char,
        value: i64,
    );
    /// Stores an undefined value in a hash, the port of the undef branch of
    /// `VK_ADD_HASH_str`.
    pub fn binkd_perl_hv_store_undef(perl: *mut Interpreter, hash: *mut Value, key: *const c_char);
    /// Stores a new hash as a reference in a hash and answers the hash for the
    /// fields, the port of the `VK_ADD_HASH_sv` with `newHV()`.
    pub fn binkd_perl_hv_store_new_hash(
        perl: *mut Interpreter,
        hash: *mut Value,
        key: *const c_char,
        readonly: c_int,
    ) -> *mut Value;
    /// Stores a new array as a reference in a hash and answers the array.
    pub fn binkd_perl_hv_store_new_array(
        perl: *mut Interpreter,
        hash: *mut Value,
        key: *const c_char,
    ) -> *mut Value;
    /// Appends a read-only string to an array, the port of `av_push()` with
    /// `newSVpv()` and `SvREADONLY_on()`.
    pub fn binkd_perl_av_push_string_ro(
        perl: *mut Interpreter,
        array: *mut Value,
        text: *const c_char,
    );
    /// Appends a new hash as a reference to an array and answers the hash, the
    /// port of `av_push()` with `newHV()` and `newRV_noinc()`.
    pub fn binkd_perl_av_push_new_hash(
        perl: *mut Interpreter,
        array: *mut Value,
        readonly: c_int,
    ) -> *mut Value;
    /// Returns an array value, the port of `perl_get_av()`.
    pub fn binkd_perl_get_av(
        perl: *mut Interpreter,
        name: *const c_char,
        create: c_int,
    ) -> *mut Value;
    /// Clears an array, the port of `av_clear()`.
    pub fn binkd_perl_av_clear(perl: *mut Interpreter, array: *mut Value);
    /// Undefines an array, the port of `av_undef()`.
    pub fn binkd_perl_av_undef(perl: *mut Interpreter, array: *mut Value);
    /// Appends a string to an array, the port of `av_push()` with `newSVpv()`.
    pub fn binkd_perl_av_push_string(
        perl: *mut Interpreter,
        array: *mut Value,
        text: *const c_char,
    );
    /// Returns the last index of an array, the port of `av_len()`.
    pub fn binkd_perl_av_len(perl: *mut Interpreter, array: *mut Value) -> c_int;
    /// Returns a string of an array, the port of `av_fetch()` with `SvPV()`.
    ///
    /// `defined` is 1 for a value and 0 for an undefined entry. The caller
    /// frees the string with `binkd_perl_free_string()`.
    pub fn binkd_perl_av_fetch_string(
        perl: *mut Interpreter,
        array: *mut Value,
        index: c_int,
        defined: *mut c_int,
        len: *mut Length,
    ) -> *mut c_char;
    /// Returns the hash of an array entry, the port of the `SvROK` and
    /// `SvTYPE` tests of `refresh_queue()`.
    ///
    /// The answer is null when the entry holds no hash reference.
    pub fn binkd_perl_av_fetch_hash(
        perl: *mut Interpreter,
        array: *mut Value,
        index: c_int,
    ) -> *mut Value;
    /// Returns a string of a hash, the port of `hv_fetch()` with `SvPV()`.
    ///
    /// `defined` is 1 for a value and 0 for an undefined entry. The caller
    /// frees the string with `binkd_perl_free_string()`.
    pub fn binkd_perl_hv_fetch_string(
        perl: *mut Interpreter,
        hash: *mut Value,
        key: *const c_char,
        defined: *mut c_int,
        len: *mut Length,
    ) -> *mut c_char;
    /// Returns an integer of a hash, the port of `hv_fetch()` with `SvIV()`.
    ///
    /// `defined` is 1 for a value and 0 for an undefined entry.
    pub fn binkd_perl_hv_fetch_int(
        perl: *mut Interpreter,
        hash: *mut Value,
        key: *const c_char,
        defined: *mut c_int,
    ) -> i64;
    /// Sets a value to bytes, the port of `sv_setpvn()`.
    pub fn binkd_perl_set_bytes(
        perl: *mut Interpreter,
        value: *mut Value,
        text: *const c_char,
        len: Length,
    );
    /// Sets a value to undef, the port of `sv_setsv(sv, &sv_undef)`.
    pub fn binkd_perl_set_undef(perl: *mut Interpreter, value: *mut Value);
    /// Sets the read-only flag of a value, the port of `SvREADONLY_on/off()`.
    pub fn binkd_perl_set_readonly(perl: *mut Interpreter, value: *mut Value, on: c_int);
}
