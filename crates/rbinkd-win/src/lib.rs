//! The Windows service mode of binkd, the port of `nt/service.c`.
//!
//! The crate holds the `-t` and `-S` commands and the service dispatcher of the
//! Windows service control manager. The pure command values compile on every
//! system. The crate uses the safe APIs of `windows-service` and `winreg` and
//! keeps `unsafe_code = "forbid"`. The raw file-system calls live in
//! `rbinkd-win-ffi`.

pub mod command;

#[cfg(windows)]
pub mod service;

#[cfg(windows)]
pub mod storage;
