//! The engine of the library, the port of the managers of `server.c` and of
//! `client.c` that a caller may embed.
//!
//! The C code keeps the config, the log, and the managers in the globals of the
//! process, so one program holds one mailer. The port holds them in one value that
//! the caller creates, so a program may embed the port beside other code. No
//! function of the port ends the process and no function installs a handler of a
//! signal: the binary holds the two.

use std::sync::{Arc, Mutex};

use binkd_config::Config;
use binkd_ftn::addr::FtnAddr;
use binkd_log::Log;
use binkd_net::{bind, listen, session::SessionOptions, Caller};
use binkd_proto::{Report, SendFile};

use crate::platform;

/// The engine of binkd, the port of the managers of `server.c` and of
/// `client.c`.
///
/// The caller creates one engine per instance, the port of the config of one
/// process of the C code. The functions answer a value and never end the process,
/// the port of the `exit()` calls of the C code that the binary holds.
pub struct Engine {
    /// The config, the port of `current_config`.
    config: Config,
    /// The log of the process.
    log: Arc<Log>,
    /// The switches of a run, the port of the flags of the C code.
    options: crate::cli::Options,
}

impl Engine {
    /// Creates an engine, the port of the startup of the managers of `main()`.
    pub fn new(config: Config, options: crate::cli::Options, log: Arc<Log>) -> Engine {
        Engine {
            config,
            options,
            log,
        }
    }

    /// Binds the listeners of the config, the port of the socket build of
    /// `do_server()` of `server.c`.
    ///
    /// A caller that holds the sockets runs the sessions itself, the port of the
    /// `accept()` of the C code.
    pub async fn bind(&self) -> Result<Vec<tokio::net::TcpListener>, binkd_net::ListenError> {
        bind(&self.config, &self.log, &platform::now).await
    }

    /// Runs the listener until a caller ends the task, the port of the loop of
    /// `do_server()` of `server.c`.
    ///
    /// The C code runs one manager of the server in a process of its own; the
    /// port runs one task, so a caller may hold the task of a listener beside
    /// other code, the port of `branch (clientmgr, 0, 0)`.
    pub async fn serve(self) -> Result<(), binkd_net::ListenError> {
        let timeout = std::time::Duration::from_secs(self.config.nettimeout.max(1) as u64);
        let options = SessionOptions::new(timeout, !self.options.no_crypt, false);
        let spool = Arc::new(Mutex::new(self.config.spool.clone()));
        let files = {
            let spool = Arc::clone(&spool);
            move |peer: &[FtnAddr]| crate::daemon::files_of(&spool, peer, platform::now().epoch)
        };
        let inbound = self.config.inbound.clone();
        let log = Arc::clone(&self.log);
        let receive = move |file: &binkd_proto::ReceivedFile| {
            crate::daemon::receive_file(&spool, &inbound, file, platform::now().epoch, &log);
            Ok(())
        };
        listen(
            self.config,
            self.log,
            platform::now,
            options,
            files,
            receive,
        )
        .await
    }

    /// Calls a node, the port of `call()` of `client.c`.
    ///
    /// The C code runs the manager of the client for every node of the queue;
    /// the caller of the port names the node and holds the report.
    pub async fn poll(&mut self, _fa: &FtnAddr) -> Option<Result<Report, binkd_net::CallError>> {
        let timeout = std::time::Duration::from_secs(self.config.nettimeout.max(1) as u64);
        let options = SessionOptions::new(timeout, !self.options.no_crypt, false);
        let spool = Arc::new(Mutex::new(self.config.spool.clone()));
        let files = {
            let spool = Arc::clone(&spool);
            move |peer: &FtnAddr| {
                crate::daemon::files_of(&spool, std::slice::from_ref(peer), platform::now().epoch)
            }
        };
        let mut caller = Caller::new();
        caller
            .call_next(&mut self.config, &self.log, &platform::now, options, &files)
            .await
    }

    /// Returns the files of a node, the port of the scan of the queue.
    pub fn files(&self, peer: &[FtnAddr]) -> Vec<SendFile> {
        let spool = Mutex::new(self.config.spool.clone());
        crate::daemon::files_of(&spool, peer, platform::now().epoch)
    }
}
