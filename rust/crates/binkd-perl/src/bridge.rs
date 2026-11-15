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

/// The indices of the hooks, the port of the `perl_subs` enum of
/// `perlhooks.c`.
pub const ON_START: usize = 0;
pub const ON_EXIT: usize = 1;
pub const ON_CALL: usize = 2;
pub const ON_ERROR: usize = 3;
pub const ON_HANDSHAKE: usize = 4;
pub const AFTER_HANDSHAKE: usize = 5;
pub const AFTER_SESSION: usize = 6;
pub const BEFORE_RECV: usize = 7;
pub const AFTER_RECV: usize = 8;
pub const BEFORE_SEND: usize = 9;
pub const AFTER_SENT: usize = 10;
pub const ON_LOG: usize = 11;
pub const ON_SEND: usize = 12;
pub const ON_RECV: usize = 13;
pub const SETUP_RLIMIT: usize = 14;
pub const NEED_RELOAD: usize = 15;
pub const CONFIG_LOADED: usize = 16;

/// The eval flag of a call of a sub, the port of `G_EVAL` of `perlhooks.c`.
pub const G_EVAL: c_int = 0x8;
/// The scalar context of a call of a sub, the port of `G_SCALAR`.
pub const G_SCALAR: c_int = 0x2;

/// The type of the name of a sub, the port of the argument of `perl_call_pv()`.
pub type SubName = *const c_char;
