//! The four bridge types of the FFI, the port of the types of `perlhooks.c`.

use std::os::raw::{c_char, c_int};

/// The interpreter of Perl, the port of `PerlInterpreter *` of `perlhooks.c`.
#[repr(C)]
pub struct Interpreter {
    _private: [u8; 0],
}

/// A scalar value of Perl, the port of `SV *` of `perlhooks.c`.
#[repr(C)]
pub struct Value {
    _private: [u8; 0],
}

/// The length of a value, the port of `STRLEN` of `perlhooks.c`.
pub type Length = usize;

/// The table of the names of the hooks, the port of `perl_subnames[]` of
/// `perlhooks.c`.
///
/// The C code holds the name of every hook in one table and it calls the sub of
/// the name with `perl_call_pv()`. The port holds the table, the port of the
/// function by function of the module.
pub const SUBNAMES: [&str; 17] = [
    "on_start",
    "on_exit",
    "on_call",
    "on_error",
    "on_handshake",
    "after_handshake",
    "after_session",
    "before_recv",
    "after_recv",
    "before_send",
    "after_sent",
    "on_log",
    "on_send",
    "on_recv",
    "setup_rlimit",
    "need_reload",
    "config_loaded",
];

/// The index of the hook of the log, the port of `PERL_ON_LOG`.
pub const ON_LOG: usize = 11;

/// The flags of a call of a sub, the port of `G_EVAL|G_SCALAR` of `perlhooks.c`.
pub const G_EVAL: c_int = 0x80;
/// The scalar context of a call of a sub, the port of `G_SCALAR`.
pub const G_SCALAR: c_int = 0x01;

/// The type of the name of a sub, the port of the argument of `perl_call_pv()`.
pub type SubName = *const c_char;
