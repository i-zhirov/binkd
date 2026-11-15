//! The recording and the replay of a session, the test mode of the plan.
//!
//! The C code writes a binary log of the statistics of a session, not of the
//! bytes. The Rust port records the bytes themselves, so the replay test can
//! drive a session from a recording and compare every byte that our side sends.
//!
//! A recording holds the bytes of the two directions of one session. The file is
//! a text file. A line holds a direction and the bytes in hexadecimal:
//!
//! ```text
//! # binkd recorded session
//! # role server
//! s 5300053459...
//! r 000d0a...
//! ```
//!
//! The direction `s` holds the bytes that our side sent, and `r` the bytes that
//! our side received. The role names the side that recorded the session.

use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::Role;

/// The direction of the recorded bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Our side sent the bytes.
    Sent,
    /// Our side received the bytes.
    Received,
}

/// The bytes of the two directions of a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recording {
    /// The role of our side.
    pub role: Role,
    /// The session asked for the NR mode, the port of `WANT_NR`.
    pub nr: bool,
    /// The session asked for the ND mode, the port of `THEY_ND`.
    pub nd: bool,
    /// The bytes that our side sent.
    pub sent: Vec<u8>,
    /// The bytes that our side received.
    pub received: Vec<u8>,
    /// The bytes of the partial file of the transfer, the port of the partial
    /// file that a session left behind. The replay creates it again.
    pub partial: Vec<u8>,
}

impl Recording {
    /// Creates an empty recording of a side.
    pub fn new(role: Role) -> Recording {
        Recording {
            role,
            nr: false,
            nd: false,
            sent: Vec::new(),
            received: Vec::new(),
            partial: Vec::new(),
        }
    }

    /// Adds the bytes of a direction.
    pub fn push(&mut self, direction: Direction, data: &[u8]) {
        match direction {
            Direction::Sent => self.sent.extend_from_slice(data),
            Direction::Received => self.received.extend_from_slice(data),
        }
    }

    /// Reads a recording from a file.
    pub fn read(path: &Path) -> io::Result<Recording> {
        Recording::parse(&std::fs::read_to_string(path)?)
    }

    /// Writes the recording into a file.
    pub fn write(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, self.to_text())
    }

    /// Parses the text of a recording.
    pub fn parse(text: &str) -> io::Result<Recording> {
        let mut role = None;
        let mut recording = Recording::new(Role::Server);
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(comment) = line.strip_prefix('#') {
                if comment.trim() == "nr" {
                    recording.nr = true;
                    continue;
                }
                if comment.trim() == "nd" {
                    recording.nd = true;
                    continue;
                }
                if let Some(hex) = comment.trim().strip_prefix("partial ") {
                    recording.partial = decode_hex(hex.trim())?;
                    continue;
                }
                if let Some(name) = comment.trim().strip_prefix("role ") {
                    role = Some(match name.trim() {
                        "client" => Role::Client,
                        "server" => Role::Server,
                        other => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("unknown role {other}"),
                            ))
                        }
                    });
                }
                continue;
            }
            let mut words = line.split_whitespace();
            let direction = match words.next() {
                Some("s") => Direction::Sent,
                Some("r") => Direction::Received,
                other => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("unknown direction {other:?}"),
                    ))
                }
            };
            let hex = words.next().unwrap_or_default();
            recording.push(direction, &decode_hex(hex)?);
        }
        recording.role = role.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "the recording names no role")
        })?;
        Ok(recording)
    }

    /// Returns the text of the recording.
    pub fn to_text(&self) -> String {
        let mut text = String::from("# binkd recorded session\n");
        match self.role {
            Role::Client => text.push_str("# role client\n"),
            Role::Server => text.push_str("# role server\n"),
        }
        if self.nr {
            text.push_str("# nr\n");
        }
        if self.nd {
            text.push_str("# nd\n");
        }
        if !self.partial.is_empty() {
            text.push_str(&format!("# partial {}\n", encode_hex(&self.partial)));
        }
        // The lines keep the order of the two directions of the session.
        let mut sent = &self.sent[..];
        let mut received = &self.received[..];
        while !sent.is_empty() || !received.is_empty() {
            let count = std::cmp::min(sent.len(), 64);
            if count > 0 {
                text.push_str(&format!("s {}\n", encode_hex(&sent[..count])));
                sent = &sent[count..];
            }
            let count = std::cmp::min(received.len(), 64);
            if count > 0 {
                text.push_str(&format!("r {}\n", encode_hex(&received[..count])));
                received = &received[count..];
            }
        }
        text
    }
}

/// Records the bytes of a transport, the recorder of the test mode.
pub struct Recorder<S> {
    stream: S,
    recording: Recording,
}

impl<S> Recorder<S> {
    /// Wraps a transport and records the bytes of both directions.
    pub fn new(stream: S, role: Role) -> Recorder<S> {
        Recorder {
            stream,
            recording: Recording::new(role),
        }
    }

