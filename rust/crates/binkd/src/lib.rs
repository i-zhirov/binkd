//! The binkd mailer: the command line, the version text, the platform
//! values, and the startup order, the port of `binkd.c` and of the platform
//! files of the C code.
//!
//! The library form is the product and the binary is a thin wrapper. The
//! caller of the library creates one value per instance, and no function of the
//! library ends the process or installs a signal handler. The crate holds the
//! command line, the startup order of the daemon, the managers of `server.c`
//! and of `client.c` through `Engine`, the config reload, the external program
//! start, and the Windows service values.

pub mod cli;
pub mod daemon;
pub mod engine;
#[cfg(feature = "perl")]
mod perl;
pub mod pipe;
pub mod platform;
pub mod start;
#[cfg(unix)]
pub(crate) mod syslog;
pub mod version;

pub use binkd_config::Config;
pub use binkd_ftn::addr::FtnAddr;
pub use binkd_proto::{Report, Role};
pub use engine::Engine;
