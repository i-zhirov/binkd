//! The config of binkd, the port of `readcfg.c`.
//!
//! The crate reads the same config files as the C code. The parser returns a
//! `Config` value and never writes to a global.

#![forbid(unsafe_code)]

pub mod dump;
pub mod keywords;
pub mod model;
pub mod parser;
pub mod port;
pub mod reload;

pub use dump::{dump, dump_with};
pub use keywords::Features;
pub use model::Config;
pub use parser::{parse, Error, Parser};
pub use port::PortResolver;
pub use reload::{Outcome, Reloader};
