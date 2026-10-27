//! The binkd mailer: the command line, the version text, the platform
//! values, and the startup order, the port of `binkd.c` and of the platform
//! files of the C code.
//!
//! The library form is the product and the binary is a thin wrapper. The
//! caller of the library creates one value per instance, and no function of the
//! library ends the process. The port of this slice holds the command line and
//! the paths that end before the daemon: the daemon, the signals, the pid
//! file, the config reload, and the external program start arrive with the
//! next slices.

pub mod cli;
pub mod platform;
pub mod start;
pub mod version;
