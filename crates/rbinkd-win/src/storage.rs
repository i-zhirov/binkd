//! The safe file-system helpers of the Windows platform, the port of the
//! `GetShortPathName()` call of `run_args()` of `srif.c` and of the volume
//! identity of `stat()` that `check_config()` of `readcfg.c` uses.
//!
//! The raw calls stay in `rbinkd-win-ffi`. This crate holds only the safe
//! wrappers and keeps `unsafe_code = "forbid"`.

pub use rbinkd_win_ffi::{short_path, volume_serial};
