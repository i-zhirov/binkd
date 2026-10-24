//! The SRIF events of a session, the port of the calls of `protocol.c` to
//! `srif.c`, and the runner of the external programs, the port of `run()` of
//! `run.c`.
//!
//! The C code runs the external program itself with `system()` of `run.c` and
//! blocks the session for the length of the program. The port does the same: the
//! session waits for the program, the port of the C code, and the plan puts the
//! long work of the port into the tasks of the sessions.

use std::net::SocketAddr;
use std::sync::Arc;

use binkd_config::model::Config;
use binkd_ftn::domain::DomainTable;
use binkd_log::{DateTime, Log, INFO};
use binkd_proto::Peer;
use binkd_spool::queue::Queue;
use binkd_srif::engine::{Context, Engine, Runner as Run};
use binkd_srif::Session;

/// The runner of the external programs, the port of `run()` of `run.c`.
///
/// The C code runs the command with `system()`, which is the shell of the system
/// with the switch that reads a command. It logs the command and its answer, and
/// the port does the same.
pub struct Runner {
    /// The log of the session.
    log: Arc<Log>,
    /// The time.
    now: DateTime,
}

impl Runner {
    /// Creates the runner of a session.
    pub fn new(log: Arc<Log>, now: DateTime) -> Runner {
        Runner { log, now }
    }
}

impl Run for Runner {
    fn run(&mut self, command: &str) {
        self.log
            .message(INFO, &format!("executing `{command}'"), &self.now);
        // The C code runs the command of a shell. The two switches of the two
        // shells of the C code read one command each.
        let mut shell = std::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" });
        shell.arg(if cfg!(windows) { "/C" } else { "-c" });
        match shell.arg(command).status() {
            Ok(status) => self.log.message(
                INFO,
                &format!("rc={}", status.code().unwrap_or(-1)),
                &self.now,
            ),
            Err(e) => self.log.message(INFO, &format!("rc=-1: {e}"), &self.now),
        };
    }
}

/// The SRIF events of a session, the port of the calls of `protocol.c` to
/// `srif.c`.
///
/// The engine, the queue of the files of an exchange, and the config are one value
/// for one session, the port of the state of `srif.c` and of the queue of the
/// session.
pub struct Events {
    /// The engine, the port of the state of `srif.c`.
    engine: Engine,
    /// The queue of the files of an exchange, the port of `state->q`.
    queue: Queue,
    /// The config, the port of the fields that the SRIF code reads.
    config: Config,
    /// The domains of the config, the port of `config->pDomains`. The C code
    /// holds them in the config, and the port holds them apart, so the spool of
    /// the config stays free for the queue of the files.
    domains: DomainTable,
    /// The runner of the external programs.
    runner: Runner,
    /// The name of the peer, the port of `state->peer_name`.
    ///
    /// The C code reads the name of the peer with `getnameinfo()`. The port
    /// answers the address of the peer, because `std` holds no name lookup for an
    /// address.
    peer_name: String,
    /// The address of the peer, the port of `state->ipaddr`.
    ipaddr: String,
    /// The log of the session.
    log: Arc<Log>,
    /// The time.
    now: DateTime,
}

impl Events {
    /// Creates the events of a session.
    pub fn new(config: &Config, peer: SocketAddr, log: Arc<Log>, now: DateTime) -> Events {
        Events {
            engine: Engine::new(),
            queue: Queue::new(),
            config: config.clone(),
            domains: config.spool.domains.clone(),
            runner: Runner::new(Arc::clone(&log), now),
            peer_name: peer.ip().to_string(),
            ipaddr: peer.ip().to_string(),
            log,
            now,
        }
    }

    /// Returns the fields of the peer that the SRIF code reads.
    ///
    /// The C code passes the whole state of the session, and the port passes the
    /// fields that the session knows, the port of `peer()` of the session.
    fn session(&self, peer: &Peer) -> Session {
        Session {
            addresses: peer.addresses.clone(),
            secure: peer.secure,
            listed: peer.listed,
            peer_name: self.peer_name.clone(),
            ipaddr: self.ipaddr.clone(),
            sysop: peer.sysop.clone(),
        }
    }

    /// Returns the paths of the files that the engine queued since a mark.
    fn queued(&self, mark: usize) -> Vec<String> {
        self.queue.entries[mark..]
            .iter()
            .map(|entry| entry.path.display().to_string())
            .collect()
    }
}

impl Events {
    /// Reports whether the engine holds no event that waits.
    pub fn is_empty(&self) -> bool {
        self.engine.is_empty()
    }
}

impl binkd_proto::Srif for Events {
    fn claim(&mut self) {
        self.engine.claim();
    }

    fn end_of_batch(&mut self) {
        self.engine.end_of_batch();
    }

    fn on_file(&mut self, filename: &str, peer: &Peer) -> Vec<String> {
        let session = self.session(peer);
        let mark = self.queue.entries.len();
        let mut context = Context {
            flags: &self.config.evt_flags,
            domains: &self.domains,
            spool: &mut self.config.spool,
            log: &self.log,
            runner: &mut self.runner,
            now: self.now,
        };
        self.engine
            .on_file(filename, &session, &mut self.queue, &mut context);
        self.queued(mark)
    }

    fn flush(&mut self, peer: &Peer) -> Vec<String> {
        let session = self.session(peer);
        let mark = self.queue.entries.len();
        let mut context = Context {
            flags: &self.config.evt_flags,
            domains: &self.domains,
            spool: &mut self.config.spool,
            log: &self.log,
            runner: &mut self.runner,
            now: self.now,
        };
        self.engine
            .flush_deferred(&session, &mut self.queue, &mut context);
        self.queued(mark)
    }

    fn finish(&mut self) {
        let mut context = Context {
            flags: &self.config.evt_flags,
            domains: &self.domains,
            spool: &mut self.config.spool,
            log: &self.log,
            runner: &mut self.runner,
            now: self.now,
        };
        self.engine.finish(&mut context);
    }
}