    /// Returns the recording of the session.
    pub fn into_recording(self) -> Recording {
        self.recording
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Recorder<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let filled = buf.filled().len();
        let result = Pin::new(&mut this.stream).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            let data = buf.filled()[filled..].to_vec();
            this.recording.push(Direction::Received, &data);
        }
        result
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Recorder<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.stream).poll_write(cx, buf);
        if let Poll::Ready(Ok(count)) = &result {
            this.recording.push(Direction::Sent, &buf[..*count]);
        }
        result
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}

/// Replays the bytes of a peer and records the bytes of our side.
pub struct Replay {
    /// The bytes of the peer that are not played yet.
    peer: VecDeque<u8>,
    /// The bytes that our side sent.
    sent: Vec<u8>,
}

impl Replay {
    /// Creates the replay of a recording.
    pub fn new(recording: &Recording) -> Replay {
        Replay {
            peer: recording.received.iter().copied().collect(),
            sent: Vec::new(),
        }
    }

    /// Returns the bytes that our side sent.
    pub fn sent(&self) -> &[u8] {
        &self.sent
    }

    /// Reports whether the peer holds no more bytes.
    pub fn done(&self) -> bool {
        self.peer.is_empty()
    }
}

impl AsyncRead for Replay {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let out = buf.initialize_unfilled();
        let count = std::cmp::min(out.len(), this.peer.len());
        for (i, byte) in this.peer.drain(..count).enumerate() {
            out[i] = byte;
        }
        buf.advance(count);
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for Replay {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.sent.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Returns the bytes of a hexadecimal string.
fn decode_hex(s: &str) -> io::Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "an odd number of hexadecimal digits",
        ));
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in s.as_bytes().chunks(2) {
        let hi = (pair[0] as char).to_digit(16);
        let lo = (pair[1] as char).to_digit(16);
        match (hi, lo) {
            (Some(hi), Some(lo)) => out.push((hi * 16 + lo) as u8),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "a hexadecimal digit is not valid",
                ))
            }
        }
    }
    Ok(out)
}

/// Returns the hexadecimal string of the bytes.
fn encode_hex(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for byte in data {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut recording = Recording::new(Role::Server);
        recording.push(Direction::Sent, &[0x53, 0x00, 0x05]);
        recording.push(Direction::Received, &[0x80, 0x01, 0x04]);
        let text = recording.to_text();
        let parsed = Recording::parse(&text).unwrap();
        assert_eq!(parsed, recording);
    }

    #[test]
    fn role() {
        let mut recording = Recording::new(Role::Client);
        recording.push(Direction::Sent, &[0x00]);
        let parsed = Recording::parse(&recording.to_text()).unwrap();
        assert_eq!(parsed.role, Role::Client);
        assert!(Recording::parse("s 00\n").is_err());
    }

    #[test]
    fn hex() {
        assert_eq!(decode_hex("00ff10").unwrap(), vec![0x00, 0xff, 0x10]);
        assert_eq!(encode_hex(&[0x00, 0xff, 0x10]), "00ff10");
        assert!(decode_hex("0f0").is_err());
        assert!(decode_hex("zz").is_err());
    }

    /// Records the two ends of a session and replays the recording.
    #[tokio::test]
    async fn record_and_replay() {
        use crate::session::{run, Protocol, Role, SessionConfig};

        let config = SessionConfig {
            sysname: "record test".to_string(),
            sysop: "Test sysop".to_string(),
            location: "Test location".to_string(),
            nodeinfo: "Test node".to_string(),
            addresses: Vec::new(),
            password: String::new(),
            inbound: std::env::temp_dir(),
            free_space: binkd_spool::inbound::assume_enough,
            spool: binkd_spool::config::SpoolConfig::new(),
            version: "test/1.0".to_string(),
            blksize: 4096,
            zminsize: 1024,
            zlevel: 0,
            zrules: Vec::new(),
            methods: crate::Methods::none(),
            crypt: false,
            no_md5: false,
            md_flag: 0,
            nr: false,
            nd: false,
            domains: binkd_ftn::domain::DomainTable::new(Vec::new()),
            max_servers: 100,
            servers: 1,
            trace: false,
            rates: Vec::new(),
            protocol: Protocol::default(),
            bw_send: -100,
            bw_recv: -100,
            skip: Vec::new(),
            listed: false,
            prescan: false,
        };
        let (left, right) = tokio::io::duplex(1 << 16);
        let mut recorder = Recorder::new(left, Role::Client);
        let client = async {
            run(
                &mut recorder,
                Role::Client,
                &config,
                &[],
                &mut |_| Ok(()),
                std::time::Duration::from_secs(5),
            )
            .await
        };
        let server = async {
            run(
                right,
                Role::Server,
                &config,
                &[],
                &mut |_| Ok(()),
                std::time::Duration::from_secs(5),
            )
            .await
        };
        let (client, server) = tokio::join!(client, server);
        assert!(client.is_ok() && server.is_ok());
        let recording = recorder.into_recording();
        assert!(!recording.sent.is_empty());
        assert!(!recording.received.is_empty());

        // The replay of the peer side drives the same session.
        let mut replay = Replay::new(&recording);
        let report = run(
            &mut replay,
            Role::Client,
            &config,
            &[],
            &mut |_| Ok(()),
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(replay.sent(), recording.sent.as_slice());
        assert!(replay.done(), "{report:?}");
    }
}
