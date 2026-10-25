//! The binkp session, the port of `protocol.c`.
//!
//! The C code keeps the state of a session in the `STATE` value and the process
//! wide flags in `common.h`. The port keeps the state in the session value and
//! drives the frames over a transport that the caller supplies.

use std::fmt;
use std::fs;
use std::future::poll_fn;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::Poll;

use binkd_ftn::addr::FtnAddr;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::bw;
use crate::compress::{self, Compressor, Decompressor, Method, Methods, ZRule, BLOCK_SIZE};
use crate::frame::{self, Command, Frame};
use binkd_spool::inbound::{FreeSpace, TFile};

/// The role of the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// We called the peer.
    Client,
    /// The peer called us.
    Server,
}

/// The settings of a session.
#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// `sysname`: the name of the system.
    pub sysname: String,
    /// `sysop`: the name of the sysop.
    pub sysop: String,
    /// `location`: the location of the system.
    pub location: String,
    /// `nodeinfo`: the information about the system.
    pub nodeinfo: String,
    /// Our addresses.
    pub addresses: Vec<FtnAddr>,
    /// The password of the session, or empty.
    pub password: String,
    /// The inbound directory of the session.
    pub inbound: PathBuf,
    /// The name and the version of the mailer of the banner.
    pub version: String,
    /// The largest data block that we send, the port of `oblksize`.
    pub blksize: usize,
    /// The smallest file that we compress, the port of `zminsize`.
    pub zminsize: u64,
    /// The zlib compression level, the port of `zlevel`.
    pub zlevel: i32,
    /// The rules of the files that we compress, the port of the `zallow` and
    /// `zdeny` directives. No matching rule denies the file.
    pub zrules: Vec<ZRule>,
    /// The compression methods of this side, the port of `z_canrecv`.
    pub methods: Methods,
    /// Crypt the traffic of the session, the port of `no_crypt`. The session
    /// crypts only when both sides request it.
    pub crypt: bool,
    /// We ask for the NR mode, the port of `to->NR_flag == NR_ON` and of
    /// `WANT_NR` of `protocol.c`. The `-nr` and `-nd` options of the node
    /// set it.
    pub nr: bool,
    /// We ask for the ND mode, the port of `to->ND_flag == ND_ON` and of
    /// `THEY_ND` of `protocol.c`. The `-nd` option of the node sets it,
    /// and the option implies `nr`.
    pub nd: bool,
    /// The domains of the spool, for the link status files of the ND mode.
    pub domains: binkd_ftn::domain::DomainTable,
    /// The spool of the session, for the partial files of a transfer.
    pub spool: binkd_spool::config::SpoolConfig,
    /// The rules of the `skip` directive, the port of `skipmask`.
    pub skip: Vec<SkipRule>,
    /// The node of the peer is listed in the config, the port of
    /// `state->listed_flag` of `skip_test()`.
    pub listed: bool,
    /// The largest number of sessions that may run at once, the port of
    /// `max_servers` of `server.c`. A negative value holds no limit.
    pub max_servers: i32,
    /// The number of the sessions that run, the port of `n_servers` of
    /// `server.c`. The listener counts the sessions.
    pub servers: usize,
    /// The session holds the frames of its log, the port of the frame log of the
    /// C code at `loglevel 10`. The log of a crypted session holds the frames
    /// before the encryption, as the C code logs them.
    pub trace: bool,
    /// The version of the protocol that the session advertises, the port of
    /// `PRTCLVER` of `Config.h`.
    pub protocol: Protocol,
    /// The rules of the `limit-rate` directive, the port of `config->rates`.
    pub rates: Vec<bw::RateRule>,
    /// The `-bw` send limit of the node, the port of `to->bw_send`.
    pub bw_send: i64,
    /// The `-bw` recv limit of the node, the port of `to->bw_recv`.
    pub bw_recv: i64,
    /// The `prescan` directive: scan the outbound after the login and announce
    /// the queue, the port of `config->prescan`.
    pub prescan: bool,
}

/// The version of the protocol that the session advertises, the port of
/// `PRTCLVER` of `Config.h`.
///
/// The C code holds the version at `1.1`, and it answers the version of the peer
/// in `state->major` and `state->minor`, so a peer of the version `1.0` makes the
/// session behave as `1.0`. The port answers `peer_11` for that, and this value
/// states what the port itself advertises. The C code takes `PRTCLVER` from the
/// makefile, and the tests of the `1.0` path build the C binary with the define.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Protocol {
    /// The version `1.0`, `PRTCLVER` of the C code before the answer of the peer.
    V10,
    /// The version `1.1`, the value of `PRTCLVER` of the C code.
    #[default]
    V11,
}

impl Protocol {
    /// Returns the text of the version.
    pub fn name(&self) -> &'static str {
        match self {
            Protocol::V10 => "1.0",
            Protocol::V11 => "1.1",
        }
    }
}

/// The fields of the session that the SRIF events read, the port of the parts of
/// `STATE` of `protoco2.h` that `srif.c` uses.
///
/// The C code passes the whole state of the session into `srif.c`, and the two
/// headers hold a cycle, one of the known exclusions of the plan. The port passes
/// this narrow value, and the adapter of `binkd-net` adds the name and the address
/// of the peer, which the transport knows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Peer {
    /// The addresses of the peer, the port of `state->fa` and `state->nfa`.
    pub addresses: Vec<FtnAddr>,
    /// The session is protected, the port of `state->state == P_SECURE`.
    pub secure: bool,
    /// The peer is a listed node, the port of `state->listed_flag`.
    pub listed: bool,
    /// The name of the sysop of the peer, the port of `state->sysop`.
    pub sysop: String,
}

/// The SRIF events of a session, the port of the calls of `protocol.c` to
/// `srif.c`.
///
/// The C code calls `srif.c` from four points: the `M_NUL FREQ` frame of the peer,
/// the end of a batch of the peer, the end of a received file, and the end of the
/// session. The port takes the events as a trait, so `binkd-proto` never depends on
/// `binkd-srif` and the caller wires the engine, the port of option A of the plan.
pub trait Srif {
    /// Takes a FREQ claim of the peer, the port of the `M_NUL FREQ` frame.
    fn claim(&mut self);
    /// Takes the end of a batch of the peer, the port of the `M_EOB` frame.
    fn end_of_batch(&mut self);
    /// Takes a received file and answers the paths of the files to send, the port
    /// of the call of `inb_done()` of `inbound.c`.
    fn on_file(&mut self, filename: &str, peer: &Peer) -> Vec<String>;
    /// Takes the end of a batch and answers the paths of the files to send, the
    /// port of `process_rcvdlist()` of `protocol.c`.
    fn flush(&mut self, peer: &Peer) -> Vec<String>;
    /// Reports whether the peer claims a FREQ, the port of `delay_EOB > 0`.
    fn claimed(&self) -> bool;
    /// Takes the end of the session, the port of `evt_set()` of `srif.c`.
    fn finish(&mut self);
}

/// The SRIF events of a session without an engine, the port of a build of the C
/// code without a `flag` or an `exec` entry.
#[derive(Clone, Debug, Default)]
pub struct NoSrif;

impl Srif for NoSrif {
    fn claim(&mut self) {}
    fn end_of_batch(&mut self) {}
    fn on_file(&mut self, _filename: &str, _peer: &Peer) -> Vec<String> {
        Vec::new()
    }
    fn flush(&mut self, _peer: &Peer) -> Vec<String> {
        Vec::new()
    }
    fn claimed(&self) -> bool {
        false
    }
    fn finish(&mut self) {}
}

/// A file to send.
#[derive(Clone, Debug)]
pub struct SendFile {
    /// The name of the file on the wire.
    pub netname: String,
    /// The path of the file. `None` is a link status of the ND mode: the
    /// announcement holds no data, and the offset is the size.
    pub path: Option<PathBuf>,
    /// The size in bytes.
    pub size: u64,
    /// The time of the file.
    pub time: i64,
}

/// A file that was received.
#[derive(Clone, Debug)]
pub struct ReceivedFile {
    /// The name of the file on the wire.
    pub netname: String,
    /// The path of the file in the inbound directory.
    pub path: PathBuf,
    /// The size in bytes.
    pub size: u64,
    /// The time of the file.
    pub time: i64,
    /// The compression method of the file, or `None`.
    pub method: Option<Method>,
}

