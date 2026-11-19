//! The daemon of rbinkd: the startup order, the signals, the pid file, the
//! listener manager, and the exit, the port of the daemon part of `main()` of
//! `binkd.c`, of `breaksig.c`, of `exitproc.c`, and of `server.c`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rbinkd_config::reload::{Outcome as ReloadOutcome, Reloader};
use rbinkd_config::{Config, Features};
use rbinkd_ftn::addr::FtnAddr;
use rbinkd_ftn::text::extract_filename;
use rbinkd_log::{Log, CONNECTION, DEBUG, DEBUG2, ERROR, INFO, START, TRACE};
use rbinkd_net::{listen, session::SessionOptions, Caller};
use rbinkd_proto::{ReceivedFile, SendFile};
use rbinkd_spool::busy::BusyList;
use rbinkd_spool::config::SpoolConfig;
use rbinkd_spool::fsutil;
use rbinkd_spool::queue::Queue;
use rbinkd_spool::tryfile;
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
#[cfg(unix)]
pub struct Signals {
    /// The `SIGHUP` stream, the port of `signal (SIGHUP, sighandler)`.
    hangup: tokio::signal::unix::Signal,
    /// The `SIGINT` stream, the port of `signal (SIGINT, exitsig)`.
    interrupt: tokio::signal::unix::Signal,
    /// The `SIGTERM` stream, the port of `signal (SIGTERM, exitsig)`.
    terminate: tokio::signal::unix::Signal,
}

#[cfg(unix)]
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

/// The signals of the daemon of Windows, the port of `nt/breaksig.c`.
///
/// The console of a program that runs from the prompt holds the control
/// events. The service mode gives the events of the service control manager
/// through a channel, the port of the `CTRL_SERVICESTOP_EVENT` of `nt/breaksig.c`.
#[cfg(windows)]
pub struct Signals {
    /// The console `CTRL_C_EVENT`.
    ctrl_c: Option<tokio::signal::windows::CtrlC>,
    /// The console `CTRL_BREAK_EVENT`.
    ctrl_break: Option<tokio::signal::windows::CtrlBreak>,
    /// The console `CTRL_CLOSE_EVENT`.
    ctrl_close: Option<tokio::signal::windows::CtrlClose>,
    /// The events of the service control manager.
    service: Option<tokio::sync::mpsc::Receiver<Event>>,
}

#[cfg(windows)]
impl Signals {
    /// Installs the console handlers, the port of `set_break_handlers()` of
    /// `nt/breaksig.c`.
    pub fn install() -> std::io::Result<Signals> {
        use tokio::signal::windows::{ctrl_break, ctrl_c, ctrl_close};
        Ok(Signals {
            ctrl_c: Some(ctrl_c()?),
            ctrl_break: Some(ctrl_break()?),
            ctrl_close: Some(ctrl_close()?),
            service: None,
        })
    }

    /// Builds the signals of the service mode, the port of the control handler
    /// of `ServiceCtrl()` of `nt/service.c`.
    pub fn service(receiver: tokio::sync::mpsc::Receiver<Event>) -> Signals {
        Signals {
            ctrl_c: None,
            ctrl_break: None,
            ctrl_close: None,
            service: Some(receiver),
        }
    }

    /// Waits for the next signal or service event.
    pub async fn next(&mut self) -> Event {
        if let Some(service) = &mut self.service {
            return service.recv().await.unwrap_or(Event::Exit(0));
        }
        let Some(ctrl_c) = self.ctrl_c.as_mut() else {
            return Event::Exit(0);
        };
        let Some(ctrl_break) = self.ctrl_break.as_mut() else {
            return Event::Exit(0);
        };
        let Some(ctrl_close) = self.ctrl_close.as_mut() else {
            return Event::Exit(0);
        };
        tokio::select! {
            _ = ctrl_c.recv() => Event::Exit(2),
            _ = ctrl_break.recv() => Event::Exit(2),
            _ = ctrl_close.recv() => Event::Exit(15),
        }
    }
}

/// The daemon of rbinkd, the port of the state of the process of `main()` of
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
    /// The source of the hooks of a session, or `None`.
    hooks: Option<rbinkd_proto::Factory>,
    /// The Perl hooks of the running daemon, or `None`.
    #[cfg(feature = "perl")]
    perl: Option<crate::perl::Runtime>,
}

