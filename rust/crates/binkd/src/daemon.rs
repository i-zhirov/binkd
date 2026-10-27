//! The daemon of binkd: the startup order, the signals, the pid file, and the
//! exit, the port of the daemon part of `main()` of `binkd.c`, of `breaksig.c`,
//! and of `exitproc.c`.

use std::sync::Arc;

use binkd_config::reload::{Outcome as ReloadOutcome, Reloader};
use binkd_config::Config;
use binkd_log::{Log, CONNECTION, DEBUG, DEBUG2, ERROR, INFO, START, TRACE};
use binkd_spool::busy::BusyList;
use binkd_spool::fsutil;

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
    /// random numbers. The port of this slice holds the line and the list; the
    /// managers arrive with the next slices.
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
    /// slice waits for the signals and checks the config files: the managers
    /// arrive with the next slices.
    pub async fn serve(&mut self, signals: &mut Signals) {
        loop {
            match signals.next().await {
                Event::Exit(number) => {
                    // The port of `exitsig()` of `breaksig.c`: the C code of
                    // the thread build logs at the level 1 and sets the flag
                    // that the managers read.
                    self.log
                        .message(INFO, &format!("got signal #{number}."), &platform::now());
                    return;
                }
                Event::Hangup => {
                    // The port of `sighandler()` of `binkd.c` and of the
                    // start of `checkcfg()` of `readcfg.c`.
                    self.log.message(CONNECTION, "got SIGHUP", &platform::now());
                    self.reload();
                }
            }
        }
    }

    /// Checks the config files, the port of `checkcfg()` of `readcfg.c`.
    pub fn reload(&mut self) {
        self.reloader.set_forced(true);
        match self.reloader.check() {
            ReloadOutcome::Unchanged => {}
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
            }
            ReloadOutcome::Failed(error) => {
                self.log
                    .message(ERROR, &format!("{error}\n"), &platform::now());
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
