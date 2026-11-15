//! The binkp protocol of binkd, the port of `protocol.c`.
//!
//! The crate drives a session over a transport that the caller supplies. It never
//! opens a socket and never reads the config file.

#![forbid(unsafe_code)]

pub mod bw;
pub mod compress;
pub mod frame;
pub mod hooks;
pub mod record;
pub mod session;

pub use compress::{Compressor, Decompressor, Method, Methods, ZRule};
pub use frame::{decode, read_frame, write_frame, Command, Frame};
pub use hooks::{
    CallVars, ErrorVars, Factory, FileVars, Handshake, Hooks, MessageSink, NoHooks, Opt, QueueFile,
    ReceivedVars, SentVars, SessionVars, Shared, BAD_AKA, BAD_AUTH, BAD_CALL, BAD_IO, BAD_MBSY,
    BAD_MERR, BAD_NA, BAD_TIMEOUT,
};
pub use record::{Direction, Recorder, Recording, Replay};
pub use session::{
    addrtype, run, run_with, run_with_hooks, skip_test, Error, FrameRecord, NoSrif, Peer, Protocol,
    ReceivedFile, Report, Role, SendFile, SessionConfig, SessionInfo, SkipRule, Srif,
};
