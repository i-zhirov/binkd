//! The binkp wire frames, the port of the frame code of `protocol.c`.
//!
//! A frame has a two byte header: the most significant bit is 1 for a command,
//! and the other bits are the length of the payload. The first byte of a command
//! payload is the command number.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The largest payload of a frame: the `MAX_BLKSIZE` define.
pub const MAX_BLOCK_SIZE: usize = 0x7fff;

/// The size of the frame header.
pub const HEADER_SIZE: usize = 2;

/// A binkp command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// `M_NUL`: the nodelist information and the options.
    Nul,
    /// `M_ADR`: the addresses of the sender.
    Adr,
    /// `M_PWD`: the password.
    Pwd,
    /// `M_FILE`: the properties of the next file.
    File,
    /// `M_OK`: the password is correct.
    Ok,
    /// `M_EOB`: the end of the batch.
    Eob,
    /// `M_GOT`: a file is complete.
    Got,
    /// `M_ERR`: a fatal error.
    Err,
    /// `M_BSY`: the system is busy.
    Bsy,
    /// `M_GET`: the sender must seek in the file.
    Get,
    /// `M_SKIP`: the sender must skip the file.
    Skip,
}

impl Command {
    /// Returns the number of the command.
    pub fn code(self) -> u8 {
        match self {
            Command::Nul => 0,
            Command::Adr => 1,
            Command::Pwd => 2,
            Command::File => 3,
            Command::Ok => 4,
            Command::Eob => 5,
            Command::Got => 6,
            Command::Err => 7,
            Command::Bsy => 8,
            Command::Get => 9,
            Command::Skip => 10,
        }
    }

    /// Returns the command of a number, or `None`.
    pub fn from_code(code: u8) -> Option<Command> {
        match code {
            0 => Some(Command::Nul),
            1 => Some(Command::Adr),
            2 => Some(Command::Pwd),
            3 => Some(Command::File),
            4 => Some(Command::Ok),
            5 => Some(Command::Eob),
            6 => Some(Command::Got),
            7 => Some(Command::Err),
            8 => Some(Command::Bsy),
            9 => Some(Command::Get),
            10 => Some(Command::Skip),
            _ => None,
        }
    }

    /// Returns the short name of the command, as the C code logs it.
    pub fn short_name(self) -> &'static str {
        match self {
            Command::Nul => "NUL",
            Command::Adr => "ADR",
            Command::Pwd => "PWD",
            Command::File => "FILE",
            Command::Ok => "OK",
            Command::Eob => "EOB",
            Command::Got => "GOT",
            Command::Err => "ERR",
            Command::Bsy => "BSY",
            Command::Get => "GET",
            Command::Skip => "SKIP",
        }
    }

    /// Returns the name of the command of the C table.
    pub fn name(self) -> &'static str {
        match self {
            Command::Nul => "M_NUL",
            Command::Adr => "M_ADR",
            Command::Pwd => "M_PWD",
            Command::File => "M_FILE",
            Command::Ok => "M_OK",
            Command::Eob => "M_EOB",
            Command::Got => "M_GOT",
            Command::Err => "M_ERR",
            Command::Bsy => "M_BSY",
            Command::Get => "M_GET",
            Command::Skip => "M_SKIP",
        }
    }
}

/// A binkp frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    /// A data block of the file in transfer.
    Data(Vec<u8>),
    /// A command with its argument.
    Command(Command, Vec<u8>),
}

impl Frame {
    /// Creates a command frame with an argument.
    pub fn command(command: Command, argument: &[u8]) -> Frame {
        Frame::Command(command, argument.to_vec())
    }

    /// Creates a command frame with a text argument.
    pub fn text(command: Command, argument: &str) -> Frame {
        Frame::Command(command, argument.as_bytes().to_vec())
    }

