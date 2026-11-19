//! The Perl hooks of binkd, the port of `perlhooks.c`.
//!
//! The crate holds a Rust port of the hooks and a minimal C shim of the Perl C
//! API. The shim holds the macros of the stack of Perl, the lifecycle of the
//! interpreter, and the XS functions that call the Rust logic. The hook logic
//! follows `perlhooks.c` function by function.
//!
//! The C code links the hooks only with `./configure --with-perl`, whose default
//! is no, and no test of the C tree names a hook. The crate is optional: the
//! default build of the workspace holds it not, and a build that wants the hooks
//! names the feature `perl` of the `binkd` crate. The crate alone holds the
//! exception of the lint `unsafe_code`, the decision of
//! `doc/configuration.md`.
//!
//! The behaviour of every hook is pinned by the cases of
//! `crates/rbinkd/tests/perl.rs` against the oracle of `--with-perl`.

mod xs;

pub mod bridge;
pub mod ffi;
pub mod hooks;

pub use hooks::{
    Call, CallError, File, Handshake, Hooks, LogHook, LogSink, MessageSink, Opt, Options,
    QueueFile, Received, Sent, SessionVars,
};
