//! The engine of the SRIF events of a session, the port of `evt_run()` of
//! `srif.c` and of the parts of `inb_done()` of `inbound.c` and of `protocol()`
//! that call it.
//!
//! The C code holds the events of a session in the state of the session:
//! `evt_queue` holds the `flag` and `exec` events that wait for the end of the
//! session, `rcvdlist` holds the files whose SRIF exchange waits for the end of
//! the batch, and `delay_EOB` counts the FREQ claims of the peer that the session
//! did not answer yet. The port keeps the three in this engine, so the session
//! holds one value and the file system work stays in one place.
//!
//! The C code runs the external program itself with `run()` of `run.c`. The plan
//! puts that file into the binary crate, so the port takes a [`Runner`] and leaves
//! the run to the caller.

use std::fs;
use std::path::Path;

use binkd_config::model::EvtFlag;
use binkd_ftn::domain::DomainTable;
use binkd_ftn::pattern;
use binkd_log::{DateTime, Log, CONNECTION, ERROR, INFO, START};
use binkd_spool::config::SpoolConfig;
use binkd_spool::queue::{EntryKind, Queue};

use crate::args::{self, ShortName};
use crate::event::{self, Event, Events};
use crate::srif::{self, Action};
use crate::Session;

/// The runner of an external program, the port of `run()` of `run.c`.
pub trait Runner {
    /// Runs a command and waits for its end, the port of `run()`.
    ///
    /// The C code runs the command with `system()` and waits for it. It does not
    /// read the answer of the program, and the port leaves that to the caller.
    fn run(&mut self, command: &str);
}

/// The surroundings of the SRIF events, the port of the arguments of `evt_run()`.
pub struct Context<'a> {
    /// The `flag` and `exec` entries of the config, the port of
    /// `config->evt_flags`.
    pub flags: &'a [EvtFlag],
    /// The domains of the config, the port of `config->pDomains`.
    pub domains: &'a DomainTable,
    /// The spool of the config, the port of the arguments of the `q_add_file()`
    /// calls of `parse_response()`.
    pub spool: &'a mut SpoolConfig,
    /// The log of the session.
    pub log: &'a Log,
    /// The runner of the external programs, the port of `run()`.
    pub runner: &'a mut dyn Runner,
    /// The lookup of the short name of a file, the port of the
    /// `GetShortPathName()` call of `run_args()`.
    pub short_name: Option<ShortName>,
    /// The time, the port of `time()`.
    pub now: DateTime,
}

/// The SRIF events of a session, the port of the state of `srif.c`.
#[derive(Clone, Debug, Default)]
pub struct Engine {
    /// The events that wait for the end of the session, the port of
    /// `state->evt_queue`.
    queue: Events,
    /// The files whose SRIF exchange waits for the end of the batch, the port of
    /// `state->rcvdlist`.
    deferred: Vec<String>,
    /// The FREQ claims of the peer that the session did not answer yet, the port
    /// of `state->delay_EOB`.
    claims: usize,
}

impl Engine {
    /// Creates the events of a session, the port of `init_protocol()`.
    pub fn new() -> Engine {
        Engine::default()
    }

    /// Takes a FREQ claim of the peer, the port of the `M_NUL FREQ` handler of
    /// `protocol()`.
    pub fn claim(&mut self) {
        self.claims += 1;
    }

    /// Takes the end of the batch of the peer, the port of the `M_EOB` handler
    /// of `protocol()`.
    pub fn end_of_batch(&mut self) {
        self.claims = 0;
    }

    /// Reports whether the peer claims a FREQ, the port of `delay_EOB > 0`.
    pub fn claimed(&self) -> bool {
        self.claims > 0
    }

