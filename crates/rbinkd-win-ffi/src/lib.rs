//! The raw Windows file-system calls of rbinkd.
//!
//! The crate holds the two calls that no stable safe standard API covers:
//! `GetShortPathNameW()` and `GetFileInformationByHandle()`. The crate
//! `rbinkd-win` accepts only the safe wrappers of this crate and sets
//! `unsafe_code = "forbid"`.

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{short_path, volume_serial};
