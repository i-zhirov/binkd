//! The log of binkd.
//!
//! The crate holds the port of the log of `tools.c` (`InitLog()`, `vLog()`, and
//! the `nolog` filter) and of the binary session logs of `binlog.c`. The C code
//! keeps the state of the log in process wide variables and opens the log file for
//! every message; the port keeps the state in a value that the caller owns, so the
//! crate stays usable from several sessions.

#![forbid(unsafe_code)]

pub mod binlog;
pub mod level;
pub mod log;

pub use binlog::{write, BinLogConfig, SessionStat};
pub use level::{level_char, Level};
pub use log::{file_line, DateTime, Log, Outcome};
