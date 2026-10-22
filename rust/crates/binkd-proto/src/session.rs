//! The binkp session, the port of `protocol.c`.
//!
//! The C code keeps the state of a session in the `STATE` value and the process
//! wide flags in `common.h`. The port keeps the state in the session value and
//! drives the frames over a transport that the caller supplies.

use std::fmt;
use std::io::Read;
use std::path::PathBuf;

use binkd_ftn::addr::FtnAddr;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::compress::{self, Compressor, Decompressor, Method, Methods, ZRule, BLOCK_SIZE};
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
    /// The session crypts its traffic, the port of `YES_CRYPT`.
    pub crypt: bool,
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
    /// We sent the end of the batch.
    sent_eob: bool,
    /// The files that we sent and the peer did not acknowledge yet, the port of
    /// `sent_fls`.
    sent_awaiting_got: usize,
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
            peer_methods: Methods::none(),
            extcmd: false,
            we_crypt: config.crypt,
            they_crypt: false,
            md5: false,
            crypt: None,
            sent_eob: false,
            sent_awaiting_got: 0,
            peer_eob: false,
        }
    }

    /// Sends a frame.
    ///
    /// The port of `msg_send2()`: the whole frame, the header included, is
    /// encrypted when the session crypts.
    async fn send(&mut self, frame: Frame) -> Result<(), Error> {
        let mut data = frame.encode();
        if let Some(crypt) = &mut self.crypt {
            binkd_ftn::crypt::encrypt_buf(&mut data, &mut crypt.keys_out);
        }
        self.stream
            .write_all(&data)
            .await
            .map_err(|e| Error::Io(e.to_string()))?;
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
        let mut header = [0u8; frame::HEADER_SIZE];
        self.read_exact(&mut header, timeout).await?;
        if let Some(crypt) = &mut self.crypt {
            binkd_ftn::crypt::decrypt_buf(&mut header, &mut crypt.keys_in);
        }
        let raw = u16::from_be_bytes(header);
        let mut payload = vec![0u8; Frame::payload_size(raw)];
        self.read_exact(&mut payload, timeout).await?;
        if let Some(crypt) = &mut self.crypt {
            binkd_ftn::crypt::decrypt_buf(&mut payload, &mut crypt.keys_in);
        }
        frame::decode(raw, payload).map_err(|e| Error::Io(e.to_string()))
    }

    /// Reads the bytes of a frame.
    async fn read_exact(
        &mut self,
        buffer: &mut [u8],
        timeout: std::time::Duration,
    ) -> Result<(), Error> {
        match tokio::time::timeout(timeout, self.stream.read_exact(buffer)).await {
            Ok(Ok(_)) => Ok(()),
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
        self.send_text(Command::Nul, &format!("VER {version} binkp/1.1"))
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
                Frame::Command(Command::Nul, argument) => self.handle_nul(&argument),
                Frame::Command(Command::Adr, argument) => {
                    self.report.peer_addresses = parse_addresses(&argument);
                    match self.role {
                        // The caller answers the password request of the
                        // answerer, the port of `ADR()`.
                        Role::Client => self.send_password().await?,
                        // No password: the answerer completes the login now.
                        Role::Server if self.config.password.is_empty() => {
                            self.finish_login().await?;
                            return Ok(());
                        }
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
        if self.config.password.is_empty() {
            return Ok(());
        }
        let password = self.config.password.clone();
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

    /// Completes the login, the port of `complete_login()`.
    ///
    /// The answerer sends its options and the state of the session. Both sides
    /// turn the encryption on when the session is secure, the peer requested
    /// the mode, we requested the mode, and the password is a real one.
    async fn finish_login(&mut self) -> Result<(), Error> {
        if self.role == Role::Server {
            self.send_options().await?;
            let state = if self.secure() {
                "secure"
            } else {
                "non-secure"
            };
            self.send_text(Command::Ok, state).await?;
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

    /// Reports whether the password accepts any value, the port of the `!`
    /// password of the C code.
    fn any_password(&self) -> bool {
        self.config.password == "!"
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
        if let Some(challenge) = &self.challenge {
            let digest = binkd_ftn::cram::build_digest(self.config.password.as_bytes(), challenge);
            return digest.as_deref() == Some(given.as_str());
        }
        given == self.config.password
    }

    /// Sends the files and the end of the batch.
    async fn send_files(&mut self, files: &[SendFile]) -> Result<(), Error> {
        for file in files {
            let method = self.send_method(file);
            self.send_file(file, method).await?;
            self.report.files_sent += 1;
            self.report.bytes_sent += file.size;
            self.sent_awaiting_got += 1;
        }
        // The peer may close the transport after its own end of the batch.
        let _ = self.send_text(Command::Eob, "").await;
        self.sent_eob = true;
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
        let extra = match method {
            Some(method) => format!(" {}", method.name()),
            None => String::new(),
        };
        self.send_text(
            Command::File,
            &format!("{} {} {} 0{}", file.netname, file.size, file.time, extra),
        )
        .await?;
        match method {
            Some(method) => self.send_compressed(file, method).await,
            None => self.send_plain(file).await,
        }
    }

    /// Sends a file in blocks of at most `blksize` bytes, the port of the
    /// plain branch of `send_block()`.
    async fn send_plain(&mut self, file: &SendFile) -> Result<(), Error> {
        let mut input = std::fs::File::open(&file.path).map_err(io_error)?;
        let mut sent: u64 = 0;
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
    async fn send_compressed(&mut self, file: &SendFile, method: Method) -> Result<(), Error> {
        let mut input = std::fs::File::open(&file.path).map_err(io_error)?;
        let mut compressor = Compressor::new(method, self.config.zlevel);
        // The bytes that were read but not compressed yet.
        let mut pending: Vec<u8> = Vec::new();
        // The bytes of the file that were read.
        let mut read: u64 = 0;
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

    /// Receives the files of the peer until its end of the batch.
    async fn receive_files(
        &mut self,
        receive: &mut dyn FnMut(&ReceivedFile) -> std::io::Result<()>,
        timeout: std::time::Duration,
    ) -> Result<(), Error> {
        let mut current: Option<Incoming> = None;
        // The C code ends the batch when the peer sent its end and every file
        // that we sent is acknowledged.
        while !self.peer_eob || self.sent_awaiting_got > 0 {
            let frame = self.read(timeout).await?;
            match frame {
                Frame::Command(Command::File, argument) => {
                    let (netname, size, time, extras) = parse_file_args(&argument)?;
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
                    current = Some(Incoming {
                        netname,
                        size,
                        time,
                        method: methods.preferred(),
                        decompressor: None,
                        data: Vec::new(),
                    });
                }
                Frame::Data(data) => {
                    let Some(mut file) = current.take() else {
                        // The C code discards a block without a file.
                        continue;
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
                                let step =
                                    decompressor.decompress(input, BLOCK_SIZE).map_err(|e| {
                                        Error::Protocol(format!(
                                            "decompression of {} failed: {e}",
                                            file.netname
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
                    file.data.extend_from_slice(&bytes);
                    if file.data.len() as u64 > file.size {
                        return Err(Error::Protocol(format!(
                            "the file {} holds {} extra bytes",
                            file.netname,
                            file.data.len() as u64 - file.size
                        )));
                    }
                    if file.data.len() as u64 == file.size {
                        let file = write_inbound(
                            &self.config.inbound,
                            &file.netname,
                            file.time,
                            file.method,
                            &file.data,
                        )?;
                        receive(&file).map_err(|e| Error::Io(e.to_string()))?;
                        let _ = self
                            .send_text(
                                Command::Got,
                                &format!("{} {} {}", file.netname, file.size, file.time),
                            )
                            .await;
                        self.report.files_received += 1;
                        self.report.bytes_received += file.size;
                        self.report.received.push(file);
                    } else {
                        current = Some(file);
                    }
                }
                Frame::Command(Command::Nul, argument) => {
                    // The C code reads an option of the peer at any time.
                    self.handle_nul(&argument);
                }
                Frame::Command(Command::Got, _) => {
                    // The port of `GOT()`: the peer acknowledged a file.
                    self.sent_awaiting_got = self.sent_awaiting_got.saturating_sub(1);
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
        }
        Ok(())
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

/// Returns the error of a peer `M_ERR` frame.
fn peer_error(argument: &[u8]) -> Error {
    let message = text(argument);
    // The answerer reports a bad password with this message.
    if message.contains("Incorrect password") {
        return Error::BadPassword;
    }
    Error::Peer(message)
}

/// The file that we receive, the port of `state->in` of `protocol.c`.
struct Incoming {
    /// The name of the file on the wire.
    netname: String,
    /// The size of the file in bytes, before the compression.
    size: u64,
    /// The time of the file.
    time: i64,
    /// The compression method of the file, or `None`.
    method: Option<Method>,
    /// The decompression stream, the port of `z_idata`.
    decompressor: Option<Decompressor>,
    /// The bytes that we received.
    data: Vec<u8>,
}

/// Writes a received file into the inbound directory.
fn write_inbound(
    inbound: &std::path::Path,
    netname: &str,
    time: i64,
    method: Option<Method>,
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
        method,
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

/// Parses the arguments of an `M_FILE` frame: the name, the size, the time,
/// the offset, and the extra arguments.
fn parse_file_args(argument: &[u8]) -> Result<(String, u64, i64, Vec<String>), Error> {
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
    let _offset = words.next();
    let extras = words.map(|w| w.to_string()).collect();
    Ok((netname, size, time, extras))
}

/// Maps an I/O error.
fn io_error(e: std::io::Error) -> Error {
    Error::Io(e.to_string())
}
