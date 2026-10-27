//! The pipe of the inetd path, the port of the sockets of the `-i` switch of
//! `main()` of `binkd.c`.

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// The two halves of a session, the port of the socket pair of the `-i` switch.
///
/// The C code duplicates the standard input and the standard output, so the session
/// runs on the sockets of the caller, the port of the `protocol (inetd_socket_in,
/// inetd_socket_out, ...)` of `main()`. The port holds the two halves, so the
/// test of the path may run the session over the pipes of the daemon.
pub struct Pipe<R, W> {
    /// The input of the session.
    input: R,
    /// The output of the session.
    output: W,
}

impl<R, W> Pipe<R, W> {
    /// Creates a pipe of an input and an output.
    pub fn new(input: R, output: W) -> Pipe<R, W> {
        Pipe { input, output }
    }
}

impl<R: AsyncRead + Unpin, W: Unpin> AsyncRead for Pipe<R, W> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.input).poll_read(cx, buf)
    }
}

impl<R: Unpin, W: AsyncWrite + Unpin> AsyncWrite for Pipe<R, W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.output).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.output).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.output).poll_shutdown(cx)
    }
}
