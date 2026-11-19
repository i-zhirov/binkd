//! The Windows service mode of binkd, the port of `nt/service.c`.
//!
//! The crate holds the `-t` and `-S` commands and the service dispatcher of the
//! Windows service control manager. The pure command values compile on every
//! system. The FFI compiles on Windows only, and the crate alone holds the
//! exception of the lint `unsafe_code`, the decision of
//! `doc/development.md`.

pub mod command;

#[cfg(windows)]
pub mod service;

#[cfg(windows)]
pub mod storage;