impl Daemon {
    /// Starts the daemon, the port of the startup order of `main()` of `binkd.c`
    /// after the config.
    ///
    /// The C code writes the `BEGIN` line, starts the busy list, and seeds the
    /// random numbers. The port of this slice holds the line and the list.
    pub fn new(config: Config, options: &Options, log: Arc<Log>, args: &[String]) -> Daemon {
        // The port of `print_args()` and of `Log (4, "BEGIN, ...")` of `main()`.
        // The C code prints the arguments after the program name, the port of
        // `print_args(tmp, sizeof(tmp), argv + 1)`.
        let printed = cli::print_args(args.get(1..).unwrap_or(&[]));
        #[cfg(windows)]
        let begin = match options.service_display.as_deref() {
            Some(name) => format!(
                "BEGIN service '{name}', {}/{}/{}{}",
                version::NAME,
                version::VERSION,
                platform::system(),
                printed
            ),
            None => format!(
                "BEGIN standalone, {}/{}/{}{}",
                version::NAME,
                version::VERSION,
                platform::system(),
                printed
            ),
        };
        #[cfg(not(windows))]
        let begin = format!(
            "BEGIN, {}/{}/{}{}",
            version::NAME,
            version::VERSION,
            platform::system(),
            printed
        );
        log.message(START, &begin, &platform::now());
        // The port of `bsy_init()` of `bsy.c`: the C code starts an empty list
        // and fills it from the scan of the spool.
        let busy = BusyList::new();
        let mut reloader = Reloader::new(config.clone());
        reloader.set_partition(platform::partition);
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
            hooks: None,
            #[cfg(feature = "perl")]
            perl: None,
        }
    }

    /// Sets the feature set of the config parse, so a reload uses the same
    /// keywords as the first parse.
    pub fn with_features(mut self, features: Features) -> Daemon {
        self.reloader.set_features(features);
        self
    }

    /// Sets the source of the hooks of the sessions.
    pub fn set_hook_factory(&mut self, hooks: Option<rbinkd_proto::Factory>) {
        self.hooks = hooks;
    }

    /// Sets the Perl hooks, the port of the `perl_init()` block of `main()`.
    #[cfg(feature = "perl")]
    pub(crate) fn set_perl(&mut self, runtime: crate::perl::Runtime) {
        let master = Arc::clone(&runtime.master);
        self.reloader
            .set_need_reload(Some(Box::new(move |files, need| match master.lock() {
                Ok(hooks) => hooks.need_reload(files, need),
                Err(_) => 0,
            })));
        self.hooks = Some(runtime.factory.clone());
        self.perl = Some(runtime);
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
    pub async fn serve(&mut self, signals: &mut Signals) -> i32 {
        // The port of the `-i` switch: the server runs one session on the
        // sockets of the caller, the port of the `protocol (inetd_socket_in,
        // inetd_socket_out, ...)` of `main()`, and the program ends.
        if self.options.inetd {
            self.inetd_session().await;
            return 0;
        }
        // The port of the `-P` switches: the C code creates the polls of the
        // nodes before the managers run.
        self.make_polls();
        // The port of `-p`: the client manager runs one pass, and the program
        // ends, the port of the `poll_flag` check of `clientmgr()`.
        if self.options.poll {
            self.client_pass().await;
            return 0;
        }
        // The port of the `do { ... } while (status == 0)` loop of `servmgr()`:
        // a reload closes the listen sockets and the manager starts again.
        let mut listener = self.spawn_listener();
        // The port of `branch (clientmgr, 0, 0)` of `main()`: the client
        // manager runs beside the server.
        let stop = Arc::new(AtomicBool::new(false));
        let mut clients = self.spawn_client(Arc::clone(&stop));
        // The port of the `select()` timeout of `do_server()`: the C code
        // checks the config files on every pass of `CHECKCFG_INTERVAL`, the
        // port of `rescan_delay`.
        let mut delay = Duration::from_secs(self.config.rescan_delay.max(1) as u64);
        let mut ticker = tokio::time::interval(delay);
        ticker.tick().await;
        loop {
            let event = tokio::select! {
                event = signals.next() => Some(event),
                _ = ticker.tick() => None,
            };
            match event {
                None => {
                    // The port of the timeout branch of `do_server()`: the
                    // check reloads only when a config file changed.
                    if self.reload_check() {
                        // The C code closes the listen sockets and the next
                        // pass of the managers reads the new config.
                        if let Some(task) = listener.take() {
                            task.abort();
                            let _ = task.await;
                        }
                        // The accept tasks close their sockets on the abort;
                        // give them a moment before the bind of the new
                        // listener.
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        listener = self.spawn_listener();
                        if let Some(task) = clients.take() {
                            task.abort();
                            let _ = task.await;
                        }
                        clients = self.spawn_client(Arc::clone(&stop));
                        delay = Duration::from_secs(self.config.rescan_delay.max(1) as u64);
                        ticker = tokio::time::interval(delay);
                        ticker.tick().await;
                    }
                }
                Some(Event::Exit(number)) => {
                    // The port of `exitsig()` of `breaksig.c`: the C code of
                    // the thread build logs at the level 1 and sets the flag
                    // that the managers read.
                    self.log
                        .message(INFO, &format!("got signal #{number}."), &platform::now());
                    break;
                }
                Some(Event::Hangup) => {
                    // The port of `sighandler()` of `binkd.c` and of the
                    // start of `checkcfg()` of `readcfg.c`.
                    self.log.message(CONNECTION, "got SIGHUP", &platform::now());
                    if self.reload() {
                        // The port of the socket close of `do_server()`:
                        // the caller of `listen()` stops the task and starts
                        // a new one with the new config. The client manager
                        // restarts too, so it holds the new config and hooks.
                        if let Some(task) = listener.take() {
                            task.abort();
                            let _ = task.await;
                        }
                        // The accept tasks close their sockets on the abort;
                        // give them a moment before the bind of the new
                        // listener.
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        listener = self.spawn_listener();
                        if let Some(task) = clients.take() {
                            task.abort();
                            let _ = task.await;
                        }
                        clients = self.spawn_client(Arc::clone(&stop));
                        delay = Duration::from_secs(self.config.rescan_delay.max(1) as u64);
                        ticker = tokio::time::interval(delay);
                        ticker.tick().await;
                    }
                }
            }
        }
        // The port of `close_srvmgr_socket()` of `exitproc.c` and of the
        // end of `clientmgr()`: the tasks hold the sockets, so their end
        // closes them.
        stop.store(true, Ordering::SeqCst);
        if let Some(task) = listener.take() {
            task.abort();
        }
        if let Some(task) = clients.take() {
            task.abort();
        }
        0
    }

    /// Runs one session on the standard input and output, the port of the
    /// `-i` switch of `main()` of `binkd.c`.
    ///
    /// The C code runs the session of the caller, the port of the
    /// `protocol (inetd_socket_in, inetd_socket_out, ...)`, and it answers the
    /// `-a` switch for the remote address and the `-f` switch for the node.
    async fn inetd_session(&mut self) {
        let peer = self
            .options
            .remote_addr
            .as_deref()
            .and_then(|addr| addr.parse().ok())
            .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
        let timeout = self.session_options().timeout;
        // The port of the `-f` switch: the node of the protected session.
        // The C code parses the address with the domains of the config and
        // looks the node up in the node table.
        let remote_node =
            self.options.remote_node.as_deref().and_then(|text| {
                match FtnAddr::parse(text, &self.config.spool.domains) {
                    Some(fa) => {
                        self.log.message(
                            INFO,
                            &format!("Session with {}", fa.format(false)),
                            &platform::now(),
                        );
                        Some(fa)
                    }
                    None => {
                        self.log.message(
                            ERROR,
                            &format!("`{text}' cannot be parsed as a Fido-style address"),
                            &platform::now(),
                        );
                        None
                    }
                }
            });
        let node = remote_node.as_ref().and_then(|fa| {
            self.config
                .spool
                .nodes
                .get(fa, platform::now().epoch)
                .cloned()
        });
        let mut settings = rbinkd_net::session_config(
            &self.config,
            node.as_ref(),
            !self.options.no_crypt,
            false,
            &platform::now(),
        );
        settings.no_md5 = self.options.no_md5;
        let spool = Arc::new(Mutex::new(self.config.spool.clone()));
        let mut files = {
            let spool = Arc::clone(&spool);
            move |peer: &[FtnAddr]| files_of(&spool, peer, platform::now().epoch)
        };
        let mut srf = rbinkd_net::srif::Events::new(
            &self.config,
            peer,
            Arc::clone(&self.log),
            platform::now(),
        )
        .with_short_name(platform::short_name());
        let mut pipe = crate::pipe::Pipe::new(tokio::io::stdin(), tokio::io::stdout());
        let result = rbinkd_proto::run_with(
            &mut pipe,
            rbinkd_proto::Role::Server,
            &settings,
            &mut files,
            &mut |file: &ReceivedFile| {
                receive_file(
                    &spool,
                    &self.config.inbound,
                    file,
                    platform::now().epoch,
                    &self.log,
                );
                Ok(())
            },
            &mut srf,
            timeout,
        )
        .await;
        if let Err(error) = result {
            // The C code of `protocol()` logs the end of the session.
            self.log.message(
                CONNECTION,
                &format!("session failed: {error}"),
                &platform::now(),
            );
        }
    }

    /// Writes the polls of the `-P` switches, the port of `poll_node()` of
    /// `binkd.c`.
    ///
    /// The C code parses the address of the switch and creates an empty poll
    /// file, the `.ilo` of the node.
    fn make_polls(&mut self) {
        for poll in self.options.polls.clone() {
            let Some(fa) = FtnAddr::parse(&poll, &self.config.spool.domains) else {
                self.log.message(
                    ERROR,
                    &format!("cannot parse the node {poll}"),
                    &platform::now(),
                );
                continue;
            };
            if !tryfile::create_poll(&fa, b'i', &self.config.spool) {
                self.log.message(
                    ERROR,
                    &format!("cannot create the poll of {poll}"),
                    &platform::now(),
                );
            }
        }
    }

    /// Returns the settings of a session, the port of the reads of the config
    /// in `protocol()` of `protocol.c`.
    fn session_options(&self) -> SessionOptions {
        let timeout = Duration::from_secs(self.config.nettimeout.max(1) as u64);
        let mut options = SessionOptions::new(timeout, !self.options.no_crypt, false);
        options.no_md5 = self.options.no_md5;
        options.free_space = platform::free_space;
        options.short_name = platform::short_name();
        match &self.hooks {
            Some(hooks) => options.with_hooks(Arc::clone(hooks)),
            None => options,
        }
    }

    /// Runs one pass of the client manager, the port of `do_client()` of
    /// `client.c`.
    async fn client_pass(&mut self) {
        let (client, _) = self.options.roles();
        if !client {
            return;
        }
        let mut caller = Caller::new();
        let options = self.session_options();
        let spool = Arc::new(Mutex::new(self.config.spool.clone()));
        // The port of the scan of `do_client()` and of `call_all()`: the C
        // code scans the queue for the client manager, the port of
        // `q_scan (SCAN_LISTED, config)`.
        let files = {
            let spool = Arc::clone(&spool);
            move |fa: &FtnAddr| files_of(&spool, std::slice::from_ref(fa), platform::now().epoch)
        };
        let results = caller
            .call_all(&mut self.config, &self.log, &platform::now, options, &files)
            .await;
        for (fa, result) in results {
            match result {
                Ok(report) => {
                    self.log.message(
                        INFO,
                        &format!(
                            "called {fa:?}: {} files sent, {} files received",
                            report.files_sent, report.files_received
                        ),
                        &platform::now(),
                    );
                }
                Err(error) => {
                    self.log.message(
                        ERROR,
                        &format!("the call of {fa:?} failed: {error:?}"),
                        &platform::now(),
                    );
                }
            }
        }
    }

    /// Runs the client manager, the port of `clientmgr()` of `client.c`.
    ///
    /// The C code loops on the passes and waits `call_delay` after a call and
    /// `rescan_delay` when the queue holds no node; a signal ends the loop.
    fn spawn_client(&self, stop: Arc<AtomicBool>) -> Option<JoinHandle<()>> {
        let (client, _) = self.options.roles();
        if !client {
            return None;
        }
        let mut config = self.config.clone();
        let log = Arc::clone(&self.log);
        let options = self.session_options();
        let delay = Duration::from_secs(self.config.rescan_delay.max(0) as u64);
        Some(tokio::spawn(async move {
            loop {
                let mut caller = Caller::new();
                let spool = Arc::new(Mutex::new(config.spool.clone()));
                let files = {
                    let spool = Arc::clone(&spool);
                    move |fa: &FtnAddr| {
                        files_of(&spool, std::slice::from_ref(fa), platform::now().epoch)
                    }
                };
                let results = caller
                    .call_all(&mut config, &log, &platform::now, options.clone(), &files)
                    .await;
                for (fa, result) in results {
                    match result {
                        Ok(report) => {
                            log.message(
                                INFO,
                                &format!(
                                    "called {fa:?}: {} files sent, {} files received",
                                    report.files_sent, report.files_received
                                ),
                                &platform::now(),
                            );
                        }
                        Err(error) => {
                            log.message(
                                ERROR,
                                &format!("the call of {fa:?} failed: {error:?}"),
                                &platform::now(),
                            );
                        }
                    }
                }
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(delay).await;
                if stop.load(Ordering::SeqCst) {
                    break;
                }
            }
        }))
    }

    /// Starts the listener of the server, the port of `do_server()` of
    /// `server.c`.
    ///
    /// The C code builds the sockets of every `listen` entry and accepts the
    /// connections; the port calls `rbinkd_net::listen()`, which does the same.
    /// The C code runs no listener for a run without the server role.
    fn spawn_listener(&self) -> Option<JoinHandle<()>> {
        let (_, server) = self.options.roles();
        if !server {
            return None;
        }
        let config = self.config.clone();
        let log = Arc::clone(&self.log);
        let options = self.session_options();
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
        self.reload_check()
    }

    /// Checks the config files of a pass, the port of `checkcfg()` of
    /// `readcfg.c` without the force of a signal.
    ///
    /// The `-C` switch holds the check of every pass of the C code, the port
    /// of the `checkcfg_flag`.
    pub fn reload_check(&mut self) -> bool {
        match self.reloader.check() {
            ReloadOutcome::Unchanged => false,
            ReloadOutcome::Reloaded(config) => {
                self.log
                    .message(CONNECTION, "Reloading configuration...", &platform::now());
                // The port of `perl_init(new_config, 0)` of `checkcfg()`: the
                // new config loads its script before the log and the config
                // change. A broken strict script keeps the old config.
                #[cfg(feature = "perl")]
                let loaded = {
                    let script = config.perl_script.clone();
                    if script.is_empty() {
                        Some(None)
                    } else {
                        match crate::perl::load(&self.log, &script, (*config).clone()) {
                            Some(runtime) => Some(Some(runtime)),
                            None if config.perl_strict => {
                                self.log.message(
                                    ERROR,
                                    &format!(
                                        "error parsing Perl script {script}, using old config"
                                    ),
                                    &platform::now(),
                                );
                                return false;
                            }
                            None => {
                                self.log.message(
                                    ERROR,
                                    &format!("cannot load Perl script {script}, hooks disabled"),
                                    &platform::now(),
                                );
                                Some(None)
                            }
                        }
                    }
                };
                self.log.init(
                    config.loglevel,
                    config.conlog,
                    &config.logpath,
                    config.nolog.clone(),
                );
                // The port of the `openlog()` call of `vLog()`: a reload
                // recreates the sink, a failed connection keeps the old one.
                #[cfg(unix)]
                crate::syslog::configure(&self.log, config.syslog_facility);
                #[cfg(feature = "perl")]
                {
                    let runtime = loaded.flatten();
                    if let Some(runtime) = runtime {
                        // The port of `perl_config_loaded(new_config)` of
                        // `checkcfg()`: the new script answers the hook.
                        if let Ok(hooks) = runtime.master.lock() {
                            hooks.config_loaded();
                        }
                        self.log.set_hook(Some(runtime.log_hook.clone()));
                        self.set_perl(runtime);
                    } else {
                        // The new config holds no script or its script failed
                        // without strict: the old hooks go, the port of the
                        // config swap of `checkcfg()`.
                        self.perl = None;
                        self.hooks = None;
                        self.reloader.set_need_reload(None);
                        self.log.set_hook(None);
                    }
                }
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
        // The port of the `perl_done(..., 1)` of `exitfunc()`.
        #[cfg(feature = "perl")]
        if let Some(perl) = &self.perl {
            if let Ok(hooks) = perl.master.lock() {
                hooks.on_exit();
            }
        }
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
pub fn files_of(spool: &Mutex<SpoolConfig>, peer: &[FtnAddr], now: i64) -> Vec<SendFile> {
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
/// port does the same in `finish_file()` of `rbinkd-proto`, so the port of this
/// closure must not run `inb_done()` again: a second call finds no temporary file
/// and answers `missing tmp file`. The closure holds the counters of the daemon, the
/// port of the `Log (1, "%s -> %s")` line of the C code.
pub fn receive_file(
    spool: &Mutex<SpoolConfig>,
    dir: &str,
    file: &ReceivedFile,
    now: i64,
    log: &Log,
) {
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
    use rbinkd_ftn::domain::{DomainTable, FtnDomain};
    use rbinkd_ftn::node::NodeSpec;

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
