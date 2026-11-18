//! The SRIF events of a session, the port of `srif.c`.
//!
//! SRIF is the Standard Request Information File, FSC-0086.001, a plain ASCII
//! file that the mailer writes for an external FREQ processor, the ERP. The mailer
//! writes the request file `.srf`, runs the ERP, and reads the response file
//! `.rsp`. The C code holds three features in one file:
//!
//! - The `flag` directive creates a flag file when a received file matches a
//!   pattern, the port of `evt_test()` and of the first branch of `evt_set()`.
//! - The `exec` directive runs an external program on a mail event, with the
//!   macros of the command expanded, the port of `run_args()`.
//! - The SRIF exchange itself: the `.srf` file of `srif_fill()` and the `.rsp`
//!   file of `parse_response()`.
//!
//! The C code passes the whole `STATE` of the session into `srif.c`, and the two
//! headers hold a cycle, one of the known exclusions of the plan. The port passes
//! the narrow value [`Session`] instead, so this crate never depends on the session
//! crate. The C code also runs the external program itself through `run()` of
//! `run.c`, which the plan puts into the binary crate: the port leaves the run to
//! the caller, so this crate holds the decisions and the caller holds the effects.

#![forbid(unsafe_code)]

pub mod args;
pub mod engine;
pub mod event;
pub mod srif;

use binkd_ftn::addr::FtnAddr;

/// The fields of a session that the SRIF code reads, the port of the parts of
/// `STATE` of `protoco2.h` that `srif.c` uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    /// The addresses of the peer, the port of `state->fa` and `state->nfa`.
    pub addresses: Vec<FtnAddr>,
    /// The session is protected, the port of `state->state == P_SECURE`.
    pub secure: bool,
    /// The peer is a listed node, the port of `state->listed_flag`.
    pub listed: bool,
    /// The name of the peer, the port of `state->peer_name`.
    pub peer_name: String,
    /// The address of the peer, the port of `state->ipaddr`. The C code
    /// answers it for a peer name that holds a character that it does not accept.
    pub ipaddr: String,
    /// The name of the sysop of the peer, the port of `state->sysop`.
    pub sysop: String,
}