/// The report of a session.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// The files that we sent.
    pub files_sent: usize,
    /// The files that we received.
    pub files_received: usize,
    /// The bytes that we sent.
    pub bytes_sent: u64,
    /// The bytes that we received.
    pub bytes_received: u64,
    /// The addresses of the peer.
    pub peer_addresses: Vec<FtnAddr>,
    /// The files that we received.
    pub received: Vec<ReceivedFile>,
    /// The frames of the session, when the settings hold the trace.
    pub frames: Vec<FrameRecord>,
    /// The session crypts its traffic, the port of `YES_CRYPT`.
    pub crypt: bool,
    /// The rules of the `skip` directive, the port of `skipmask`.
    pub skip: Vec<SkipRule>,
    /// The node of the peer is listed in the config, the port of
    /// `state->listed_flag` of `skip_test()`.
    pub listed: bool,
    /// The largest number of sessions that may run at once, the port of
    /// `max_servers` of `server.c`. A negative value holds no limit.
    pub max_servers: i32,
    /// The number of the sessions that run, the port of `n_servers` of
    /// `server.c`. The listener counts the sessions.
    pub servers: usize,
    /// The session holds the frames of its log, the port of the frame log of the
    /// C code at `loglevel 10`. The log of a crypted session holds the frames
    /// before the encryption, as the C code logs them.
    pub trace: bool,
    /// The version of the protocol that the session advertises, the port of
    /// `PRTCLVER` of `Config.h`.
    pub protocol: Protocol,
    /// The rules of the `limit-rate` directive, the port of `config->rates`.
    pub rates: Vec<bw::RateRule>,
    /// The `-bw` send limit of the node, the port of `to->bw_send`.
    pub bw_send: i64,
    /// The `-bw` recv limit of the node, the port of `to->bw_recv`.
    pub bw_recv: i64,
    /// The peer skipped a file of the session, the port of
    /// `state->r_skipped_flag`. The C code sends the queue of the session
    /// only while the flag is clear, the port of `OK_SEND_FILES()`.
    pub skipped: bool,
}

/// The NR mode of a session, the port of `NR_flag` of `protoco2.h`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NrMode {
    /// `WANT_NR`: we ask for the mode.
    pub want: bool,
    /// `WE_NR`: we announce the files with the offset -1.
    pub we: bool,
    /// `THEY_NR`: the peer is the NR sender.
    pub they: bool,
}

/// The ND mode of a session, the port of `ND_flag` of `protoco2.h`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NdMode {
    /// `WE_ND`: we wait for `M_GOT` after each file.
    pub we: bool,
    /// `THEY_ND`: the peer waits for `M_GOT`.
    pub they: bool,
    /// `CAN_NDA`: the peer holds the asymmetric mode.
    pub can_nda: bool,
}

/// One frame of the log of a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameRecord {
    /// The frame left our side.
    pub sent: bool,
    /// The stable name of the frame.
    pub name: String,
}

/// The address types of the `skip` rules, the port of `addrtype` of
/// `readcfg.h`.
pub mod addrtype {
    /// Every address type.
    pub const A_ALL: i32 = -1;
    /// A listed node.
    pub const A_LST: i32 = 1;
    /// An unlisted node.
    pub const A_UNLST: i32 = 2;
    /// A secure session.
    pub const A_PROT: i32 = 4;
    /// A non-secure session.
    pub const A_UNPROT: i32 = 8;
}

/// One rule of the `skip` directive, the port of `struct skipchain` of
/// `readcfg.h`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkipRule {
    /// The mask.
    pub mask: String,
    /// The address type.
    pub atype: i32,
    /// The size in bytes, or -1.
    pub size: i64,
    /// The file is deleted, not skipped.
    pub destr: bool,
}

/// Tests the skip rules, the port of `skip_test()` of `protocol.c`.
///
/// The first rule that matches the address type and the name decides. A rule with
/// a size of 0 or more matches a file of that size or more only.
pub fn skip_test<'a>(
    rules: &'a [SkipRule],
    atype: i32,
    netname: &str,
    size: u64,
) -> Option<&'a SkipRule> {
    for rule in rules {
        if rule.atype & atype != 0
            && binkd_ftn::pattern::pmatch_ncase(rule.mask.as_bytes(), netname.as_bytes())
        {
            if rule.size >= 0 && size >= rule.size as u64 {
                return Some(rule);
            }
            return None;
        }
    }
    None
}

/// A session error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The transport failed.
    Io(String),
    /// The peer sent `M_ERR`.
    Peer(String),
    /// The peer is busy.
    Busy(String),
    /// The peer sent a frame that the state does not allow.
    Protocol(String),
    /// The password is wrong.
    BadPassword,
    /// The peer did not answer.
    Timeout,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "the transport failed: {e}"),
            Error::Peer(e) => write!(f, "the peer reported an error: {e}"),
            Error::Busy(e) => write!(f, "the peer is busy: {e}"),
            Error::Protocol(e) => write!(f, "the protocol failed: {e}"),
            Error::BadPassword => write!(f, "the password is wrong"),
            Error::Timeout => write!(f, "the peer did not answer"),
        }
    }
}

impl std::error::Error for Error {}

/// Runs a binkp session.
///
/// The port of `protocol()`. The caller supplies the transport, so the session runs
/// over a socket, a pipe, or a memory buffer.
pub async fn run<S>(
    stream: S,
    role: Role,
    config: &SessionConfig,
    files: &[SendFile],
    receive: &mut (dyn FnMut(&ReceivedFile) -> std::io::Result<()> + Send),
    timeout: std::time::Duration,
) -> Result<Report, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    run_with(
        stream,
        role,
        config,
        &mut |_| files.to_vec(),
        receive,
        &mut NoSrif,
        timeout,
    )
    .await
}

/// Runs a binkp session with the files of the peer from a closure.
///
/// The port of `protocol()`: the answerer scans the queue for the peer after
/// the login, when the addresses of the peer are known, so the caller may give
/// the files at that moment.
pub async fn run_with<S>(
    stream: S,
    role: Role,
    config: &SessionConfig,
    files: &mut (dyn FnMut(&[FtnAddr]) -> Vec<SendFile> + Send),
    receive: &mut (dyn FnMut(&ReceivedFile) -> std::io::Result<()> + Send),
    srif: &mut (dyn Srif + Send),
    timeout: std::time::Duration,
) -> Result<Report, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut session = Session::new(stream, role, config);
    session.handshake(srif, timeout).await?;
    // The port of the `maxservers` check of `protocol()`: the C code answers
    // `M_BSY` after the banner, and only for a session that the peer called.
    if role == Role::Server
        && config.max_servers >= 0
        && config.servers > config.max_servers as usize
    {
        let _ = session.send_text(Command::Bsy, "Too many servers").await;
        return Err(Error::Busy("too many servers".to_string()));
    }
    // The port of the part of `ADR()` that reads the `-bw` of the nodes.
    session.setup_session_limits();
    let files = files(&session.report.peer_addresses);
    session.send_files(&files, srif, timeout).await?;
    session.receive_files(receive, srif, timeout).await?;
    // The port of `evt_set()`: the events that wait for the end of the session
    // run here, and the files that an exchange answered go last.
    srif.finish();
    session.report.crypt = session.crypt.is_some();
    Ok(session.report)
}

/// The state of a session.
struct Session<S> {
    stream: S,
    role: Role,
    config: SessionConfig,
    report: Report,
    /// The challenge of CRAM-MD5, or `None`.
    challenge: Option<Vec<u8>>,
    /// The peer holds binkp/1.1.
    peer_11: bool,
    /// The compression methods of the peer, the port of `z_cansend`.
    peer_methods: Methods,
    /// The peer advertised `EXTCMD`.
    extcmd: bool,
    /// We request `CRYPT`, the port of `WE_CRYPT`.
    we_crypt: bool,
    /// The peer requested `CRYPT`, the port of `THEY_CRYPT`.
    they_crypt: bool,
    /// We authenticated with CRAM-MD5, the port of `MD_flag`.
    md5: bool,
    /// The encryption of the session, or `None`, the port of `YES_CRYPT`.
    crypt: Option<Crypt>,
    /// The inbound of the session, the port of the inbound of `STATE`.
    inbound: binkd_spool::inbound::Inbound,
    /// We sent the end of the batch.
    sent_eob: bool,
    /// The NR mode of the session, the port of `NR_flag`.
    nr: NrMode,
    /// The ND mode of the session, the port of `ND_flag`.
    nd: NdMode,
    /// We announced a file and wait for the answer of the peer, the port of
    /// `off_req_sent`.
    off_req_sent: bool,
    /// The files that we sent, the port of `sent_fls`. The peer may ask for
    /// one of them with `M_GET`, or decline it with `M_SKIP`.
    sent_fls: Vec<SentFile>,
    /// The requests that we sent and the peer did not answer yet, the port of
    /// `GET_FILE_balance`.
    get_file_balance: usize,
    /// The file that the receiver holds for the deferred rename, the port of
    /// `in_complete` of `protocol.c`.
    in_complete: Option<CompleteFile>,
    /// The command frames of the batch, the port of `msgs_in_batch`. The C code
    /// starts the next batch when the count passes the two `M_EOB` frames.
    msgs_in_batch: usize,
    /// The peer sent the end of the batch.
    peer_eob: bool,
    /// The meter of the send direction, the port of `state->bw_send`.
    bw_send: bw::Bandwidth,
    /// The meter of the recv direction, the port of `state->bw_recv`.
    bw_recv: bw::Bandwidth,
    /// The bytes that the session read ahead, the port of the read buffer of the
    /// C code, `state->ibuf`.
    pending: Vec<u8>,
    /// The header of a frame that waits for its data, the port of the read of the
    /// header of `recv_block()` of the C code, which reads the header, decrypts
    /// it, and knows the length of the data then.
    pending_header: Option<(u16, usize)>,
    /// The files that an exchange answered, the port of the queue of the session
    /// of the C code, `state->q`, which the send path sends.
    answered: Vec<String>,
    /// The files that the prescan of the session found, the port of the queue of
    /// the session of the C code, `state->q`, which `do_prescan()` builds and
    /// the send path sends.
    prescan_files: Vec<SendFile>,
}