    /// Reports whether the events wait for the end of the session.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty() && self.deferred.is_empty()
    }

    /// Takes a received file, the port of the part of `inb_done()` of
    /// `inbound.c` that runs the events.
    ///
    /// The C code tests the file against the `flag` and `exec` entries of the
    /// config. It creates an immediate flag file itself and it queues a delayed
    /// one, and it runs the commands of `evt_run()` after that. It answers the
    /// files of a response list in the queue of the session.
    pub fn on_file(
        &mut self,
        filename: &str,
        session: &Session,
        queue: &mut Queue,
        context: &mut Context,
    ) {
        let events = event::events(context.flags, filename);
        // The C code creates a flag file in `evt_test()`, and it runs the
        // commands of `evt_run()` after that, so the port holds the two apart
        // as well.
        for event in &events {
            match event {
                Event::Flag { path, immediate } => {
                    let delay = if *immediate { "" } else { "delayed " };
                    context.log.message(
                        START,
                        &format!("got {filename}, {delay}creating {path}"),
                        &context.now,
                    );
                    if *immediate {
                        flag(path);
                    } else {
                        self.queue.add(event.clone());
                    }
                }
                Event::Command { command, immediate } => {
                    let delay = if *immediate { "" } else { "delayed " };
                    context.log.message(
                        START,
                        &format!("got {filename}, {delay}starting {command}"),
                        &context.now,
                    );
                }
            }
        }
        // The C code answers 1 for a matching `exec` entry, and it runs
        // `evt_run()` then only.
        if events
            .iter()
            .any(|event| matches!(event, Event::Command { .. }))
        {
            let claimed = self.claimed();
            self.run_commands(filename, session, queue, context, claimed, &events);
        }
        // The C code answers one FREQ claim of the peer for a request file,
        // the port of `if (state->delay_EOB && isreq(file->netname))`.
        if self.claimed() && is_request(filename) {
            self.claims -= 1;
        }
    }

    /// Runs the files whose exchange waits for the end of the batch, the port of
    /// `process_rcvdlist()` of `protocol.c`.
    ///
    /// The C code runs `evt_run()` for every file of the list with a FREQ claim,
    /// so the exchange of the file runs even when the peer answered the claims of
    /// the batch in between.
    pub fn flush_deferred(&mut self, session: &Session, queue: &mut Queue, context: &mut Context) {
        for filename in std::mem::take(&mut self.deferred) {
            let events = event::events(context.flags, &filename);
            self.run_commands(&filename, session, queue, context, true, &events);
        }
    }

    /// Runs the events that wait for the end of the session, the port of
    /// `evt_set()` of `srif.c`.
    pub fn finish(&mut self, context: &mut Context) {
        for event in self.queue.drain() {
            match event {
                Event::Flag { path, .. } => {
                    context
                        .log
                        .message(START, &format!("Creating {path}"), &context.now);
                    flag(&path);
                }
                Event::Command { command, .. } => {
                    context
                        .log
                        .message(START, &format!("Running {command}"), &context.now);
                    context.runner.run(&command);
                }
            }
        }
    }

    /// Runs the commands of the `exec` entries of a received file, the port of
    /// the part of `evt_run()` that reads a command.
    fn run_commands(
        &mut self,
        filename: &str,
        session: &Session,
        queue: &mut Queue,
        context: &mut Context,
        freq: bool,
        events: &[Event],
    ) {
        for event in events {
            let Event::Command { command, immediate } = event else {
                continue;
            };
            if !holds_srif(command) {
                // The C code expands the macros of the command and it runs the
                // command at once, or it queues the command for the end of the
                // session.
                let expanded = args::expand_with(command, filename, session, context.short_name);
                log_short_name_error(context, expanded.short_name_error);
                if expanded.aborted {
                    context.log.message(
                        ERROR,
                        "Security problem. Execution aborted...",
                        &context.now,
                    );
                } else if *immediate {
                    context.runner.run(&expanded.command);
                } else {
                    self.queue.add(Event::Command {
                        command: expanded.command,
                        immediate: false,
                    });
                }
                continue;
            }
            // The command holds `*S`, so it is the command of an external
            // FREQ processor, and the session runs the SRIF exchange.
            if !freq {
                // The C code runs the exchange at the end of the batch.
                self.deferred.push(filename.to_string());
                continue;
            }
            self.exchange(command, filename, session, queue, context);
        }
    }

    /// Runs one SRIF exchange, the port of the part of `evt_run()` that reads a
    /// command of `*S`.
    fn exchange(
        &mut self,
        command: &str,
        filename: &str,
        session: &Session,
        queue: &mut Queue,
        context: &mut Context,
    ) {
        // The C code builds the two paths from the first address of the peer.
        let Some(address) = session.addresses.first() else {
            context
                .log
                .message(ERROR, "mksrifpaths: error", &context.now);
            return;
        };
        let Some((srf, rsp)) = srif::paths(address, context.domains) else {
            context
                .log
                .message(ERROR, "mksrifpaths: error", &context.now);
            return;
        };
        // The C code replaces `*S` of the command with the path of the request
        // file, and it expands the other macros of the command after that.
        let command = replace_srif(command, &srf);
        let request = srif::Request {
            sysop: session.sysop.clone(),
            addresses: session.addresses.clone(),
            request_list: filename.to_string(),
            response_list: rsp.clone(),
            secure: session.secure,
            listed: session.listed,
            caller_id: session.peer_name.clone(),
        };
        if let Err(e) = fs::write(&srf, request.to_text()) {
            context
                .log
                .message(ERROR, &format!("srif_fill: error: {e}"), &context.now);
        } else {
            // The C code runs the program of the command, and it deletes the
            // received file after that. The C code answers a run of an immediate
            // command with the value zero, so the delete always runs.
            let expanded = args::expand_with(&command, filename, session, context.short_name);
            log_short_name_error(context, expanded.short_name_error);
            if expanded.aborted {
                context.log.message(
                    ERROR,
                    "Security problem. Execution aborted...",
                    &context.now,
                );
            } else {
                context.runner.run(&expanded.command);
            }
            let _ = fs::remove_file(filename);
        }
        // The C code parses the response file and it queues the files that it
        // answers, even when the request file failed. It deletes the two files in
        // any case.
        match fs::read_to_string(&rsp) {
            Ok(text) => {
                let entries = srif::parse_response(&text);
                if entries.is_empty() {
                    context
                        .log
                        .message(INFO, "SRIF response file is empty", &context.now);
                }
                for entry in entries {
                    match entry {
                        srif::Entry::File { path, action } => {
                            let action = match action {
                                Action::Delete => b'd',
                                Action::Keep => 0,
                                Action::Kill => b'a',
                            };
                            queue.add_last_file(
                                Path::new(&path),
                                address,
                                b'h',
                                action,
                                EntryKind::Filebox,
                                context.spool,
                                context.now.epoch,
                                session.listed,
                            );
                        }
                        srif::Entry::Unknown { predictor, line } => {
                            context.log.message(
                                CONNECTION,
                                &format!(
                                    "parse_response: unknown predictor `{predictor}', \
                                     ignored response file `{line}'"
                                ),
                                &context.now,
                            );
                        }
                    }
                }
            }
            Err(e) => {
                context
                    .log
                    .message(ERROR, &format!("parse_response: {rsp}: {e}"), &context.now);
            }
        }
        let _ = fs::remove_file(&srf);
        let _ = fs::remove_file(&rsp);
    }
}

