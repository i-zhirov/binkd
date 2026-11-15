//! The hooks of a session, the port of the `perl_on_*` calls of the C code.
//!
//! The session calls the methods of the trait at the points of the C code. The
//! crate states no dependency of the crate of the Perl hooks: the caller
//! supplies the trait object, and the binary implements it over the interpreter.

use std::sync::Arc;

use binkd_ftn::addr::FtnAddr;

use crate::frame::Command;

/// The session options of `%opt`, the port of the hash of `setup_session()`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Opt {
    /// The ND mode, the port of `$opt{ND}`.
    pub nd: i32,
    /// The NR mode, the port of `$opt{NR}`.
    pub nr: i32,
    /// The MD mode, the port of `$opt{MD}`.
    pub md: i32,
    /// The crypt mode, the port of `$opt{crypt}`.
    pub crypt: i32,
    /// The compression methods of the peer, the port of `$opt{GZ}`.
    pub gz: i32,
}

/// A file of the session queue, the port of an entry of `@queue`.
#[derive(Clone, Debug, Default)]
pub struct QueueFile {
    /// The path of the file, the port of `$queue[n]{file}`.
    pub file: String,
    /// The size, the port of `$queue[n]{size}`.
    pub size: u64,
    /// The time, the port of `$queue[n]{time}`.
    pub time: i64,
    /// The sent flag, the port of `$queue[n]{sent}`.
    pub sent: i32,
    /// The flavour, the port of `$queue[n]{flvr}`.
    pub flvr: char,
    /// The action, the port of `$queue[n]{action}`.
    pub action: char,
    /// The type, the port of `$queue[n]{type}`.
    pub type_: char,
    /// The address of the receiver, the port of `$queue[n]{addr}`.
    pub addr: FtnAddr,
}

/// The values of a session, the port of the parts of `STATE` that
/// `setup_session()` sets in the Perl variables.
#[derive(Clone, Debug, Default)]
pub struct SessionVars {
    /// The session answers a call, the port of `$call`.
    pub call: bool,
    /// The start time, the port of `$start`.
    pub start: i64,
    /// The name of the peer, the port of `$host`.
    pub host: String,
    /// The address of the peer, the port of `$ip`.
    pub ip: String,
    /// Our address, the port of `$our_ip`.
    pub our_ip: String,
    /// Our port, the port of `$our_port`.
    pub our_port: i32,
    /// The security state, the port of `$secure`.
    pub secure: i32,
    /// The name of the peer system, the port of `$sysname`.
    pub sysname: String,
    /// The sysop of the peer, the port of `$sysop`.
    pub sysop: String,
    /// The location of the peer, the port of `$location`.
    pub location: String,
    /// The mail traffic of the queue, the port of `$traf_mail`.
    pub traffic_mail: u64,
    /// The file traffic of the queue, the port of `$traf_file`.
    pub traffic_file: u64,
    /// The addresses of the peer, the port of `@he`.
    pub he: Vec<FtnAddr>,
    /// Our addresses, the port of `@me`.
    pub me: Vec<FtnAddr>,
    /// The send rate, the port of `$bw_send`.
    pub bw_send: i64,
    /// The receive rate, the port of `$bw_recv`.
    pub bw_recv: i64,
    /// The received bytes, the port of `$bytes_rcvd`.
    pub bytes_rcvd: u64,
    /// The sent bytes, the port of `$bytes_sent`.
    pub bytes_sent: u64,
    /// The received files, the port of `$files_rcvd`.
    pub files_rcvd: u64,
    /// The sent files, the port of `$files_sent`.
    pub files_sent: u64,
    /// The compression method of the sent files, the port of `$z_send`.
    pub z_send: i32,
    /// The compression method of the received files, the port of `$z_recv`.
    pub z_recv: i32,
    /// The session options, the port of `%opt`.
    pub opt: Opt,
    /// The session queue, the port of `@queue`.
    pub queue: Vec<QueueFile>,
}