impl<S> Session<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn new(stream: S, role: Role, config: &SessionConfig) -> Session<S> {
        // The port of `init_protocol()`: the inbound of the session.
        let inbound = binkd_spool::inbound::Inbound {
            dir: config.inbound.display().to_string(),
            secure: !config.password.is_empty() && config.password != "!",
            // Our addresses of the session, the port of `state->fa`.
            fa: config.addresses.clone(),
            nfa: config.addresses.len(),
            skip_all_flag: false,
        };
        Session {
            stream,
            role,
            config: config.clone(),
            report: Report::default(),
            challenge: None,
            peer_11: false,
            peer_methods: Methods::none(),
            extcmd: false,
            we_crypt: config.crypt,
            they_crypt: false,
            md5: false,
            crypt: None,
            inbound,
            sent_eob: false,
            nr: NrMode {
                want: config.nr,
                we: false,
                they: false,
            },
            nd: NdMode {
                we: false,
                they: config.nd,
                can_nda: false,
            },
            off_req_sent: false,
            sent_fls: Vec::new(),
            get_file_balance: 0,
            in_complete: None,
            msgs_in_batch: 0,
            peer_eob: false,
            bw_send: bw::Bandwidth::new(0, 0),
            bw_recv: bw::Bandwidth::new(0, 0),
            pending: Vec::new(),
            pending_header: None,
            answered: Vec::new(),
            prescan_files: Vec::new(),
        }
    }

    /// Sends a frame.
    ///
    /// The port of `msg_send2()`: the whole frame, the header included, is
    /// encrypted when the session crypts.
    async fn send(&mut self, frame: Frame) -> Result<(), Error> {
        if matches!(frame, Frame::Command(..)) {
            // The port of `msg_send2()`: the batch counts the messages.
            self.msgs_in_batch += 1;
        }
        if self.config.trace {
            if let Some(name) = frame.log_name() {
                self.report.frames.push(FrameRecord { sent: true, name });
            }
        }
        let mut data = frame.encode();
        if let Some(crypt) = &mut self.crypt {
            binkd_ftn::crypt::encrypt_buf(&mut data, &mut crypt.keys_out);
        }
        // The port of the meter of `send_block()`: the wait of the limit comes
        // before the write, and the bytes of the frame come after it.
        self.wait_rate(true).await;
        self.stream
            .write_all(&data)
            .await
            .map_err(|e| Error::Io(e.to_string()))?;
        self.bw_send.bytes += data.len() as u64;
        self.stream
            .flush()
            .await
            .map_err(|e| Error::Io(e.to_string()))
    }

    /// Sends a command with a text argument.
    async fn send_text(&mut self, command: Command, text: &str) -> Result<(), Error> {
        self.send(Frame::text(command, text)).await
    }

    /// Reads a frame.
    ///
    /// The port of `recv_block()`: the header and the payload are decrypted
    /// before the frame is decoded.
    async fn read(&mut self, timeout: std::time::Duration) -> Result<Frame, Error> {
        loop {
            if let Some(frame) = self.take_frame()? {
                return Ok(frame);
            }
            self.read_more(timeout).await?;
        }
    }

    async fn try_read_frame(&mut self) -> Result<Option<Frame>, Error> {
        loop {
            if let Some(frame) = self.take_frame()? {
                return Ok(Some(frame));
            }
            let mut buf = [0u8; 4096];
            let mut filled = tokio::io::ReadBuf::new(&mut buf);
            let mut polled = None;
            poll_fn(
                |cx| match Pin::new(&mut self.stream).poll_read(cx, &mut filled) {
                    Poll::Ready(result) => {
                        polled = Some(result);
                        Poll::Ready(())
                    }
                    Poll::Pending => Poll::Ready(()),
                },
            )
            .await;
            let Some(result) = polled else {
                return Ok(None);
            };
            result.map_err(|e| Error::Io(e.to_string()))?;
            let n = filled.filled().len();
            if n == 0 {
                return Err(Error::Io("the peer closed the transport".to_string()));
            }
            self.pending.extend_from_slice(filled.filled());
        }
    }

    fn take_frame(&mut self) -> Result<Option<Frame>, Error> {
        let (raw, size) = match self.pending_header {
            Some(header) => header,
            None => {
                if self.pending.len() < frame::HEADER_SIZE {
                    return Ok(None);
                }
                let mut header: Vec<u8> = self.pending.drain(..frame::HEADER_SIZE).collect();
                if let Some(crypt) = &mut self.crypt {
                    binkd_ftn::crypt::decrypt_buf(&mut header, &mut crypt.keys_in);
                }
                let raw = u16::from_be_bytes([header[0], header[1]]);
                let size = Frame::payload_size(raw);
                self.pending_header = Some((raw, size));
                (raw, size)
            }
        };
        if self.pending.len() < size {
            return Ok(None);
        }
        let mut payload: Vec<u8> = self.pending.drain(..size).collect();
        if let Some(crypt) = &mut self.crypt {
            binkd_ftn::crypt::decrypt_buf(&mut payload, &mut crypt.keys_in);
        }
        self.pending_header = None;
        let frame = frame::decode(raw, payload).map_err(|e| Error::Io(e.to_string()))?;
        if matches!(frame, Frame::Command(..)) {
            self.msgs_in_batch += 1;
        }
        if self.config.trace {
            if let Some(name) = frame.log_name() {
                self.report.frames.push(FrameRecord { sent: false, name });
            }
        }
        Ok(Some(frame))
    }

    async fn read_more(&mut self, timeout: std::time::Duration) -> Result<(), Error> {
        let mut buf = [0u8; 4096];
        self.wait_rate(false).await;
        match tokio::time::timeout(timeout, self.stream.read(&mut buf)).await {
            Ok(Ok(0)) => Err(Error::Io("the peer closed the transport".to_string())),
            Ok(Ok(n)) => {
                self.bw_recv.bytes += n as u64;
                self.pending.extend_from_slice(&buf[..n]);
                Ok(())
            }
            Ok(Err(e)) => Err(Error::Io(e.to_string())),
            Err(_) => Err(Error::Timeout),
        }
    }

    /// Waits while a direction is over the rate limit, the port of the part of the
    /// `select()` loop of `protocol()`.
    async fn wait_rate(&mut self, send: bool) {
        let bw = if send {
            &mut self.bw_send
        } else {
            &mut self.bw_recv
        };
        if let Some(delay) = bw.check(std::time::Instant::now()) {
            tokio::time::sleep(delay).await;
        }
    }

    /// Sets the session limits, the port of the part of `ADR()` that reads the
    /// `-bw` of the nodes of the peer.
    fn setup_session_limits(&mut self) {
        let (send, recv) =
            bw::session_limits(self.config.spool.nodes.nodes(), &self.report.peer_addresses);
        self.bw_send.abs = send.0;
        self.bw_send.rel = send.1;
        self.bw_recv.abs = recv.0;
        self.bw_recv.rel = recv.1;
    }

    /// Starts the limit of a file, the port of `setup_rate_limit()`.
    ///
    /// The C code matches the rules of `limit-rate` in the order of the config,
    /// and it takes the first rule whose address type and mask match the file.
    fn setup_rate_limit(&mut self, send: bool, name: &str) {
        let amask = self.addr_type();
        let mut rate = -100;
        for rule in &self.config.rates {
            if rule.atype & amask != 0
                && binkd_ftn::pattern::pmatch_ncase(rule.mask.as_bytes(), name.as_bytes())
            {
                rate = rule.rate;
                break;
            }
        }
        let bw = if send {
            &mut self.bw_send
        } else {
            &mut self.bw_recv
        };
        bw.limit = bw::file_limit(rate, bw.abs, bw.rel);
        bw.start();
    }

    /// Runs the handshake of the port of `banner()` and the command handlers.
    ///
    /// Both sides send the banner and the addresses. The server sends its password
    /// and waits for the password of the caller; a side without a password completes
    /// the login at once. The login is complete when the `M_OK` arrives.
    async fn handshake(
        &mut self,
        srif: &mut (dyn Srif + Send),
        timeout: std::time::Duration,
    ) -> Result<(), Error> {
        // The answerer sends the challenge of CRAM-MD5 as its first message,
        // the port of `banner()`.
        if self.role == Role::Server && self.secure() {
            let challenge = binkd_ftn::cram::generate_challenge(b"");
            // The first byte of the challenge is its length.
            let digest = binkd_ftn::cram::to_string(&challenge[1..]);
            self.send_text(Command::Nul, &format!("OPT {digest}"))
                .await?;
            self.challenge = Some(challenge);
        }

        // The banner of both sides.
        let sysname = self.config.sysname.clone();
        self.send_text(Command::Nul, &format!("SYS {sysname}"))
            .await?;
        let sysop = self.config.sysop.clone();
        self.send_text(Command::Nul, &format!("ZYZ {sysop}"))
            .await?;
        let location = self.config.location.clone();
        self.send_text(Command::Nul, &format!("LOC {location}"))
            .await?;
        let nodeinfo = self.config.nodeinfo.clone();
        self.send_text(Command::Nul, &format!("NDL {nodeinfo}"))
            .await?;
        let version = self.config.version.clone();
        let protocol = self.config.protocol.name();
        self.send_text(Command::Nul, &format!("VER {version} binkp/{protocol}"))
            .await?;
        // The options of the caller, the port of the `OPT` message of
        // `banner()`. The answerer sends them with the state of the login,
        // the port of `PWD()`.
        if self.role == Role::Client {
            self.send_options().await?;
        }
        let addresses: Vec<String> = self
            .config
            .addresses
            .iter()
            .map(|fa| fa.format(true))
            .collect();
        self.send_text(Command::Adr, &addresses.join(" ")).await?;

        // The frames of the peer up to the login acknowledgement.
        loop {
            match self.read(timeout).await? {
                Frame::Command(Command::Nul, argument) => self.handle_nul(&argument, srif),
                Frame::Command(Command::Adr, argument) => {
                    self.report.peer_addresses = parse_addresses(&argument);
                    match self.role {
                        // The caller answers the password request of the
                        // answerer, the port of `ADR()`.
                        Role::Client => {
                            // The port of the login of the caller: the C
                            // code scans the queue and announces it before
                            // the password, the port of `do_prescan()`.
                            self.prescan().await?;
                            self.send_password().await?
                        }
                        // The answerer waits for the password of the caller,
                        // the port of `PWD()`.
                        Role::Server => {}
                    }
                }
                Frame::Command(Command::Pwd, argument) => {
                    // The answerer checks the password of the caller, the port
                    // of `PWD()`.
                    if !self.check_password(&argument) {
                        let _ = self.send_text(Command::Err, "Incorrect password").await;
                        return Err(Error::BadPassword);
                    }
                    // The port of `MD_flag`: the caller authenticated with
                    // the digest.
                    self.md5 |= text(&argument).starts_with("CRAM-");
                    if self.role == Role::Server {
                        self.finish_login().await?;
                        return Ok(());
                    }
                }
                Frame::Command(Command::Ok, argument) => {
                    // The port of `OK()`: the answerer reports the state of
                    // the session.
                    if text(&argument).contains("non-secure") {
                        self.we_crypt = false;
                    }
                    self.finish_login().await?;
                    return Ok(());
                }
                Frame::Command(Command::Err, argument) => {
                    return Err(peer_error(&argument));
                }
                Frame::Command(Command::Bsy, argument) => {
                    return Err(Error::Busy(text(&argument)));
                }
                other => {
                    return Err(Error::Protocol(format!("{other:?} before the login")));
                }
            }
        }
    }

    /// Sends the password of the caller, the port of `ADR()`.
    ///
    /// The caller answers the challenge with the digest of CRAM-MD5. Without a
    /// challenge it sends the password.
    async fn send_password(&mut self) -> Result<(), Error> {
        // The port of `ADR()`: a caller without a password sends `-`.
        let password = if self.config.password.is_empty() {
            "-".to_string()
        } else {
            self.config.password.clone()
        };
        let answer = match &self.challenge {
            Some(challenge) => {
                // The port of `MD_flag`: we authenticated with MD5.
                self.md5 = true;
                binkd_ftn::cram::build_digest(password.as_bytes(), challenge).unwrap_or(password)
            }
            None => password,
        };
        self.send_text(Command::Pwd, &answer).await
    }

    /// Scans the spool for the peer and announces the queue, the port of
    /// `do_prescan()` of `protocol.c`.
    ///
    /// The C code runs the scan after the login of both roles, before the answer
    /// of the password, and the send path sends the queue that the scan builds,
    /// the port of `state->q`. The C code announces the claim of a request file
    /// only for a peer of the version 1.0, the port of the test of
    /// `state->major * 100 + state->minor <= 100` of `do_prescan()`.
    async fn prescan(&mut self) -> Result<(), Error> {
        if !self.config.prescan || !self.can_send_files() {
            return Ok(());
        }
        let mut queue = binkd_spool::queue::Queue::new();
        queue.scan_addrs(
            &self.config.addresses,
            self.role == Role::Client,
            &mut self.config.spool,
            now(),
            false,
        );
        let (netsize, filessize) = queue.get_sizes();
        self.send_text(Command::Nul, &format!("TRF {netsize} {filessize}"))
            .await?;
        // The C code announces one claim per request file of the queue, the
        // port of `q_freq_num()` of `ftnq.c`.
        if !self.peer_11 {
            for _ in 0..queue.freq_num() {
                self.send_text(Command::Nul, "FREQ").await?;
            }
        }
        self.prescan_files = queue
            .entries
            .iter()
            .map(|entry| queued_file(&entry.path.to_string_lossy()))
            .collect();
        Ok(())
    }

    /// Reports whether the session may send the files of its queue, the port of
    /// `OK_SEND_FILES()` of `protoco2.h`.
    ///
    /// The C code holds the flag of a file that the peer skipped,
    /// `state->r_skipped_flag`, which the port does not model yet.
    fn can_send_files(&self) -> bool {
        (!self.config.spool.send_if_pwd || self.secure() || self.role == Role::Client)
            && !self.report.skipped
    }

    /// Completes the login, the port of `complete_login()`.
    ///
    /// The answerer sends its options and the state of the session. Both sides
    /// turn the encryption on when the session is secure, the peer requested
    /// the mode, we requested the mode, and the password is a real one.
    async fn finish_login(&mut self) -> Result<(), Error> {
        if self.role == Role::Server {
            // The port of the ND fixups of `PWD()`: the answerer tells
            // the caller about the ND mode of the node.
            if self.nd.we && !self.nd.can_nda {
                self.nd.they = true;
            }
            if !self.nd.we && !self.nd.can_nda {
                self.nd.they = false;
            }
            // The port of `PWD()`: the answerer scans the queue and
            // announces it before the options and the answer.
            self.prescan().await?;
            self.send_options().await?;
            let state = if self.secure() {
                "secure"
            } else {
                "non-secure"
            };
            self.send_text(Command::Ok, state).await?;
        }
        // The port of `complete_login()`: the batch forgets the messages of
        // the login.
        self.msgs_in_batch = 0;
        // The port of `complete_login()`: the ND mode implies the NR mode.
        if self.nd.we {
            self.nr.we = true;
        }
        if self.role == Role::Client {
            // The port of `OK()`: the caller keeps the ND mode only when
            // both sides hold it, because the peer may not support the
            // asymmetric mode.
            if self.nd.we != self.nd.they {
                self.nd.we = false;
                self.nd.they = false;
            }
        }
        if self.we_crypt && self.they_crypt && self.secure() && self.md5 && !self.any_password() {
            self.crypt = Some(Crypt::new(&self.config.password, self.role));
        }
        Ok(())
    }

    /// Sends the options of this side, the port of the `OPT` message of
    /// `banner()` and `PWD()`.
    async fn send_options(&mut self) -> Result<(), Error> {
        let mut options = vec!["EXTCMD".to_string()];
        options.extend(self.config.methods.names().iter().map(|n| n.to_string()));
        if self.role == Role::Client {
            // The port of the `OPT` message of `banner()`: the caller
            // announces `NDA`, asks for the NR mode, and tells the peer
            // about the ND mode of the node.
            options.push("NDA".to_string());
            if self.nr.want {
                options.push("NR".to_string());
            }
            if self.nd.they {
                options.push("ND".to_string());
            }
        } else {
            // The port of the `OPT` message of `PWD()`: the answerer
            // echoes the modes and adds `NDA` for the asymmetric mode.
            if self.nr.want {
                options.push("NR".to_string());
            }
            if self.nd.they {
                options.push("ND".to_string());
            }
            if self.nd.we != self.nd.they {
                options.push("NDA".to_string());
            }
        }
        // The port of `PWD()`: the answerer echoes `CRYPT` only when the
        // caller requested it.
        if self.we_crypt && (self.role == Role::Client || self.they_crypt) {
            options.push("CRYPT".to_string());
        }
        self.send_text(Command::Nul, &format!("OPT {}", options.join(" ")))
            .await
    }

    /// Reports whether the password of the session was used, the port of
    /// `P_SECURE`.
    fn secure(&self) -> bool {
        !self.config.password.is_empty() && !self.any_password()
    }

    /// Returns the address type of the session, the port of `amask` of
    /// `skip_test()` of `protocol.c`.
    fn addr_type(&self) -> i32 {
        let mut amask = if self.config.listed {
            addrtype::A_LST
        } else {
            addrtype::A_UNLST
        };
        amask |= if self.secure() {
            addrtype::A_PROT
        } else {
            addrtype::A_UNPROT
        };
        amask
    }

    /// Reports whether the password accepts any value, the port of the `!`
    /// password of the C code.
    fn any_password(&self) -> bool {
        self.config.password == "!"
    }

    /// Returns the fields of the session that the SRIF events read.
    fn peer(&self) -> Peer {
        Peer {
            addresses: self.report.peer_addresses.clone(),
            secure: self.secure(),
            listed: self.config.listed,
            sysop: self.config.sysop.clone(),
        }
    }

    /// Reads the options of a `M_NUL` frame.
    fn handle_nul(&mut self, argument: &[u8], srif: &mut (dyn Srif + Send)) {
        let text = text(argument);
        let mut words = text.split_whitespace();
        match words.next() {
            Some("OPT") => {
                for option in words {
                    if option == "NR" {
                        // The port of `NUL()`: the peer wants the NR
                        // mode, so we announce the files.
                        self.nr.we = true;
                    } else if option == "ND" {
                        // The port of `NUL()`: the peer wants the ND
                        // mode, so we wait for `M_GOT`.
                        self.nd.we = true;
                    } else if option == "NDA" {
                        // The port of `NUL()`: the peer holds the
                        // asymmetric ND mode.
                        self.nd.can_nda = true;
                    } else if option.starts_with(binkd_ftn::cram::PREFIX) {
                        // The port of `MD_getChallenge()`: the first byte of
                        // the challenge is its length.
                        self.challenge = binkd_ftn::cram::parse(option);
                    } else if let Some(method) = Method::from_name(option) {
                        // The port of the `NUL()` handler: the peer can
                        // receive the method.
                        self.peer_methods.insert(method);
                    } else if option == "EXTCMD" {
                        // The port of `state->extcmd`: the peer may ask us
                        // to compress a file.
                        self.extcmd = true;
                    } else if option == "CRYPT" {
                        // The port of the `NUL()` handler: the peer wants
                        // the crypt mode.
                        self.they_crypt = true;
                    }
                }
            }
            // The port of `NUL()`: the peer claims a FREQ for us, the port
            // of `state->delay_EOB++`. The C code holds the claim in a
            // message of its own, not among the options.
            Some("FREQ") => srif.claim(),
            Some("VER") => {
                self.peer_11 = text.contains("binkp/1.1");
            }
            _ => {}
        }
    }

    /// Checks the password of the peer.
    fn check_password(&self, argument: &[u8]) -> bool {
        if self.config.password.is_empty() || self.any_password() {
            return true;
        }
        let given = text(argument);
        if given == "-" {
            // The port of `PWD()`: `-` means no password.
            return true;
        }
        if let Some(challenge) = &self.challenge {
            let digest = binkd_ftn::cram::build_digest(self.config.password.as_bytes(), challenge);
            return digest.as_deref() == Some(given.as_str());
        }
        given == self.config.password
    }

    /// Sends the files and the end of the batch.
    async fn send_files(
        &mut self,
        files: &[SendFile],
        srif: &mut (dyn Srif + Send),
        timeout: std::time::Duration,
    ) -> Result<(), Error> {
        // The port of `complete_login()`: the C code scans the queue of the
        // session when the prescan did not, and the send path sends the queue.
        let files: Vec<SendFile> = if self.prescan_files.is_empty() {
            files.to_vec()
        } else {
            std::mem::take(&mut self.prescan_files)
        };
        for file in &files {
            self.send_queued(file, srif, timeout).await?;
        }
        self.send_pending(srif, timeout).await?;
        Ok(())
    }

    async fn send_pending(
        &mut self,
        srif: &mut (dyn Srif + Send),
        timeout: std::time::Duration,
    ) -> Result<(), Error> {
        for path in std::mem::take(&mut self.answered) {
            let file = queued_file(&path);
            self.send_queued(&file, srif, timeout).await?;
        }
        if self.sent_eob || !self.sent_fls.is_empty() || self.get_file_balance > 0 {
            return Ok(());
        }
        // The C code holds the end of the batch back only for a peer of the
        // version 1.0 that claims a FREQ, the port of `delay_EOB` of
        // `protocol.c`. A peer of the version 1.1 receives our end at once.
        if srif.claimed() && !self.peer_11 {
            return Ok(());
        }
        let _ = self.send_text(Command::Eob, "").await;
        self.sent_eob = true;
        Ok(())
    }

    async fn handle_frame(
        &mut self,
        frame: Frame,
        receive: &mut (dyn FnMut(&ReceivedFile) -> std::io::Result<()> + Send),
        srif: &mut (dyn Srif + Send),
        current: &mut Option<Incoming>,
    ) -> Result<(), Error> {
        match frame {
            Frame::Command(Command::File, argument) => {
                // The port of the deferred rename of `start_file_recv()`:
                // the completed file takes its true name before the next
                // file starts.
                if let Some(complete) = self.in_complete.take() {
                    self.finish_file(complete, receive, srif).await?;
                }
                let (netname, size, time, offset, extras) = parse_file_args(&argument)?;
                if offset == NR_REQUEST {
                    // The port of `start_file_recv()`: the peer is the
                    // NR sender, and it asks us for the file.
                    self.nr.they = true;
                    let _ = self
                        .send_text(Command::Get, &format!("{netname} {size} {time} 0"))
                        .await;
                    self.get_file_balance += 1;
                    return Ok(());
                }
                // The port of the `M_FILE` handler: the answer of the
                // peer ends a request that we sent.
                if self.nr.they && self.get_file_balance > 0 {
                    self.get_file_balance -= 1;
                }
                // The port of the `M_FILE` handler: the extra arguments
                // select the compression of the file.
                let mut methods = Methods::none();
                for extra in &extras {
                    if let Some(method) = Method::from_name(extra) {
                        methods.insert(method);
                    }
                }
                if methods.contains(Method::Gz) && methods.contains(Method::Bz2) {
                    let _ = self
                        .send_text(
                            Command::Err,
                            &format!("Can't handle GZ and BZ2 at the same time for {netname}"),
                        )
                        .await;
                    return Err(Error::Protocol(format!(
                        "both GZ and BZ2 extras are specified for {netname}"
                    )));
                }
                // The port of `skip_test()` of `start_file_recv()`: a
                // rule of the `skip` directive declines the file.
                let amask = self.addr_type();
                if let Some(rule) = skip_test(&self.config.skip, amask, &netname, size) {
                    let _ = self
                        .send_text(
                            if rule.destr {
                                Command::Got
                            } else {
                                Command::Skip
                            },
                            &format!("{netname} {size} {time}"),
                        )
                        .await;
                    return Ok(());
                }
                // The port of `inb_test()` of `start_file_recv()`: we
                // hold the file already.
                let now = now();
                let mut rnd = random();
                let tfile = TFile {
                    netname: netname.clone(),
                    size,
                    time,
                    start: now,
                };
                if self
                    .inbound
                    .inb_test(&netname, size, time, self.config.spool.renamestyle)
                    .is_some()
                {
                    let _ = self
                        .send_text(Command::Got, &format!("{netname} {size} {time}"))
                        .await;
                    return Ok(());
                }
                // The port of `inb_fopen()` of `start_file_recv()`: the
                // partial file of the transfer.
                let file = self.inbound.inb_fopen(
                    &tfile,
                    &self.config.spool,
                    FreeSpace::default(),
                    now,
                    &mut rnd,
                );
                let Some(file) = file else {
                    let _ = self
                        .send_text(Command::Skip, &format!("{netname} {size} {time}"))
                        .await;
                    return Ok(());
                };
                let received = file.metadata().map(|m| m.len()).unwrap_or(0);
                if offset != received as i64 {
                    // The port of `start_file_recv()`: we ask for the
                    // file from the position of the partial file, and we
                    // forget the partial that we hold.
                    let _ = self
                        .send_text(Command::Get, &format!("{netname} {size} {time} {received}"))
                        .await;
                    self.get_file_balance += 1;
                    return Ok(());
                } else if offset != 0 || self.nr.they {
                    self.get_file_balance = self.get_file_balance.saturating_sub(1);
                }
                // The port of `start_file_recv()`: the rate of the file
                // starts before the data.
                self.setup_rate_limit(false, &netname);
                *current = Some(Incoming {
                    tfile,
                    file,
                    method: methods.preferred(),
                    decompressor: None,
                    received,
                });
            }
            Frame::Data(data) => {
                let Some(mut file) = current.take() else {
                    // The C code discards a block without a file.
                    return Ok(());
                };
                // The port of the compressed branch of `recv_block()`.
                let bytes = match file.method {
                    None => data,
                    Some(method) => {
                        if file.decompressor.is_none() {
                            file.decompressor = Some(Decompressor::new(method));
                        }
                        let mut out = Vec::new();
                        let mut input = &data[..];
                        loop {
                            let decompressor = file.decompressor.as_mut().unwrap();
                            let step = decompressor.decompress(input, BLOCK_SIZE).map_err(|e| {
                                Error::Protocol(format!(
                                    "decompression of {} failed: {e}",
                                    file.tfile.netname
                                ))
                            })?;
                            out.extend_from_slice(&step.data);
                            input = &input[step.consumed..];
                            if step.ended {
                                // The C code ends the stream and starts a
                                // new one for the next block.
                                file.decompressor = None;
                                break;
                            }
                            // A stream may hold input inside itself and
                            // produce no output for a moment.
                            if step.consumed == 0 && step.data.is_empty() {
                                break;
                            }
                        }
                        out
                    }
                };
                // The port of the `fwrite()` of `recv_block()`: the
                // bytes of the file go to the partial file.
                if std::io::Write::write_all(&mut file.file, &bytes).is_err() {
                    return Err(Error::Protocol(format!(
                        "cannot write {}",
                        file.tfile.netname
                    )));
                }
                file.received += bytes.len() as u64;
                if file.received > file.tfile.size {
                    return Err(Error::Protocol(format!(
                        "the file {} holds {} extra bytes",
                        file.tfile.netname,
                        file.received - file.tfile.size
                    )));
                }
                if file.received == file.tfile.size {
                    let complete = CompleteFile {
                        tfile: file.tfile,
                        method: file.method,
                    };
                    let _ = self
                        .send_text(
                            Command::Got,
                            &format!(
                                "{} {} {}",
                                complete.tfile.netname, complete.tfile.size, complete.tfile.time
                            ),
                        )
                        .await;
                    self.report.files_received += 1;
                    self.report.bytes_received += complete.tfile.size;
                    if self.nd.they {
                        // The port of the `THEY_ND` branch of
                        // `recv_block()`: the rename waits for the
                        // next file or for the end of the batch.
                        self.in_complete = Some(complete);
                    } else {
                        self.finish_file(complete, receive, srif).await?;
                    }
                } else {
                    *current = Some(file);
                }
            }
            Frame::Command(Command::Nul, argument) => {
                // The C code reads an option of the peer at any time.
                self.handle_nul(&argument, srif);
            }
            Frame::Command(Command::Got, argument) => {
                // The port of `GOT()`: the peer acknowledged a file.
                let (netname, size, time) = parse_sent_args(&argument)?;
                let mut address = None;
                for file in &self.sent_fls {
                    if file.matches(&netname, size, time) {
                        address = file.fa.clone();
                        break;
                    }
                }
                self.sent_fls
                    .retain(|file| !file.matches(&netname, size, time));
                // The port of `GOT()`: a request of the NR mode clears
                // the link status, an ND sender saves the answer of the
                // peer as the new status.
                if let Some(fa) = address {
                    if self.off_req_sent || !self.nd.we {
                        let _ = binkd_spool::status::set_status(&self.config.domains, &fa, "");
                    } else {
                        let _ = binkd_spool::status::set_status(
                            &self.config.domains,
                            &fa,
                            &text(&argument),
                        );
                    }
                }
            }
            Frame::Command(Command::Get, argument) => {
                self.get_file(&argument).await?;
            }
            Frame::Command(Command::Skip, argument) => {
                self.skip_file(&argument)?;
            }
            Frame::Command(Command::Ok, _) => {
                // The login of the peer completes with our `M_OK`.
            }
            Frame::Command(Command::Pwd, argument) => {
                // The peer asks for the password after the login.
                if !self.check_password(&argument) {
                    self.send_text(Command::Err, "Incorrect password").await?;
                    return Err(Error::BadPassword);
                }
                self.send_text(Command::Ok, "").await?;
            }
            Frame::Command(Command::Eob, _) => {
                self.peer_eob = true;
                // The port of `EOB()`: the peer answered every claim of
                // the batch, so the session answers the exchange now.
                srif.end_of_batch();
                // The port of `EOB()`: a partial file of a peer of the
                // NR mode stays for the next session, and the port
                // removes it otherwise.
                if let Some(file) = current.take() {
                    if !self.nr.they {
                        let mut rnd = random();
                        self.inbound
                            .inb_reject(&file.tfile, &self.config.spool, now(), &mut rnd);
                    }
                }
                // The port of `EOB()`: the completed file takes its
                // true name at the end of the batch.
                if let Some(complete) = self.in_complete.take() {
                    self.finish_file(complete, receive, srif).await?;
                }
            }
            Frame::Command(Command::Err, argument) => {
                return Err(peer_error(&argument));
            }
            Frame::Command(Command::Bsy, argument) => {
                return Err(Error::Busy(text(&argument)));
            }
            other => {
                return Err(Error::Protocol(format!("{other:?} after the login")));
            }
        }
        Ok(())
    }

    /// Sends one file with the bookkeeping of `current_file_was_sent()`.
    ///
    /// The C code sends the files of the queue of the session, which holds the
    /// files of a batch and the files of an SRIF exchange alike.
    async fn send_queued(
        &mut self,
        file: &SendFile,
        srif: &mut (dyn Srif + Send),
        timeout: std::time::Duration,
    ) -> Result<(), Error> {
        // The port of `start_file_transfer()`: the name of the file on the
        // wire, the port of `netname_()` of `prothlp.c`.
        let mut rnd = rnd();
        let wire = binkd_ftn::text::netname(file.netname.as_bytes(), &mut rnd);
        let file = SendFile {
            netname: String::from_utf8_lossy(&wire).into_owned(),
            ..file.clone()
        };
        let file = &file;
        let method = self.send_method(file);
        self.send_file(file, method).await?;
        self.report.files_sent += 1;
        self.report.bytes_sent += file.size;
        // The port of `current_file_was_sent()`: the peer may ask for
        // the file with `M_GET` or decline it with `M_SKIP`, and it
        // acknowledges it with `M_GOT`.
        self.sent_fls.push(SentFile {
            netname: file.netname.clone(),
            size: file.size,
            time: file.time,
            path: file.path.clone(),
            method,
            fa: self.report.peer_addresses.first().cloned(),
        });
        if self.nd.we {
            // The port of the `waiting_for_GOT` wait of
            // `current_file_was_sent()`: an ND sender starts the next
            // file only after the peer acknowledged this one.
            self.wait_for_got(srif, timeout).await?;
        }
        Ok(())
    }

    /// Returns the compression method of a file, the port of `z_send_init()`.
    ///
    /// The peer must hold `EXTCMD` and the method, the file must be large
    /// enough, and a rule must allow the name. bzip2 is preferred over zlib.
    fn send_method(&self, file: &SendFile) -> Option<Method> {
        if !self.extcmd || file.size < self.config.zminsize {
            return None;
        }
        if !compress::allows(&self.config.zrules, &file.netname) {
            return None;
        }
        self.peer_methods.preferred_between(self.config.methods)
    }

    /// Sends one file, the port of `start_file_transfer()` and `send_block()`.
    ///
    /// The `M_FILE` frame holds the size of the file, not the size of the
    /// compressed data. The extra argument holds the method.
    async fn send_file(&mut self, file: &SendFile, method: Option<Method>) -> Result<(), Error> {
        // The port of `start_file_transfer()`: the rate of the file starts
        // before the announcement of the file.
        self.setup_rate_limit(true, &file.netname);
        let extra = match method {
            Some(method) => format!(" {}", method.name()),
            None => String::new(),
        };
        // The port of `start_file_transfer()`: a status entry of the ND mode
        // holds no file, and the announcement names the size as the offset.
        if file.path.is_none() {
            self.send_text(
                Command::File,
                &format!(
                    "{} {} {} {}{}",
                    file.netname, file.size, file.time, file.size, extra
                ),
            )
            .await?;
            return Ok(());
        }
        // The port of `start_file_transfer()`: an NR sender announces the
        // file with the offset -1 and waits for the answer of the peer.
        if self.nr.we {
            self.send_text(
                Command::File,
                &format!("{} {} {} -1{}", file.netname, file.size, file.time, extra),
            )
            .await?;
            self.off_req_sent = true;
            return Ok(());
        }
        self.send_text(
            Command::File,
            &format!("{} {} {} 0{}", file.netname, file.size, file.time, extra),
        )
        .await?;
        self.send_data(file, method, 0).await
    }

    /// Sends the data of a file from an offset, the port of `send_block()`.
    async fn send_data(
        &mut self,
        file: &SendFile,
        method: Option<Method>,
        offset: u64,
    ) -> Result<(), Error> {
        let Some(path) = file.path.clone() else {
            return Ok(());
        };
        match method {
            Some(method) => self.send_compressed(&path, file, method, offset).await,
            None => self.send_plain(&path, file, offset).await,
        }
    }

    /// Sends a file in blocks of at most `blksize` bytes, the port of the
    /// plain branch of `send_block()`.
    async fn send_plain(
        &mut self,
        path: &PathBuf,
        file: &SendFile,
        offset: u64,
    ) -> Result<(), Error> {
        let mut input = std::fs::File::open(path).map_err(io_error)?;
        let mut sent: u64 = offset;
        if offset > 0 {
            use std::io::Seek;
            input
                .seek(std::io::SeekFrom::Start(offset))
                .map_err(io_error)?;
        }
        // An empty file is one empty block, as in the C code.
        loop {
            let count = std::cmp::min(self.config.blksize as u64, file.size - sent) as usize;
            let mut block = vec![0u8; count];
            if count > 0 {
                input.read_exact(&mut block).map_err(io_error)?;
                sent += count as u64;
            }
            self.send(Frame::Data(block)).await?;
            if count < self.config.blksize {
                break;
            }
        }
        Ok(())
    }

    /// Sends a compressed file, the port of the compressed branch of
    /// `send_block()`.
    ///
    /// The compressed stream is one stream for the whole file. Each data block
    /// holds at most `blksize` compressed bytes, as in the C code.
    async fn send_compressed(
        &mut self,
        path: &PathBuf,
        file: &SendFile,
        method: Method,
        offset: u64,
    ) -> Result<(), Error> {
        let mut input = std::fs::File::open(path).map_err(io_error)?;
        let mut compressor = Compressor::new(method, self.config.zlevel);
        // The bytes that were read but not compressed yet.
        let mut pending: Vec<u8> = Vec::new();
        // The bytes of the file that were read.
        let mut read: u64 = offset;
        if offset > 0 {
            use std::io::Seek;
            input
                .seek(std::io::SeekFrom::Start(offset))
                .map_err(io_error)?;
        }
        // The compressed bytes of the block in progress.
        let mut block: Vec<u8> = Vec::new();
        let mut ended = false;
        while !ended {
            // Read at most `ZBLKSIZE` bytes after the pending bytes.
            if pending.is_empty() && read < file.size {
                let count = std::cmp::min(BLOCK_SIZE as u64, file.size - read) as usize;
                let start = pending.len();
                pending.resize(start + count, 0);
                input.read_exact(&mut pending[start..]).map_err(io_error)?;
                read += count as u64;
            }
            let finish = read == file.size;
            let step = compressor
                .compress(&pending, self.config.blksize - block.len(), finish)
                .map_err(|e| Error::Protocol(format!("compression failed: {e}")))?;
            block.extend_from_slice(&step.data);
            pending.drain(..step.consumed);
            ended = finish && step.ended;
            if (block.len() == self.config.blksize || ended) && !block.is_empty() {
                self.send(Frame::Data(std::mem::take(&mut block))).await?;
            }
        }
        Ok(())
    }

    /// Writes a received file and runs the callback, the port of `inb_done()`.
    ///
    /// The ND mode defers the call to the next file or to the end of the
    /// batch, the port of `in_complete` of `protocol.c`.
    async fn finish_file(
        &mut self,
        complete: CompleteFile,
        receive: &mut (dyn FnMut(&ReceivedFile) -> std::io::Result<()> + Send),
        srif: &mut (dyn Srif + Send),
    ) -> Result<(), Error> {
        // The port of `inb_done()`: the partial file takes its true name.
        let mut rnd = random();
        let name = self
            .inbound
            .inb_done(&complete.tfile, &mut self.config.spool, now(), &mut rnd);
        let Some(name) = name else {
            return Err(Error::Protocol(format!(
                "cannot save {}",
                complete.tfile.netname
            )));
        };
        let file = ReceivedFile {
            netname: complete.tfile.netname.clone(),
            path: PathBuf::from(&name),
            size: complete.tfile.size,
            time: complete.tfile.time,
            method: complete.method,
        };
        receive(&file).map_err(|e| Error::Io(e.to_string()))?;
        self.report.received.push(file);
        // The port of `inb_done()`: the events of the file run here, and the
        // files that an SRIF exchange answered wait for the end of the batch.
        // The C code takes the name of the file in the inbound directory.
        let peer = self.peer();
        let answered = srif.on_file(&name, &peer);
        self.answered.extend(answered);
        Ok(())
    }

    /// Takes the acknowledgement of a file, the port of `GOT()`.
    ///
    /// The port removes the file from the queue of the sent files and saves
    /// the answer of the peer as the link status of the ND mode. A request of
    /// the NR mode clears the status.
    fn got_file(&mut self, argument: &[u8]) -> Result<(), Error> {
        let (netname, size, time) = parse_sent_args(argument)?;
        let mut address = None;
        for file in &self.sent_fls {
            if file.matches(&netname, size, time) {
                address = file.fa.clone();
                break;
            }
        }
        self.sent_fls
            .retain(|file| !file.matches(&netname, size, time));
        if let Some(fa) = address {
            if self.off_req_sent || !self.nd.we {
                let _ = binkd_spool::status::set_status(&self.config.domains, &fa, "");
            } else {
                let _ = binkd_spool::status::set_status(&self.config.domains, &fa, &text(argument));
            }
        }
        Ok(())
    }

    /// Reads the frames of the peer until it acknowledges the files that we
    /// sent, the port of the `waiting_for_GOT` wait of `current_file_was_sent()`.
    async fn wait_for_got(
        &mut self,
        srif: &mut (dyn Srif + Send),
        timeout: std::time::Duration,
    ) -> Result<(), Error> {
        while !self.sent_fls.is_empty() {
            let frame = self.read(timeout).await?;
            match frame {
                Frame::Command(Command::Got, argument) => self.got_file(&argument)?,
                Frame::Command(Command::Get, argument) => self.get_file(&argument).await?,
                Frame::Command(Command::Skip, argument) => self.skip_file(&argument)?,
                Frame::Command(Command::Nul, argument) => self.handle_nul(&argument, srif),
                Frame::Command(Command::Eob, _) => {
                    self.peer_eob = true;
                    // The port of the `EOB()` handler: the peer answered
                    // every claim of the batch.
                    srif.end_of_batch();
                }
                Frame::Command(Command::Err, argument) => return Err(peer_error(&argument)),
                Frame::Command(Command::Bsy, argument) => {
                    return Err(Error::Busy(text(&argument)));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Declines a file that we announced, the port of `SKIP()`.
    fn skip_file(&mut self, argument: &[u8]) -> Result<(), Error> {
        let (netname, size, time) = parse_sent_args(argument)?;
        let before = self.sent_fls.len();
        self.sent_fls
            .retain(|file| !file.matches(&netname, size, time));
        if self.sent_fls.len() == before {
            return Err(Error::Protocol(format!("unexpected M_SKIP for {netname}")));
        }
        // The port of `SKIP()`: the peer skipped a file of the session,
        // so the C code holds the queue of the session back.
        self.report.skipped = true;
        self.off_req_sent = false;
        Ok(())
    }

    /// Sends a file again from an offset, the port of `GET()`.
    ///
    /// The peer asks for a file that we announced in the NR mode. The port
    /// moves the file of `sent_fls` back to the queue, seeks to the offset,
    /// and announces the file again.
    async fn get_file(&mut self, argument: &[u8]) -> Result<(), Error> {
        let (netname, size, time, offset, extras) = parse_get_args(argument)?;
        let index = self
            .sent_fls
            .iter()
            .position(|file| file.matches(&netname, size, time));
        let Some(index) = index else {
            return Err(Error::Protocol(format!("unexpected M_GET for {netname}")));
        };
        let file = self.sent_fls.remove(index);
        if offset < 0 || offset as u64 > file.size {
            let _ = self
                .send_text(
                    Command::Err,
                    &format!("Invalid M_GET for {netname}, offset {offset}"),
                )
                .await;
            return Err(Error::Protocol(format!(
                "the M_GET offset {offset} is after the end of {netname}"
            )));
        }
        // The port of `GET()`: `NZ` suppresses the compression of the file.
        let method = if extras.iter().any(|extra| extra == "NZ") {
            None
        } else {
            file.method
        };
        let extra = match method {
            Some(method) => format!(" {}", method.name()),
            None => String::new(),
        };
        self.send_text(
            Command::File,
            &format!(
                "{} {} {} {}{}",
                file.netname, file.size, file.time, offset, extra
            ),
        )
        .await?;
        self.send_data(
            &SendFile {
                netname: file.netname.clone(),
                path: file.path.clone(),
                size: file.size,
                time: file.time,
            },
            method,
            offset as u64,
        )
        .await?;
        // The port of `current_file_was_sent()`: the file returns to the queue
        // of the sent files and waits for the acknowledgement of the peer.
        self.sent_fls.push(SentFile {
            netname: file.netname.clone(),
            size: file.size,
            time: file.time,
            path: file.path.clone(),
            method,
            fa: file.fa.clone(),
        });
        self.off_req_sent = false;
        Ok(())
    }

    /// Receives the files of the peer until its end of the batch.
    async fn receive_files(
        &mut self,
        receive: &mut (dyn FnMut(&ReceivedFile) -> std::io::Result<()> + Send),
        srif: &mut (dyn Srif + Send),
        timeout: std::time::Duration,
    ) -> Result<(), Error> {
        let mut current: Option<Incoming> = None;
        loop {
            // The send path of the C code: the files that an exchange
            // answered, and our own end of the batch when the session has
            // nothing more to send.
            self.send_pending(srif, timeout).await?;
            // The C code ends the batch when the peer sent its end, our own
            // end went out, every file that we sent is acknowledged, every
            // request that we sent is answered, and no file is in transfer.
            if self.peer_eob
                && self.sent_eob
                && self.sent_fls.is_empty()
                && self.get_file_balance == 0
                && current.is_none()
            {
                // The port of the end of the batch of `protocol()`: a batch
                // of more than the two `M_EOB` frames starts the next one.
                let peer = self.peer();
                let answered = srif.flush(&peer);
                self.answered.extend(answered);
                self.send_pending(srif, timeout).await?;
                if self.msgs_in_batch <= 2 || self.config.protocol == Protocol::V10 {
                    return Ok(());
                }
                self.msgs_in_batch = 0;
                self.peer_eob = false;
                self.sent_eob = false;
                continue;
            }
            // The read of the C code: the frames of the read ahead first,
            // then one frame of the transport. The C code breaks the read
            // loop when the transport fails, and the condition above decides
            // the status of the session then.
            let frame = match self.try_read_frame().await {
                Ok(Some(frame)) => frame,
                Ok(None) => match self.read(timeout).await {
                    Ok(frame) => frame,
                    Err(_) => break,
                },
                Err(_) => break,
            };
            self.handle_frame(frame, receive, srif, &mut current)
                .await?;
        }
        // The port of the end of `protocol()`: the session is successful
        // when it sent its end, the peer sent its end, every file that we
        // sent is acknowledged, and no file is in transfer.
        if self.sent_eob
            && self.peer_eob
            && self.sent_fls.is_empty()
            && self.get_file_balance == 0
            && current.is_none()
        {
            return Ok(());
        }
        Err(Error::Io(
            "the session ended before the exchange completed".to_string(),
        ))
    }
}

/// The encryption of a session, the port of `keys_out` and `keys_in` of
/// `protoco2.h`.
struct Crypt {
    /// The keys of the outbound stream.
    keys_out: [u32; 3],
    /// The keys of the inbound stream.
    keys_in: [u32; 3],
}

impl Crypt {
    /// Sets the keys, the port of the crypt block of `complete_login()`.
    ///
    /// A side crypts its outbound stream with the key of the password and its
    /// inbound stream with the key of `-` and the password.
    fn new(password: &str, role: Role) -> Crypt {
        let mut dashed = vec![b'-'];
        dashed.extend_from_slice(password.as_bytes());
        let key = binkd_ftn::crypt::init_keys(password.as_bytes());
        let dashed = binkd_ftn::crypt::init_keys(&dashed);
        match role {
            Role::Client => Crypt {
                keys_out: key,
                keys_in: dashed,
            },
            Role::Server => Crypt {
                keys_out: dashed,
                keys_in: key,
            },
        }
    }
}

/// Returns the current time in seconds, the port of `safe_time()`.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Returns the next random number of the name of a file on the wire, the port
/// of `rnd()` of `tools.c`.
///
/// The C code answers the time and the sequence of `rand()` of the C library,
/// and the sequence of `rand()` differs between the systems, so the port holds
/// the sequence of the port.
fn rnd() -> impl FnMut() -> u32 {
    let mut state = now() as u32 ^ std::process::id();
    move || {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (now() as u32).wrapping_add(state & 0x7fff_ffff)
    }
}

/// Returns the next random number, the port of `getrand()`.
fn random() -> impl FnMut() -> u32 {
    let mut state = now() as u32 ^ std::process::id();
    move || {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (state >> 16) & 0x7fff
    }
}

/// Returns the error of a peer `M_ERR` frame.
fn peer_error(argument: &[u8]) -> Error {
    let message = text(argument);
    // The answerer reports a bad password with this message.
    if message.contains("Incorrect password") {
        return Error::BadPassword;
    }
    Error::Peer(message)
}

/// The offset of an `M_FILE` frame that asks for the file, the port of the
/// `-1` offset of the NR mode.
const NR_REQUEST: i64 = -1;

/// A file that we sent, the port of `TFILE sent_fls` of `protocol.c`.
struct SentFile {
    /// The name of the file on the wire.
    netname: String,
    /// The size in bytes.
    size: u64,
    /// The time of the file.
    time: i64,
    /// The path of the file, for a resend. `None` for a status entry.
    path: Option<PathBuf>,
    /// The compression method of the file, for a resend.
    method: Option<Method>,
    /// The address of the receiver, for the link status of the ND mode.
    fa: Option<FtnAddr>,
}

impl SentFile {
    /// Reports whether the file holds the name, the size, and the time, the
    /// port of `tfile_cmp()` of `tools.c`.
    fn matches(&self, netname: &str, size: u64, time: i64) -> bool {
        self.netname == netname && self.size == size && self.time == time
    }
}

/// The file that the receiver holds for the deferred rename of the ND mode,
/// the port of `in_complete` of `protocol.c`.
struct CompleteFile {
    /// The state of the file, the port of `in_complete`.
    tfile: TFile,
    /// The compression method of the file.
    method: Option<Method>,
}

/// The file that we receive, the port of `state->in` of `protocol.c`.
struct Incoming {
    /// The state of the file, the port of `state->in`.
    tfile: TFile,
    /// The partial file of the transfer, the port of `state->in.f`.
    file: std::fs::File,
    /// The compression method of the file, or `None`.
    method: Option<Method>,
    /// The decompression stream, the port of `z_idata`.
    decompressor: Option<Decompressor>,
    /// The bytes of the file that arrived, the position of the partial file.
    received: u64,
}

/// Returns the text of a frame argument.
fn text(argument: &[u8]) -> String {
    String::from_utf8_lossy(argument).into_owned()
}

/// Parses the addresses of an `M_ADR` frame.
fn parse_addresses(argument: &[u8]) -> Vec<FtnAddr> {
    let table = binkd_ftn::domain::DomainTable::new(Vec::new());
    text(argument)
        .split_whitespace()
        .filter_map(|word| FtnAddr::parse(word, &table))
        .collect()
}

/// Parses the arguments of an `M_FILE` frame: the name, the size, the time,
/// the offset, and the extra arguments.
/// Builds a file to send from a path, the port of the part of `netname_()` of
/// `prothlp.c` that reads the name of a file.
///
/// The C code answers the name of the file for a file that is not a mail packet
/// or a request, with the arcshield change and the quoting of the spaces. The port
/// answers the name of the file, and the rest of `netname_()` is the work of the
/// binary, the port of the plan.
fn queued_file(path: &str) -> SendFile {
    let name = Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let metadata = fs::metadata(path).ok();
    SendFile {
        netname: name,
        path: Some(PathBuf::from(path)),
        size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
        time: metadata
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    }
}

fn parse_file_args(argument: &[u8]) -> Result<(String, u64, i64, i64, Vec<String>), Error> {
    let text = text(argument);
    let mut words = text.split_whitespace();
    let netname = words.next().unwrap_or_default().to_string();
    let size = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
    let time = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
    if netname.is_empty() {
        return Err(Error::Protocol("M_FILE without a name".to_string()));
    }
    // The offset of the file follows the time, as `parse_msg_args()` of the
    // C code does. The extra arguments follow the offset.
    let offset = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
    let extras = words.map(|w| w.to_string()).collect();
    Ok((netname, size, time, offset, extras))
}

/// Parses the arguments of an `M_GET` frame: the name, the size, the time,
/// the offset, and the extra arguments.
fn parse_get_args(argument: &[u8]) -> Result<(String, u64, i64, i64, Vec<String>), Error> {
    parse_file_args(argument)
}

/// Parses the arguments of an `M_GOT` and `M_SKIP` frame: the name, the size,
/// and the time.
fn parse_sent_args(argument: &[u8]) -> Result<(String, u64, i64), Error> {
    let (netname, size, time, _, _) = parse_file_args(argument)?;
    Ok((netname, size, time))
}

/// Maps an I/O error.
fn io_error(e: std::io::Error) -> Error {
    Error::Io(e.to_string())
}
