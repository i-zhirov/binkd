//! The daemon of binkd: the startup order, the signals, the pid file, the
//! listener manager, and the exit, the port of the daemon part of `main()` of
//! `binkd.c`, of `breaksig.c`, of `exitproc.c`, and of `server.c`.

use std::sync::{Arc, Mutex};

use binkd_config::reload::{Outcome as ReloadOutcome, Reloader};
use binkd_config::Config;
use binkd_ftn::addr::FtnAddr;
use binkd_ftn::text::extract_filename;
use binkd_log::{Log, CONNECTION, DEBUG, DEBUG2, ERROR, INFO, START, TRACE};
use binkd_net::{listen, session::SessionOptions};
use binkd_proto::{ReceivedFile, SendFile};
use binkd_spool::busy::BusyList;
use binkd_spool::config::SpoolConfig;
use binkd_spool::fsutil;
use binkd_spool::queue::Queue;
use tokio::task::JoinHandle;

use crate::cli::{self, Options};
use crate::platform;
use crate::version;

/// The event of a signal, the port of the flags of `sighandler()` of `binkd.c`
/// and of `exitsig()` of `breaksig.c`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The `SIGHUP`: the daemon checks the config files, the port of the
    /// `got_sighup` flag of `binkd.c`.
    Hangup,
    /// The `SIGTERM` or the `SIGINT`: the daemon ends, the port of the
    /// `binkd_exit` flag of `exitproc.c`. The value is the number of the
    /// signal, the port of the `arg` of `exitsig()`.
    Exit(i32),
}

/// The signals of the daemon, the port of `set_break_handlers()` of `breaksig.c`
/// and of the `signal()` calls of `main()` of `binkd.c`.
///
/// The C code installs the handlers and the handlers set the flags that the main
/// loop reads. The port takes the streams of the runtime, and the caller installs
/// them, so the library holds no global state.
pub struct Signals {
    /// The `SIGHUP` stream, the port of `signal (SIGHUP, sighandler)`.
    hangup: tokio::signal::unix::Signal,
    /// The `SIGINT` stream, the port of `signal (SIGINT, exitsig)`.
    interrupt: tokio::signal::unix::Signal,
    /// The `SIGTERM` stream, the port of `signal (SIGTERM, exitsig)`.
    terminate: tokio::signal::unix::Signal,
}

impl Signals {
    /// Installs the handlers and returns the streams, the port of
    /// `set_break_handlers()` of `breaksig.c` and of the `signal()` calls of
    /// `main()` of `binkd.c`.
    ///
    /// The C code ignores `SIGHUP` at the start and enables the handler before
    /// the managers run; the port installs it here. The C code ignores `SIGPIPE`,
    /// and the Rust runtime ignores it for every program.
    pub fn install() -> std::io::Result<Signals> {
        use tokio::signal::unix::{signal, SignalKind};
        Ok(Signals {
            hangup: signal(SignalKind::hangup())?,
            interrupt: signal(SignalKind::interrupt())?,
            terminate: signal(SignalKind::terminate())?,
        })
    }

    /// Waits for the next signal, the port of the loop of `servmgr()` of
    /// `server.c` that reads the flags of the handlers.
    pub async fn next(&mut self) -> Event {
        tokio::select! {
            _ = self.hangup.recv() => Event::Hangup,
            // The numbers are the numbers of `SIGINT` and of `SIGTERM` of the
            // system, the port of the `arg` of `exitsig()`.
            _ = self.interrupt.recv() => Event::Exit(2),
            _ = self.terminate.recv() => Event::Exit(15),
        }
    }
}

/// The daemon of binkd, the port of the state of the process of `main()` of
/// `binkd.c`.
pub struct Daemon {
    /// The config in use, the port of `current_config`.
    config: Config,
    /// The switches of the command line.
    options: Options,
    /// The check of the config files, the port of `checkcfg()`.
    reloader: Reloader,
    /// The busy flags of the spool, the port of `bsy_list` of `bsy.c`.
    busy: BusyList,
    /// The log of the process.
    log: Arc<Log>,
}

impl Daemon {
    /// Starts the daemon, the port of the startup order of `main()` of `binkd.c`
    /// after the config.
    ///
    /// The C code writes the `BEGIN` line, starts the busy list, and seeds the
    /// random numbers. The port of this slice holds the line and the list.
    pub fn new(config: Config, options: &Options, log: Arc<Log>, args: &[String]) -> Daemon {
        // The port of `print_args()` and of `Log (4, "BEGIN, ...")` of `main()`.
        log.message(
            START,
            &format!(
                "BEGIN, {}/{}/{}{}",
                version::NAME,
                version::VERSION,
                platform::system(),
                cli::print_args(args)
            ),
            &platform::now(),
        );
        // The port of `bsy_init()` of `bsy.c`: the C code starts an empty list
        // and fills it from the scan of the spool.
        let busy = BusyList::new();
        let mut reloader = Reloader::new(config.clone());
        reloader.set_check_on_change(options.check_on_change);
        if let Ok(dir) = std::env::current_dir() {
            reloader.set_base_dir(&dir);
        }
        Daemon {
            config,
            options: options.clone(),
            reloader,
            busy,
            log,
        }
    }

