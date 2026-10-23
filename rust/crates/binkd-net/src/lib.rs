//! The network layer of binkd.
//!
//! The crate holds the port of `server.c` (the listen sockets and the accept
//! loop), of `client.c` (the outgoing call), of `iptools.c` (the port names and
//! the socket options), and of the host list of `readcfg.c` and `ftnaddr.c`. The
//! session itself is the port of `protocol.c` in `binkd-proto`, so the listener
//! and the caller hand the transport to it and never look inside.

#![forbid(unsafe_code)]

pub mod base64;
pub mod caller;
pub mod des;
mod des_tables;
pub mod dns;
pub mod hosts;
pub mod listener;
pub mod md4;
pub mod ntlm;
pub mod port;
pub mod proxy;
pub mod session;
pub mod srv;

pub use caller::{Call, CallError, Caller};
pub use hosts::{ftnaddress_to_domain, hosts, Host};
pub use listener::{bind, listen, ListenError};
pub use port::{find_port, BINKP, DEFAULT_PORT};
pub use session::{session_config, SessionOptions};
