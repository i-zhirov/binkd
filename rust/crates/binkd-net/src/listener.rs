//! The listen sockets and the accept loop, the port of `do_server()` of
//! `server.c`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use binkd_config::model::Config;
use binkd_ftn::addr::FtnAddr;
use binkd_log::{DateTime, Log, CONNECTION, ERROR, INFO, TRACE};
use binkd_proto::{run_with, ReceivedFile, Role, SendFile};
use tokio::net::{TcpListener, TcpStream};

use crate::session::{session_config, SessionOptions};

/// The listener failed.
#[derive(Debug)]
pub enum ListenError {
    /// The config holds no `listen` entry that the port can bind.
    NoListen,
    /// The transport failed.
    Io(std::io::Error),
}

impl std::fmt::Display for ListenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ListenError::NoListen => write!(f, "no listen socket"),
            ListenError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ListenError {}

/// Binds one listener for every `listen` entry, the port of the part of
/// `do_server()` that builds the sockets.
///
/// The C code asks `getaddrinfo()` for every entry and binds one socket for
/// every result, so an entry with a name may hold several sockets. The port binds
/// one listener for every entry, and the transport of the runtime resolves the
/// name, so an entry with a name holds one listener for the first address.
pub async fn bind(
    config: &Config,
    log: &Arc<Log>,
    now: &impl Fn() -> DateTime,
) -> Result<Vec<TcpListener>, ListenError> {
    let mut listeners = Vec::new();
    for entry in &config.listen {
        let Some(port) = crate::port::find_port(&entry.port) else {
            return Err(ListenError::NoListen);
        };
        // The C code binds every address for an empty address of the entry,
        // the port of `AI_PASSIVE` of `do_server()`.
        let addr = if entry.addr.is_empty() {
            format!("0.0.0.0:{port}")
        } else {
            format!("{}:{port}", entry.addr)
        };
        match TcpListener::bind(&addr).await {
            Ok(listener) => {
                let local = listener
                    .local_addr()
                    .map(|addr| addr.to_string())
                    .unwrap_or(addr);
                // The C code logs the port of the entry at the level 3.
                log.message(INFO, &format!("listen on {local}"), &now());
                listeners.push(listener);
            }
            Err(e) => {
                log.message(ERROR, &format!("bind {addr}: {e}"), &now());
                return Err(ListenError::Io(e));
            }
        }
    }
    if listeners.is_empty() {
        log.message(ERROR, "no listen socket", &now());
        return Err(ListenError::NoListen);
    }
    Ok(listeners)
}

/// Runs the listener until the task ends, the port of `do_server()`.
///
/// The C code forks a process for every accepted connection and closes the
/// listen sockets in the child. The port starts a task for every connection and
/// holds the listeners in the task of the listener, so the sessions share one
/// process and never share state.
///
/// The `files` closure gives the outgoing files of a peer, the port of the scan
/// of the queue of `protocol()`, and `receive` runs for every file that a
/// session receives.
pub async fn listen(
    config: Config,
    log: Arc<Log>,
    now: impl Fn() -> DateTime + Send + Sync + 'static,
    options: SessionOptions,
    files: impl Fn(&[FtnAddr]) -> Vec<SendFile> + Send + Sync + 'static,
    receive: impl Fn(&ReceivedFile) -> std::io::Result<()> + Send + Sync + 'static,
) -> Result<(), ListenError> {
    let listeners = bind(&config, &log, &now).await?;
    let now = Arc::new(now);
    let sessions = Arc::new(AtomicUsize::new(0));
    // The C code waits on every listen socket at once with `select()`. The port
    // starts a task for every listener, and the tasks hand the accepted
    // connections to one loop.
    let (sender, mut accepted) =
        tokio::sync::mpsc::channel::<(TcpStream, std::net::SocketAddr)>(16);
    for listener in listeners {
        let sender = sender.clone();
        let log = Arc::clone(&log);
        let now = Arc::clone(&now);
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        // The C code resolves the name of the peer later, in
                        // the session, and the port of `getnameinfo()` logs
                        // the numeric address here.
                        log.message(INFO, &format!("incoming from {addr}"), &now());
                        if sender.send((stream, addr)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        // The C code retries the errors of a reset or a
                        // timeout, and gives up on the others.
                        log.message(ERROR, &format!("accept: {e}"), &now());
                        break;
                    }
                }
            }
        });
    }
    let receive = Arc::new(receive);
    let files = Arc::new(files);
    while let Some((stream, addr)) = accepted.recv().await {
        // The port of `++n_servers` of `do_server()`: the C code counts the
        // sessions before it starts one.
        let count = sessions.fetch_add(1, Ordering::SeqCst) + 1;
        log.message(TRACE, &format!("started server #{count}"), &now());
        let mut settings = session_config(&config, None, options.crypt, options.trace);
        settings.max_servers = config.max_servers;
        settings.servers = count;
        let files = Arc::clone(&files);
        let receive = Arc::clone(&receive);
        let sessions = Arc::clone(&sessions);
        let log = Arc::clone(&log);
        let now = Arc::clone(&now);
        let config = config.clone();
        let server_log = Arc::clone(&log);
        tokio::spawn(async move {
            // The SRIF events of the session, the port of the calls of
            // `protocol.c` to `srif.c`.
            let mut srif = crate::srif::Events::new(&config, addr, server_log, now());
            let result = run_with(
                stream,
                Role::Server,
                &settings,
                &mut |peer| files(peer),
                &mut |file| receive(file),
                &mut srif,
                options.timeout,
            )
            .await;
            if let Err(e) = result {
                log.message(CONNECTION, &format!("session failed: {e}"), &now());
            }
            sessions.fetch_sub(1, Ordering::SeqCst);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use binkd_config::model::Config;

    #[tokio::test]
    async fn bind_no_listen_entry() {
        let config = Config::new();
        let log = Arc::new(Log::new(0, 0, "", Vec::new()));
        let now = || DateTime {
            epoch: 0,
            day: 1,
            month: 1,
            hour: 0,
            minute: 0,
            second: 0,
        };
        assert!(matches!(
            bind(&config, &log, &now).await,
            Err(ListenError::NoListen)
        ));
    }
}
