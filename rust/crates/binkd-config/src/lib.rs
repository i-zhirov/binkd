//! The config of binkd, the port of `readcfg.c`.
//!
//! The crate reads the same config files as the C code. The parser returns a
//! `Config` value and never writes to a global.

#![forbid(unsafe_code)]

pub mod keywords;
pub mod model;
pub mod parser;
pub mod port;

pub use model::Config;
pub use parser::{parse, Error, Parser};
pub use port::PortResolver;