/// Reports whether a command asks for the SRIF exchange, the port of the
/// `strstr()` calls of `evt_run()`.
fn holds_srif(command: &str) -> bool {
    command.contains("*S") || command.contains("*s")
}

/// Logs a failed short-name lookup, the port of the C
/// `Log(2, "GetShortPathName() fails %d", ...)` line.
fn log_short_name_error(context: &Context, error: Option<u32>) {
    if let Some(code) = error {
        context
            .log
            .message(2, &format!("GetShortPathName() fails {code}"), &context.now);
    }
}

/// Replaces every `*S` of a command with a path, the port of the `ed()` call of
/// `evt_run()`.
///
/// The C code replaces the macro without the case, so `*s` answers the path as
/// well.
fn replace_srif(command: &str, path: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' {
            if let Some(next) = chars.peek().copied() {
                if next == 'S' || next == 's' {
                    chars.next();
                    out.push_str(path);
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// Reports whether a name is a request file, the port of `isreq()` of `tools.c`.
fn is_request(filename: &str) -> bool {
    pattern::pmatch_ncase(b"*.req", filename.as_bytes())
}

/// Creates an empty flag file, or touches a file that is there, the port of
/// `create_empty_sem_file()` and of `touch()` of `tools.c`.
///
/// The C code creates the file with `O_EXCL` and answers zero when the file is
/// there already, and it sets the time of the access and of the change then.
fn flag(path: &str) {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(_) => {}
        Err(_) => {
            if let Ok(file) = fs::OpenOptions::new().write(true).open(path) {
                let now = std::time::SystemTime::now();
                let times = fs::FileTimes::new().set_accessed(now).set_modified(now);
                let _ = file.set_times(times);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binkd_ftn::addr::FtnAddr;
    use binkd_log::{Level, Sink};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    /// The runner of the tests: it records the commands, and it plays the part
    /// of the external FREQ processor.
    #[derive(Default)]
    struct Recorder {
        commands: Arc<Mutex<Vec<String>>>,
        /// The content of the request file of the command.
        requests: Arc<Mutex<Vec<String>>>,
        /// The response that the test answers for the program.
        response: Arc<Mutex<Option<String>>>,
    }

    impl Runner for Recorder {
        fn run(&mut self, command: &str) {
            if let Ok(mut commands) = self.commands.lock() {
                commands.push(command.to_string());
            }
            // The program reads the request file and it writes the response
            // file, the port of the ERP.
            let srf = command
                .split_whitespace()
                .find(|word| word.ends_with(".srf"));
            let rsp = srf.map(|word| word.replace(".srf", ".rsp"));
            if let Some(path) = &srf {
                if let Ok(text) = fs::read_to_string(path) {
                    if let Ok(mut requests) = self.requests.lock() {
                        requests.push(text);
                    }
                }
            }
            // The program of the test answers the response file of the
            // request, the port of an external FREQ processor.
            if let Some(response) = self.response.lock().ok().and_then(|r| r.clone()) {
                if let Some(path) = rsp {
                    let _ = fs::write(path, response);
                }
            }
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("binkd-srif-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("outb")).unwrap();
        dir
    }

    fn address(s: &str) -> FtnAddr {
        FtnAddr::parse(s, &DomainTable::new(Vec::new())).unwrap()
    }

    fn session() -> Session {
        Session {
            addresses: vec![address("2:5047/13@fidonet")],
            secure: true,
            // The C code queues a file of the response list for an unlisted
            // node only, the port of `if (!listed)` of `q_add_file()`.
            listed: false,
            peer_name: "example.org".to_string(),
            ipaddr: "192.0.2.1".to_string(),
            sysop: "Peer sysop".to_string(),
        }
    }

    fn spool(dir: &Path) -> SpoolConfig {
        let mut spool = SpoolConfig::new();
        spool.domains = DomainTable::new(vec![binkd_ftn::domain::FtnDomain {
            name: "fidonet".to_string(),
            path: dir.display().to_string(),
            dir: "outb".to_string(),
            z: [2, -1],
            idomain: None,
            alias4: None,
        }]);
        spool
    }

    fn now() -> DateTime {
        DateTime {
            epoch: 1_000_000_000,
            year: 2001,
            day: 9,
            month: 9,
            hour: 1,
            minute: 46,
            second: 40,
            offset_minutes: 0,
        }
    }

    fn flag_entry(path: &str, pattern: &str, imm: bool) -> EvtFlag {
        EvtFlag {
            path: Some(path.to_string()),
            command: None,
            pattern: pattern.to_string(),
            imm,
        }
    }

    fn command_entry(command: &str, pattern: &str, imm: bool) -> EvtFlag {
        EvtFlag {
            path: None,
            command: Some(command.to_string()),
            pattern: pattern.to_string(),
            imm,
        }
    }

    /// The flag entry of a file creates the flag file at once, the port of the
    /// first branch of `evt_test()`.
    #[test]
    fn the_flag_file_of_a_file() {
        let dir = temp_dir("flag");
        let path = dir.join("toss.now");
        let flags = vec![flag_entry(path.to_str().unwrap(), "*.pkt", true)];
        let mut engine = Engine::new();
        let mut queue = Queue::new();
        let log = Log::new(0, 0, "", Vec::new());
        let mut runner = Recorder::default();
        let mut spool = spool(&dir);
        let mut context = Context {
            flags: &flags,
            domains: &spool.domains.clone(),
            spool: &mut spool,
            log: &log,
            runner: &mut runner,
            now: now(),
            short_name: None,
        };
        engine.on_file("13b70064.pkt", &session(), &mut queue, &mut context);
        assert!(path.is_file());
        assert!(engine.is_empty());
        // A delayed entry waits for the end of the session.
        let delayed = dir.join("toss2.now");
        let flags = vec![flag_entry(delayed.to_str().unwrap(), "*.pkt", false)];
        let mut context = Context {
            flags: &flags,
            ..context
        };
        engine.on_file("13b70064.pkt", &session(), &mut queue, &mut context);
        assert!(!delayed.is_file());
        assert!(!engine.is_empty());
        engine.finish(&mut context);
        assert!(delayed.is_file());
        assert!(engine.is_empty());
    }

    /// The flag file of a file that is there keeps its content, the port of the
    /// `create_empty_sem_file()` answer of the C code.
    #[test]
    fn the_flag_file_of_an_old_file() {
        let dir = temp_dir("flag-old");
        let path = dir.join("toss.now");
        fs::write(&path, b"old").unwrap();
        let flags = vec![flag_entry(path.to_str().unwrap(), "*.pkt", true)];
        let mut engine = Engine::new();
        let mut queue = Queue::new();
        let log = Log::new(0, 0, "", Vec::new());
        let mut runner = Recorder::default();
        let mut spool = spool(&dir);
        let mut context = Context {
            flags: &flags,
            domains: &spool.domains.clone(),
            spool: &mut spool,
            log: &log,
            runner: &mut runner,
            now: now(),
            short_name: None,
        };
        engine.on_file("13b70064.pkt", &session(), &mut queue, &mut context);
        // The C code touches the file, so its content stays.
        assert_eq!(fs::read(&path).unwrap(), b"old");
    }

    /// The command of a file runs at once or waits for the end of the session,
    /// the port of the two branches of `run_args()`.
    #[test]
    fn the_command_of_a_file() {
        let dir = temp_dir("command");
        let flags = vec![
            command_entry("toss *F", "*.pkt", true),
            command_entry("slow *F", "*.pkt", false),
        ];
        let mut engine = Engine::new();
        let mut queue = Queue::new();
        let log = Log::new(0, 0, "", Vec::new());
        let mut runner = Recorder::default();
        let commands = Arc::clone(&runner.commands);
        let mut spool = spool(&dir);
        let mut context = Context {
            flags: &flags,
            domains: &spool.domains.clone(),
            spool: &mut spool,
            log: &log,
            runner: &mut runner,
            now: now(),
            short_name: None,
        };
        engine.on_file("13b70064.pkt", &session(), &mut queue, &mut context);
        // The immediate command ran, and the other waits.
        assert_eq!(*commands.lock().unwrap(), vec!["toss 13b70064.pkt"]);
        engine.finish(&mut context);
        assert_eq!(
            *commands.lock().unwrap(),
            vec!["toss 13b70064.pkt", "slow 13b70064.pkt"]
        );
    }

    /// The log of the tests records the level and the text of a message.
    struct LogRecord(Arc<Mutex<Vec<(Level, String)>>>);

    impl Sink for LogRecord {
        fn write(&self, level: Level, text: &str) {
            if let Ok(mut records) = self.0.lock() {
                records.push((level, text.to_string()));
            }
        }
    }

    fn short_name_failure(_filename: &str) -> Result<String, u32> {
        Err(5)
    }

    /// A failed short-name lookup logs the C text and runs the command with the
    /// validated long name.
    #[test]
    fn a_failed_short_name_logs_and_runs_the_name() {
        let dir = temp_dir("short-name-log");
        let flags = vec![command_entry("toss *N", "*.pkt", true)];
        let mut engine = Engine::new();
        let mut queue = Queue::new();
        let log = Log::new(2, 0, "", Vec::new());
        let records = Arc::new(Mutex::new(Vec::new()));
        log.set_sink(Some(Arc::new(LogRecord(Arc::clone(&records)))));
        let mut runner = Recorder::default();
        let commands = Arc::clone(&runner.commands);
        let mut spool = spool(&dir);
        let domains = spool.domains.clone();
        let mut context = Context {
            flags: &flags,
            domains: &domains,
            spool: &mut spool,
            log: &log,
            runner: &mut runner,
            now: now(),
            short_name: Some(short_name_failure),
        };
        engine.on_file("13b70064.pkt", &session(), &mut queue, &mut context);
        assert_eq!(*commands.lock().unwrap(), vec!["toss 13b70064.pkt"]);
        let records = records.lock().unwrap();
        assert!(
            records
                .iter()
                .any(|(level, text)| *level == 2 && text == "GetShortPathName() fails 5"),
            "{records:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The SRIF exchange of a file, the port of the `*S` branch of `evt_run()`.
    ///
    /// The program of the test answers the response file, the port of an external
    /// FREQ processor, and the engine queues the files of the response.
    #[test]
    fn the_exchange_of_a_file() {
        let dir = temp_dir("exchange");
        let request = dir.join("in").join("13b70064.req");
        fs::create_dir_all(dir.join("in")).unwrap();
        fs::write(&request, b"the request").unwrap();
        let flags = vec![command_entry("program -SRIF *S", "*.req", true)];
        let mut engine = Engine::new();
        let mut queue = Queue::new();
        let log = Log::new(0, 0, "", Vec::new());
        // The program answers the file of the response, the port of the ERP.
        let answer = format!("={}\n", dir.join("outb").join("13b70064.pkt").display());
        let mut runner = Recorder {
            response: Arc::new(Mutex::new(Some(answer))),
            ..Recorder::default()
        };
        let commands = Arc::clone(&runner.commands);
        let requests = Arc::clone(&runner.requests);
        let mut spool = spool(&dir);
        let domains = spool.domains.clone();
        let mut context = Context {
            flags: &flags,
            domains: &domains,
            spool: &mut spool,
            log: &log,
            runner: &mut runner,
            now: now(),
            short_name: None,
        };
        // The peer claims a FREQ, the port of the `M_NUL FREQ` frame. The C
        // code takes the full name of the received file.
        engine.claim();
        let name = request.to_str().unwrap();
        engine.on_file(name, &session(), &mut queue, &mut context);

        // The command holds the path of the request file, the port of the
        // `*S` replacement of the C code. The C code builds the two paths
        // from the first address of the peer, which is `2:5047/13` here.
        let commands = commands.lock().unwrap().clone();
        assert_eq!(commands.len(), 1);
        let srf = dir.join("outb").join("13b7000d.srf");
        assert_eq!(
            commands[0].replace('\\', "/"),
            format!("program -SRIF {}", srf.display()).replace('\\', "/")
        );
        // The request file holds the statements of the session. The C code
        // deletes the file at the end of the exchange, so the program of the
        // test reads it, the port of the ERP.
        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 1, "{requests:?}");
        let text = &requests[0];
        assert!(text.contains("Sysop Peer sysop\n"), "{text}");
        assert!(text.contains("AKA 2:5047/13@fidonet\n"), "{text}");
        assert!(text.contains(&format!("RequestList {name}\n")), "{text}");
        assert!(text.contains("ResponseList "), "{text}");
        assert!(text.contains("RemoteStatus PROTECTED\n"), "{text}");
        assert!(text.contains("SystemStatus UNLISTED\n"), "{text}");
        // The engine queued the file of the response, and it deleted the
        // request file and the two files of the exchange.
        assert_eq!(queue.entries.len(), 1);
        assert_eq!(queue.entries[0].path, dir.join("outb").join("13b70064.pkt"));
        assert_eq!(queue.entries[0].action, b'd');
        assert_eq!(queue.entries[0].flvr, b'h');
        assert!(!request.is_file());
        assert!(!srf.is_file());
        assert!(!dir.join("outb").join("13b7000d.rsp").is_file());
    }

    /// A file of an `exec` command without a FREQ claim waits for the end of
    /// the batch, the port of `add_to_rcvdlist()` of the C code.
    #[test]
    fn the_deferred_exchange_of_a_file() {
        let dir = temp_dir("deferred");
        let request = dir.join("in").join("13b70064.req");
        fs::create_dir_all(dir.join("in")).unwrap();
        fs::write(&request, b"the request").unwrap();
        let flags = vec![command_entry("program -SRIF *S", "*.req", true)];
        let mut engine = Engine::new();
        let mut queue = Queue::new();
        let log = Log::new(0, 0, "", Vec::new());
        let answer = format!("+{}\n", dir.join("outb").join("13b70064.pkt").display());
        let mut runner = Recorder {
            response: Arc::new(Mutex::new(Some(answer))),
            ..Recorder::default()
        };
        let commands = Arc::clone(&runner.commands);
        let mut spool = spool(&dir);
        let domains = spool.domains.clone();
        let mut context = Context {
            flags: &flags,
            domains: &domains,
            spool: &mut spool,
            log: &log,
            runner: &mut runner,
            now: now(),
            short_name: None,
        };
        // The peer did not claim a FREQ, so the exchange waits.
        let name = request.to_str().unwrap();
        engine.on_file(name, &session(), &mut queue, &mut context);
        assert!(!engine.is_empty());
        assert!(commands.lock().unwrap().is_empty());
        assert!(queue.entries.is_empty());
        assert!(request.is_file());
        // The end of the batch runs the exchange, the port of
        // `process_rcvdlist()`.
        engine.flush_deferred(&session(), &mut queue, &mut context);
        assert_eq!(commands.lock().unwrap().len(), 1);
        assert_eq!(queue.entries.len(), 1);
        // The `+` predictor keeps the file, the port of the C code.
        assert_eq!(queue.entries[0].action, 0);
    }

    /// The engine counts the FREQ claims of the peer, the port of
    /// `state->delay_EOB`.
    #[test]
    fn the_claims_of_the_peer() {
        let dir = temp_dir("claims");
        let flags = vec![command_entry("program -SRIF *S", "*.req", true)];
        let mut engine = Engine::new();
        let mut queue = Queue::new();
        let log = Log::new(0, 0, "", Vec::new());
        let mut runner = Recorder::default();
        let mut spool = spool(&dir);
        let domains = spool.domains.clone();
        let mut context = Context {
            flags: &flags,
            domains: &domains,
            spool: &mut spool,
            log: &log,
            runner: &mut runner,
            now: now(),
            short_name: None,
        };
        assert!(!engine.claimed());
        engine.claim();
        engine.claim();
        assert!(engine.claimed());
        // The end of the batch of the peer takes every claim.
        engine.end_of_batch();
        assert!(!engine.claimed());
        // A request file takes one claim.
        let request = dir.join("in").join("13b70065.req");
        fs::create_dir_all(dir.join("in")).unwrap();
        fs::write(&request, b"the request").unwrap();
        engine.claim();
        engine.on_file("13b70065.req", &session(), &mut queue, &mut context);
        assert!(!engine.claimed());
    }
}