/// The values of the hook of the handshake, the port of `perl_on_handshake()`.
#[derive(Clone, Debug, Default)]
pub struct Handshake {
    /// The session values.
    pub session: SessionVars,
    /// The addresses of the peer, the port of `@he`.
    pub he: Vec<FtnAddr>,
    /// Our addresses that the hook sets, the port of `@me`.
    pub me: Option<Vec<FtnAddr>>,
    /// The expected password that the hook sets, the port of `$passwd`.
    pub password: Option<String>,
    /// The mail traffic of the queue, the port of `$traf_mail`.
    pub traffic_mail: u64,
    /// The file traffic of the queue, the port of `$traf_file`.
    pub traffic_file: u64,
}

/// The values of the hook of an outgoing call, the port of `perl_on_call()`.
#[derive(Clone, Debug)]
pub struct CallVars {
    /// The address of the node, the port of `$addr`.
    pub addr: FtnAddr,
    /// The hosts of the node, the port of `$hosts`.
    pub hosts: String,
    /// The proxy, the port of `$proxy`.
    pub proxy: String,
    /// The SOCKS proxy, the port of `$socks`.
    pub socks: String,
    /// The mail traffic of the queue, the port of `$traf_mail`.
    pub traffic_mail: u64,
    /// The file traffic of the queue, the port of `$traf_file`.
    pub traffic_file: u64,
}

/// The `$where` values of `on_error()`, the port of `enum bad_try_type` of
/// `ftnq.h`.
pub const BAD_NA: i32 = 0;
/// The call failed, the port of `BAD_CALL`.
pub const BAD_CALL: i32 = 1;
/// The peer sent `M_ERR`, the port of `BAD_MERR`.
pub const BAD_MERR: i32 = 2;
/// The peer answered busy, the port of `BAD_MBSY`.
pub const BAD_MBSY: i32 = 3;
/// The transport or the local file failed, the port of `BAD_IO`.
pub const BAD_IO: i32 = 4;
/// The peer did not answer in time, the port of `BAD_TIMEOUT`.
pub const BAD_TIMEOUT: i32 = 5;
/// The peer holds no address that we need, the port of `BAD_AKA`.
pub const BAD_AKA: i32 = 6;
/// The password or the digest failed, the port of `BAD_AUTH`.
pub const BAD_AUTH: i32 = 7;

/// The values of the hook of a failed call, the port of `perl_on_error()`.
#[derive(Clone, Debug, Default)]
pub struct ErrorVars {
    /// The address of the node, the port of `$addr`.
    pub addr: FtnAddr,
    /// The error, the port of `$error`.
    pub error: String,
    /// The place of the error, the port of `$where`.
    pub location: i32,
}

/// The values of a file of a hook, the port of the `TFILE` reads.
#[derive(Clone, Debug, Default)]
pub struct FileVars {
    /// The path, the port of `$file`.
    pub path: String,
    /// The netname, the port of `$name`.
    pub name: String,
    /// The size, the port of `$size`.
    pub size: u64,
    /// The time, the port of `$time`.
    pub time: i64,
    /// The offset of a received file, the port of `$offs`.
    pub offset: u64,
}

/// The values of a received file, the port of `perl_after_recv()`.
#[derive(Clone, Debug, Default)]
pub struct ReceivedVars {
    /// The netname, the port of `$name`.
    pub name: String,
    /// The size, the port of `$size`.
    pub size: u64,
    /// The time, the port of `$time`.
    pub time: i64,
    /// The temporary name, the port of `$tmpfile`.
    pub tmp: String,
    /// The real name, the port of `$file`.
    pub file: String,
}

/// The values of a sent file, the port of `perl_after_sent()`.
#[derive(Clone, Debug, Default)]
pub struct SentVars {
    /// The path, the port of `$file`.
    pub path: String,
    /// The netname, the port of `$name`.
    pub name: String,
    /// The size, the port of `$size`.
    pub size: u64,
    /// The time, the port of `$time`.
    pub time: i64,
    /// The start time, the port of `$start`.
    pub start: i64,
    /// The action, the port of `$action`.
    pub action: char,
}

