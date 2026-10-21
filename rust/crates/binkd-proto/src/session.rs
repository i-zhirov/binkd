//! The binkp session, the port of `protocol.c`.
//!
//! The C code keeps the state of a session in the `STATE` value and the process
//! wide flags in `common.h`. The port keeps the state in the session value and
//! drives the frames over a transport that the caller supplies.

use std::fmt;
use std::path::PathBuf;

use binkd_ftn::addr::FtnAddr;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::frame::{self, Command, Frame};

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
}

/// A file to send.
#[derive(Clone, Debug)]
pub struct SendFile {
    /// The name of the file on the wire.
    pub netname: String,
    /// The path of the file.
    pub path: PathBuf,
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
    receive: &mut dyn FnMut(&ReceivedFile) -> std::io::Result<()>,
    timeout: std::time::Duration,
) -> Result<Report, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut session = Session::new(stream, role, config);
    session.handshake(timeout).await?;
    session.send_files(files).await?;
    session.receive_files(receive, timeout).await?;
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
    /// We sent the end of the batch.
    sent_eob: bool,
    /// The peer sent the end of the batch.
    peer_eob: bool,
}

impl<S> Session<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn new(stream: S, role: Role, config: &SessionConfig) -> Session<S> {
        Session {
            stream,
            role,
            config: config.clone(),
            report: Report::default(),
            challenge: None,
            peer_11: false,
            sent_eob: false,
            peer_eob: false,
        }
    }

    /// Sends a frame.
    async fn send(&mut self, frame: Frame) -> Result<(), Error> {
        frame::write_frame(&mut self.stream, &frame)
            .await
            .map_err(|e| Error::Io(e.to_string()))
    }

    /// Sends a command with a text argument.
    async fn send_text(&mut self, command: Command, text: &str) -> Result<(), Error> {
        self.send(Frame::text(command, text)).await
    }

    /// Reads a frame.
    async fn read(&mut self, timeout: std::time::Duration) -> Result<Frame, Error> {
        match tokio::time::timeout(timeout, frame::read_frame(&mut self.stream)).await {
            Ok(Ok(frame)) => Ok(frame),
            Ok(Err(e)) => Err(Error::Io(e.to_string())),
            Err(_) => Err(Error::Timeout),
        }
    }

    /// Runs the handshake of the port of `banner()` and the command handlers.
    ///
    /// Both sides send the banner and the addresses. The server sends its password
    /// and waits for the password of the caller; a side without a password completes
    /// the login at once. The login is complete when the `M_OK` arrives.
    async fn handshake(&mut self, timeout: std::time::Duration) -> Result<(), Error> {
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
        self.send_text(Command::Nul, &format!("VER {version} binkp/1.1"))
            .await?;
        let addresses: Vec<String> = self
            .config
            .addresses
            .iter()
            .map(|fa| fa.format(true))
            .collect();
        self.send_text(Command::Adr, &addresses.join(" ")).await?;

        if self.role == Role::Client && !self.config.password.is_empty() {
            // The caller sends its password after its addresses.
            let password = self.config.password.clone();
            self.send_text(Command::Pwd, &password).await?;
        }

        // The frames of the peer up to the login acknowledgement.
        loop {
            match self.read(timeout).await? {
                Frame::Command(Command::Nul, argument) => self.handle_nul(&argument),
                Frame::Command(Command::Adr, argument) => {
                    self.report.peer_addresses = parse_addresses(&argument);
                    if self.role == Role::Server {
                        let password = self.config.password.clone();
                        if password.is_empty() {
                            // No password: the server completes the login now.
                            self.send_text(Command::Ok, "").await?;
                            return Ok(());
                        }
                        // The server asks the caller for its password.
                        self.send_text(Command::Pwd, &password).await?;
                    }
                }
                Frame::Command(Command::Pwd, argument) => {
                    // Both sides answer the password of the other.
                    if !self.check_password(&argument) {
                        let _ = self.send_text(Command::Err, "Incorrect password").await;
                        return Err(Error::BadPassword);
                    }
                    self.send_text(Command::Ok, "").await?;
                    if self.role == Role::Server {
                        return Ok(());
                    }
                }
                Frame::Command(Command::Ok, _) => return Ok(()),
                Frame::Command(Command::Err, argument) => {
                    return Err(Error::Peer(text(&argument)));
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

    /// Reads the options of a `M_NUL` frame.
    fn handle_nul(&mut self, argument: &[u8]) {
        let text = text(argument);
        let mut words = text.split_whitespace();
        match words.next() {
            Some("OPT") => {
                for option in words {
                    if option.eq_ignore_ascii_case("NR") {
                        // The NR mode arrives in a later step.
                    } else if let Some(hex) = option.strip_prefix("CRAM-MD5-") {
                        self.challenge = parse_hex(hex);
                    }
                }
            }
            Some("VER") => {
                self.peer_11 = text.contains("binkp/1.1");
            }
            _ => {}
        }
    }

    /// Checks the password of the peer.
    fn check_password(&self, argument: &[u8]) -> bool {
        if self.config.password.is_empty() {
            return true;
        }
        let given = text(argument);
        if let Some(challenge) = &self.challenge {
            let digest = binkd_ftn::cram::build_digest(self.config.password.as_bytes(), challenge);
            return digest.as_deref() == Some(given.as_str());
        }
        given == self.config.password
    }

    /// Sends the files and the end of the batch.
    async fn send_files(&mut self, files: &[SendFile]) -> Result<(), Error> {
        for file in files {
            let data = std::fs::read(&file.path).map_err(|e| Error::Io(e.to_string()))?;
            self.send_text(
                Command::File,
                &format!("{} {} {} 0", file.netname, data.len(), file.time),
            )
            .await?;
            self.send(Frame::Data(data.clone())).await?;
            self.report.files_sent += 1;
            self.report.bytes_sent += data.len() as u64;
        }
        // The peer may close the transport after its own end of the batch.
        let _ = self.send_text(Command::Eob, "").await;
        self.sent_eob = true;
        Ok(())
    }

    /// Receives the files of the peer until its end of the batch.
    async fn receive_files(
        &mut self,
        receive: &mut dyn FnMut(&ReceivedFile) -> std::io::Result<()>,
        timeout: std::time::Duration,
    ) -> Result<(), Error> {
        let mut current: Option<(String, u64, i64, Vec<u8>)> = None;
        while !self.peer_eob {
            let frame = self.read(timeout).await?;
            match frame {
                Frame::Command(Command::File, argument) => {
                    let (netname, size, time) = parse_file_args(&argument)?;
                    current = Some((netname, size, time, Vec::new()));
                }
                Frame::Data(data) => {
                    let Some((netname, size, time, buffer)) = &mut current else {
                        // The C code discards a block without a file.
                        continue;
                    };
                    buffer.extend_from_slice(&data);
                    self.report.bytes_received += data.len() as u64;
                    if buffer.len() as u64 >= *size {
                        let file = write_inbound(&self.config.inbound, netname, *time, buffer)?;
                        receive(&file).map_err(|e| Error::Io(e.to_string()))?;
                        let _ = self
                            .send_text(
                                Command::Got,
                                &format!("{} {} {}", file.netname, file.size, file.time),
                            )
                            .await;
                        self.report.files_received += 1;
                        self.report.received.push(file);
                        current = None;
                    }
                }
                Frame::Command(Command::Got, _) => {}
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
                }
                Frame::Command(Command::Err, argument) => {
                    return Err(Error::Peer(text(&argument)));
                }
                Frame::Command(Command::Bsy, argument) => {
                    return Err(Error::Busy(text(&argument)));
                }
                other => {
                    return Err(Error::Protocol(format!("{other:?} after the login")));
                }
            }
        }
        Ok(())
    }
}

/// Writes a received file into the inbound directory.
fn write_inbound(
    inbound: &std::path::Path,
    netname: &str,
    time: i64,
    data: &[u8],
) -> Result<ReceivedFile, Error> {
    let name =
        String::from_utf8_lossy(&binkd_ftn::text::strdequote(netname.as_bytes())).into_owned();
    let path = inbound.join(&name);
    std::fs::write(&path, data).map_err(|e| Error::Io(e.to_string()))?;
    Ok(ReceivedFile {
        netname: name,
        path,
        size: data.len() as u64,
        time,
    })
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

/// Parses the arguments of an `M_FILE` frame.
fn parse_file_args(argument: &[u8]) -> Result<(String, u64, i64), Error> {
    let text = text(argument);
    let mut words = text.split_whitespace();
    let netname = words.next().unwrap_or_default().to_string();
    let size = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
    let time = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
    if netname.is_empty() {
        return Err(Error::Protocol("M_FILE without a name".to_string()));
    }
    Ok((netname, size, time))
}

/// Parses a hex string.
fn parse_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
    }
    Some(out)
}