    /// Returns the bytes of the frame.
    ///
    /// The port of `mkhdr()`: the size of a command payload holds the command
    /// byte, so the argument is one byte shorter than the C string.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Frame::Data(data) => {
                let size = data.len() as u16;
                out.push((size >> 8) as u8);
                out.push(size as u8);
                out.extend_from_slice(data);
            }
            Frame::Command(command, argument) => {
                let size = (argument.len() + 1) as u16 | 0x8000;
                out.push((size >> 8) as u8);
                out.push(size as u8);
                out.push(command.code());
                out.extend_from_slice(argument);
            }
        }
        out
    }

    /// Returns the stable name of the frame, for the log of a session.
    ///
    /// The name holds the kind of the frame and, for a banner frame, the first
    /// word of the argument. The arguments that differ between two runs, the time,
    /// the challenge of CRAM-MD5, the password, and the name of a file, are not
    /// part of the name.
    pub fn log_name(&self) -> Option<String> {
        match self {
            Frame::Data(data) => Some(format!("DATA {}", data.len())),
            Frame::Command(command, argument) => {
                let name = command.short_name();
                if *command != Command::Nul {
                    return Some(name.to_string());
                }
                let text = String::from_utf8_lossy(argument);
                let kind = text.split_whitespace().next().unwrap_or_default();
                if matches!(kind, "SYS" | "ZYZ" | "LOC" | "NDL" | "TIME" | "VER" | "OPT") {
                    Some(format!("{name} {kind}"))
                } else {
                    None
                }
            }
        }
    }

    /// Returns the size of the payload of a header.
    pub fn payload_size(header: u16) -> usize {
        (header & 0x7fff) as usize
    }

    /// Returns true when a header starts a command.
    pub fn is_command(header: u16) -> bool {
        header & 0x8000 != 0
    }
}

/// Decodes a frame from a header and its payload.
///
/// The port of the frame part of `recv_block()`: the caller reads the bytes,
/// decrypts them, and gives the header and the payload here.
pub fn decode(header: u16, payload: Vec<u8>) -> std::io::Result<Frame> {
    if !Frame::is_command(header) {
        return Ok(Frame::Data(payload));
    }
    let Some(&code) = payload.first() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "a command frame holds no command byte",
        ));
    };
    let command = Command::from_code(code).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unknown command {code}"),
        )
    })?;
    Ok(Frame::Command(command, payload[1..].to_vec()))
}

/// Reads a frame.
///
/// The port of `recv_block()`. A data block of a size of 0 is not a frame.
pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> std::io::Result<Frame> {
    let mut header = [0u8; HEADER_SIZE];
    reader.read_exact(&mut header).await?;
    let raw = u16::from_be_bytes(header);
    let size = Frame::payload_size(raw);
    let mut payload = vec![0u8; size];
    reader.read_exact(&mut payload).await?;
    decode(raw, payload)
}

/// Writes a frame.
pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &Frame,
) -> std::io::Result<()> {
    writer.write_all(&frame.encode()).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_numbers() {
        for code in 0..=10u8 {
            let command = Command::from_code(code).unwrap();
            assert_eq!(command.code(), code);
            assert_eq!(Command::from_code(code), Some(command));
        }
        assert_eq!(Command::from_code(11), None);
    }

    #[test]
    fn command_frame_bytes() {
        // The example of the C documentation: M_OK with no argument.
        let frame = Frame::command(Command::Ok, b"");
        assert_eq!(frame.encode(), vec![0x80, 0x01, 0x04]);
        let frame = Frame::text(Command::Nul, "SYS binkd");
        let bytes = frame.encode();
        assert_eq!(&bytes[..3], &[0x80, 0x0a, 0x00]);
        assert_eq!(&bytes[3..], b"SYS binkd");
    }

    #[test]
    fn the_time_frame_name() {
        let frame = Frame::text(Command::Nul, "TIME Sun, 15 Nov 2026 09:05:07 +0000");
        assert_eq!(frame.log_name().as_deref(), Some("NUL TIME"));
    }

    #[tokio::test]
    async fn round_trip() {
        let frames = vec![
            Frame::command(Command::Ok, b""),
            Frame::text(Command::Nul, "SYS binkd"),
            Frame::text(Command::Adr, "2:5047/13@fidonet"),
            Frame::text(Command::File, "13b7000d.pkt 100 1000000000 0"),
            Frame::Data(vec![1, 2, 3, 0, 255]),
            Frame::Data(vec![0; MAX_BLOCK_SIZE]),
        ];
        let (mut client, mut server) = tokio::io::duplex(64 * 1024);
        let expected = frames.clone();
        let writer = tokio::spawn(async move {
            for frame in &frames {
                write_frame(&mut client, frame).await.unwrap();
            }
        });
        for frame in &expected {
            assert_eq!(read_frame(&mut server).await.unwrap(), *frame);
        }
        writer.await.unwrap();
    }
}