/// A sink of the messages that a hook queues, the port of `msg_send2()`.
///
/// The session supplies the sink and drains it after the hook, so a script may
/// queue a binkp message from any hook.
pub type MessageSink = Arc<dyn Fn(u8, &str) + Send + Sync>;

/// The hooks of a session, the port of the `perl_on_*` functions of
/// `perlhooks.c`.
///
/// Every method has a default that continues the session, so a caller may
/// implement one hook alone.
///
/// The session calls the hooks of one value on one task. A value MUST be
/// `Send + Sync`; a value that holds an interpreter serializes the calls
/// itself. The per-session values of the port keep the state of the session
/// separate, and the guards of the interpreter are per thread, not process
/// wide.
pub trait Hooks: Send + Sync {
    /// The hook of an outgoing call, the port of `perl_on_call()`.
    fn on_call(&self, _call: &mut CallVars) -> bool {
        true
    }

    /// The hook of a failed call, the port of `perl_on_error()`.
    fn on_error(&self, _error: &ErrorVars) {}

    /// The hook of the handshake, the port of `perl_on_handshake()`.
    fn on_handshake(&self, _vars: &mut Handshake) -> Option<String> {
        None
    }

    /// Reports whether the hooks hold the handshake hook, the port of the
    /// `PERL_ON_HANDSHAKE` bit of the C code.
    fn has_on_handshake(&self) -> bool {
        false
    }

    /// The hook after the handshake, the port of `perl_after_handshake()`.
    fn after_handshake(&self, _vars: &SessionVars) -> Option<String> {
        None
    }

    /// The hook after the session, the port of `perl_after_session()`.
    fn after_session(&self, _vars: &SessionVars, _ok: bool) {}

    /// The hook before a file is received, the port of `perl_before_recv()`.
    fn before_recv(&self, _vars: &SessionVars, _file: &FileVars) -> i32 {
        0
    }

    /// The hook after a file is received, the port of `perl_after_recv()`.
    fn after_recv(&self, _vars: &SessionVars, _file: &mut ReceivedVars) -> i32 {
        0
    }

    /// The hook before a file is sent, the port of `perl_before_send()`.
    fn before_send(&self, _vars: &SessionVars, _file: &mut FileVars) -> i32 {
        0
    }

    /// The hook after a file is sent, the port of `perl_after_sent()`.
    fn after_sent(&self, _vars: &SessionVars, _file: &mut SentVars) -> i32 {
        0
    }

    /// The hook of a sent message, the port of `perl_on_send()`.
    fn on_send(&self, _vars: &SessionVars, _command: Command, _s1: &str, _s2: &str) {}

    /// The hook of a received message, the port of `perl_on_recv()`.
    fn on_recv(&self, _vars: &SessionVars, _data: &[u8]) {}

    /// The hook of the rate limit, the port of `perl_setup_rlimit()`.
    fn setup_rlimit(&self, _vars: &SessionVars, _file: &str, _limit: &mut i64) {}

    /// Sets the sink of the messages that the script queues, the port of the
    /// `STATE` of `msg_send2()`. The session calls it before the hooks run.
    fn set_message_sink(&self, _sink: Option<MessageSink>) {}

    /// Reads the queue that a hook changed, the port of `refresh_queue()` of
    /// `perlhooks.c`.
    ///
    /// The answer is `None` when the hook holds no queue, and the session keeps
    /// its queue then. The `fallback` is the address of the peer, the port of
    /// `state->fa[0]` of the C code.
    fn refresh_queue(&self, _fallback: &FtnAddr) -> Option<Vec<QueueFile>> {
        None
    }
}

/// The hooks of a session that hold no script.
pub struct NoHooks;

impl Hooks for NoHooks {}

/// The hooks of a session, or none.
pub type Shared = Arc<dyn Hooks>;

/// A source of the hooks of a session, the port of `perl_init_clone()`.
///
/// The binary clones the interpreter for every session, so two sessions do not
/// share the variables of the script.
pub type Factory = Arc<dyn Fn() -> Option<Shared> + Send + Sync>;
