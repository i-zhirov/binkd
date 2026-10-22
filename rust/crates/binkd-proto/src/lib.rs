//! The binkp protocol of binkd, the port of `protocol.c`.
//!
//! The crate drives a session over a transport that the caller supplies. It never
//! opens a socket and never reads the config file.

#![forbid(unsafe_code)]

pub mod compress;
pub mod frame;
pub mod session;

pub use compress::{Compressor, Decompressor, Method, Methods, ZRule};
pub use frame::{decode, read_frame, write_frame, Command, Frame};
pub use session::{run, Error, ReceivedFile, Report, Role, SendFile, SessionConfig};
