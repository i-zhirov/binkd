//! The FFI of the Perl hooks of binkd, the port of `perlhooks.c`.
//!
//! The C code links the hooks only with `./configure --with-perl`, whose default
//! is no, and no test of the C tree names a hook. The crate is optional: the
//! default build of the workspace holds it not, and a build that wants the hooks
//! names the feature `perl` of the `binkd` crate. The crate alone holds the
//! exception of the lint `unsafe_code`, the decision of
//! `doc/decisions/perl-hooks.md`.
//!
//! The port follows the C `perlhooks.c` function by function, and the behaviour of
//! every hook is pinned by the cases of `crates/binkd/tests/perl.rs` against the
//! oracle of `--with-perl`.

pub mod bridge;
pub mod ffi;
pub mod hooks;

pub use hooks::Hooks;