    /// Writes the pid file, the port of the pid file block of `main()` of
    /// `binkd.c`.
    ///
    /// The C code unlinks an old file, writes the new file, and runs the
    /// managers. The port of this slice holds the first two steps.
    pub fn write_pid(&mut self) {
        let path = &self.config.pid_file;
        if path.is_empty() {
            return;
        }
        match std::fs::remove_file(path) {
            Ok(()) => {
                self.log.message(
                    ERROR,
                    &format!("unexpected pid_file: {path}: unlinked"),
                    &platform::now(),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // The C code logs this message at the level 8.
                self.log.message(
                    DEBUG2,
                    &format!("unlink_pid_file: {path}: {error}"),
                    &platform::now(),
                );
            }
            Err(error) => {
                self.log.message(
                    ERROR,
                    &format!("unlink_pid_file: {path}: {error}"),
                    &platform::now(),
                );
            }
        }
        if fsutil::create_sem_file(path) {
            // The port of the `Log (5, "created %s")` of `create_sem_file()`.
            self.log
                .message(TRACE, &format!("created {path}"), &platform::now());
        } else {
            self.log
                .message(ERROR, &format!("Can't create {path}"), &platform::now());
        }
    }

    /// Runs the daemon until a signal ends it, the port of the loop of
    /// `servmgr()` of `server.c` and of `main()` of `binkd.c`.
    ///
    /// The C code runs the server and the client managers here. The port of this
    /// slice runs the listener of the server; the client manager arrives with the
    /// next slice.
    pub async fn serve(&mut self, signals: &mut Signals) {
        // The port of the `do { ... } while (status == 0)` loop of `servmgr()`:
        // a reload closes the listen sockets and the manager starts again.
        let mut listener = self.spawn_listener();
        loop {
            match signals.next().await {
                Event::Exit(number) => {
                    // The port of `exitsig()` of `breaksig.c`: the C code of
                    // the thread build logs at the level 1 and sets the flag
                    // that the managers read.
                    self.log
                        .message(INFO, &format!("got signal #{number}."), &platform::now());
                    break;
                }
                Event::Hangup => {
                    // The port of `sighandler()` of `binkd.c` and of the
                    // start of `checkcfg()` of `readcfg.c`.
                    self.log.message(CONNECTION, "got SIGHUP", &platform::now());
                    if self.reload() {
                        // The port of the socket close of `do_server()`:
                        // the caller of `listen()` stops the task and starts
                        // a new one with the new config.
                        if let Some(task) = listener.take() {
                            task.abort();
                        }
                        listener = self.spawn_listener();
                    }
                }
            }
        }
        // The port of `close_srvmgr_socket()` of `exitproc.c`: the task holds
        // the listen sockets, so the end of the task closes them.
        if let Some(task) = listener.take() {
            task.abort();
        }
    }

    /// Starts the listener of the server, the port of `do_server()` of
    /// `server.c`.
    ///
    /// The C code builds the sockets of every `listen` entry and accepts the
    /// connections; the port calls `binkd_net::listen()`, which does the same.
    /// The C code runs no listener for a run without the server role.
    fn spawn_listener(&self) -> Option<JoinHandle<()>> {
        let (_, server) = self.options.roles();
        if !server {
            return None;
        }
        let config = self.config.clone();
        let log = Arc::clone(&self.log);
        let timeout = std::time::Duration::from_secs(self.config.nettimeout.max(1) as u64);
        let options = SessionOptions::new(timeout, !self.options.no_crypt, false);
        let spool = Arc::new(Mutex::new(self.config.spool.clone()));
        let files = {
            let spool = Arc::clone(&spool);
            move |peer: &[FtnAddr]| files_of(&spool, peer, platform::now().epoch)
        };
        let receive = {
            let spool = Arc::clone(&spool);
            let inbound = self.config.inbound.clone();
            let log = Arc::clone(&self.log);
            move |file: &ReceivedFile| {
                receive_file(&spool, &inbound, file, platform::now().epoch, &log);
                Ok(())
            }
        };
        let task_log = Arc::clone(&log);
        Some(tokio::spawn(async move {
            if let Err(error) = listen(config, log, platform::now, options, files, receive).await {
                // The C code of `do_server()` logs the error of the socket
                // build and returns -1, which ends the loop of `servmgr()`.
                task_log.message(ERROR, &format!("servmgr: {error}"), &platform::now());
            }
        }))
    }

    /// Checks the config files, the port of `checkcfg()` of `readcfg.c`.
    ///
    /// Answers true when the daemon takes a new config, the port of the nonzero
    /// return of `checkcfg()` that restarts the listen sockets.
    pub fn reload(&mut self) -> bool {
        self.reloader.set_forced(true);
        match self.reloader.check() {
            ReloadOutcome::Unchanged => false,
            ReloadOutcome::Reloaded(config) => {
                self.log
                    .message(CONNECTION, "Reloading configuration...", &platform::now());
                self.log.init(
                    config.loglevel,
                    config.conlog,
                    &config.logpath,
                    config.nolog.clone(),
                );
                self.config = *config;
                true
            }
            ReloadOutcome::Failed(error) => {
                self.log
                    .message(ERROR, &format!("{error}\n"), &platform::now());
                false
            }
        }
    }

    /// Ends the daemon, the port of `exitfunc()` of `exitproc.c`.
    ///
    /// The C code removes the busy flags and the pid file at the end of the
    /// process, the port of `atexit(exitfunc)`. The port does the same when
    /// the caller ends the daemon.
    pub fn stop(&mut self) {
        // The port of the `Log (7, "exitfunc()")` of `exitfunc()`.
        self.log.message(DEBUG, "exitfunc()", &platform::now());
        self.busy.remove_all(&self.config.spool);
        let path = &self.config.pid_file;
        if !path.is_empty() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Returns the files of a peer, the port of the queue scan of the answerer:
/// `q_scan_addrs()` and `q_get_sizes()` of `ftnq.c`.
///
/// The C code of `complete_login()` scans the queue for the addresses of the
/// peer and the send loop takes the files one by one.
fn files_of(spool: &Mutex<SpoolConfig>, peer: &[FtnAddr], now: i64) -> Vec<SendFile> {
    let Some(first) = peer.first() else {
        return Vec::new();
    };
    let Ok(mut spool) = spool.lock() else {
        return Vec::new();
    };
    let mut queue = Queue::new();
    queue.scan_addrs(peer, true, &mut spool, now, false);
    queue.get_sizes();
    let mut files = Vec::new();
    while let Some(index) = queue.select_next_file(first, &[]) {
        let entry = &queue.entries[index];
        files.push(SendFile {
            // The C code announces the name of the file, the port of the
            // `netname` of the queue entry.
            netname: String::from_utf8_lossy(extract_filename(
                entry.path.to_string_lossy().as_bytes(),
            ))
            .into_owned(),
            path: Some(entry.path.clone()),
            size: entry.size,
            time: entry.time,
        });
    }
    files
}

/// Notes a received file, the port of the counters of `inb_done()` of
/// `inbound.c`.
///
/// The C code renames the temporary file inside the session, and the caller of the
/// port does the same in `finish_file()` of `binkd-proto`, so the port of this
/// closure must not run `inb_done()` again: a second call finds no temporary file
/// and answers `missing tmp file`. The closure holds the counters of the daemon, the
/// port of the `Log (1, "%s -> %s")` line of the C code.
fn receive_file(spool: &Mutex<SpoolConfig>, dir: &str, file: &ReceivedFile, now: i64, log: &Log) {
    let _ = spool;
    let _ = dir;
    let _ = now;
    log.message(
        INFO,
        &format!("{} -> {}", file.netname, file.path.display()),
        &platform::now(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use binkd_ftn::domain::{DomainTable, FtnDomain};
    use binkd_ftn::node::NodeSpec;

    #[test]
    fn the_scan_finds_the_file_of_a_peer() {
        let dir = std::env::temp_dir().join(format!("binkd-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("outb")).unwrap();
        std::fs::write(dir.join("outb/13b7000d.cut"), vec![b's'; 400]).unwrap();
        let mut spool = SpoolConfig::new();
        spool.domains = DomainTable::new(vec![FtnDomain {
            name: "fidonet".to_string(),
            path: dir.display().to_string(),
            dir: "outb".to_string(),
            z: [2, -1],
            idomain: None,
            alias4: None,
        }]);
        let peer = FtnAddr::parse("2:5047/13@fidonet", &spool.domains).unwrap();
        spool.add_node(&peer, &NodeSpec::default(), 1_000_000_000);
        let spool = Mutex::new(spool);
        let files = files_of(&spool, &[peer], 1_000_000_000);
        assert_eq!(files.len(), 1, "{files:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
