//! The adapter of the Perl hooks to the protocol crate, the port of the part
//! of `perlhooks.c` that reads the session state.

use std::sync::{Arc, Mutex, Weak};

use binkd_config::Config;
use binkd_log::Log;
use binkd_perl::{
    Call, CallError, File, Handshake as PerlHandshake, Hooks, Received, Sent,
    SessionVars as PerlSessionVars,
};
use binkd_proto::{
    CallVars, ErrorVars, FileVars, Handshake, QueueFile, ReceivedVars, SentVars, SessionVars,
};

/// The log of the helpers of Perl, the port of the C `Log()`.
struct LogSink {
    log: Weak<Log>,
}

impl binkd_perl::LogSink for LogSink {
    fn log(&self, level: i32, message: &str) {
        if let Some(log) = self.log.upgrade() {
            log.message(level, message, &crate::platform::now());
        }
    }
}

/// The Perl hooks of a running daemon: the master interpreter, the source of
/// the session hooks, and the log hook.
pub(crate) struct Runtime {
    /// The master interpreter, the port of `cfg->perl`.
    pub(crate) master: Arc<Mutex<Hooks>>,
    /// The source of the hooks of one session.
    pub(crate) factory: binkd_proto::Factory,
    /// The hook of the log, the port of `perl_on_log()`.
    pub(crate) log_hook: Arc<dyn binkd_log::Hook>,
}

/// Loads a script for a config, the port of `perl_init()`.
pub(crate) fn load(log: &Arc<Log>, script: &str, config: Config) -> Option<Runtime> {
    let options = binkd_perl::Options {
        log: Some(Arc::new(LogSink {
            log: Arc::downgrade(log),
        })),
        domains: config.spool.domains.clone(),
        aka: config.spool.aka.clone(),
        sender: None,
        config: Some(config),
    };
    let hooks = binkd_perl::Hooks::load_with(script, options)?;
    let master = Arc::new(Mutex::new(hooks));
    let clone = Arc::clone(&master);
    let factory: binkd_proto::Factory = Arc::new(move || {
        let master = clone.lock().ok()?;
        let clone = master.clone_for_session()?;
        Some(Arc::new(ProtoHook::new(Arc::new(Mutex::new(clone)))) as Arc<dyn binkd_proto::Hooks>)
    });
    let log_hook = Arc::new(binkd_perl::LogHook::from_shared(Arc::clone(&master)));
    Some(Runtime {
        master,
        factory,
        log_hook,
    })
}

/// The hooks of a script as the value of the protocol crate.
///
/// The interpreter belongs to one thread at a time, so the adapter holds a
/// mutex. The log hook and the session hooks share the same value.
pub(crate) struct ProtoHook {
    hooks: Arc<Mutex<Hooks>>,
}

impl ProtoHook {
    /// Wraps the shared hooks of a script.
    pub(crate) fn new(hooks: Arc<Mutex<Hooks>>) -> ProtoHook {
        ProtoHook { hooks }
    }
}

/// The sink of `msg_send()` of one session, the port of the `STATE` of
/// `msg_send2()`.
struct ProtoSink(binkd_proto::MessageSink);

impl binkd_perl::MessageSink for ProtoSink {
    fn send(&self, kind: u8, text: &str) {
        (self.0)(kind, text);
    }
}

/// Converts the session values of the protocol crate.
fn session(vars: &SessionVars) -> PerlSessionVars {
    PerlSessionVars {
        call: vars.call,
        start: vars.start,
        host: vars.host.clone(),
        ip: vars.ip.clone(),
        our_ip: vars.our_ip.clone(),
        our_port: vars.our_port,
        secure: vars.secure,
        sysname: vars.sysname.clone(),
        sysop: vars.sysop.clone(),
        location: vars.location.clone(),
        traffic_mail: vars.traffic_mail,
        traffic_file: vars.traffic_file,
        he: vars.he.clone(),
        me: vars.me.clone(),
        bw_send: vars.bw_send,
        bw_recv: vars.bw_recv,
        bytes_rcvd: vars.bytes_rcvd,
        bytes_sent: vars.bytes_sent,
        files_rcvd: vars.files_rcvd,
        files_sent: vars.files_sent,
        z_send: vars.z_send,
        z_recv: vars.z_recv,
        opt: binkd_perl::Opt {
            nd: vars.opt.nd,
            nr: vars.opt.nr,
            md: vars.opt.md,
            crypt: vars.opt.crypt,
            gz: vars.opt.gz,
        },
        queue: vars
            .queue
            .iter()
            .map(|entry| binkd_perl::QueueFile {
                file: entry.file.clone(),
                size: entry.size,
                time: entry.time,
                sent: entry.sent,
                flvr: entry.flvr,
                action: entry.action,
                type_: entry.type_,
                addr: entry.addr.clone(),
            })
            .collect(),
    }
}

impl binkd_proto::Hooks for ProtoHook {
    fn on_call(&self, call: &mut CallVars) -> bool {
        let Ok(hooks) = self.hooks.lock() else {
            return true;
        };
        let mut vars = Call {
            addr: call.addr.clone(),
            hosts: call.hosts.clone(),
            proxy: call.proxy.clone(),
            socks: call.socks.clone(),
            traffic_mail: call.traffic_mail,
            traffic_file: call.traffic_file,
        };
        let result = hooks.on_call(&mut vars);
        if result {
            call.hosts = vars.hosts;
            call.proxy = vars.proxy;
            call.socks = vars.socks;
        }
        result
    }

    fn on_error(&self, error: &ErrorVars) {
        let Ok(hooks) = self.hooks.lock() else {
            return;
        };
        let vars = CallError {
            addr: error.addr.clone(),
            error: error.error.clone(),
            location: error.location,
        };
        hooks.on_error(&vars);
    }

    fn has_on_handshake(&self) -> bool {
        self.hooks
            .lock()
            .map(|hooks| hooks.has_on_handshake())
            .unwrap_or(false)
    }

    fn on_handshake(&self, vars: &mut Handshake) -> Option<String> {
        let Ok(hooks) = self.hooks.lock() else {
            return None;
        };
        let mut input = PerlHandshake {
            session: session(&vars.session),
            he: vars.he.clone(),
            me: None,
            password: None,
            traffic_mail: vars.traffic_mail,
            traffic_file: vars.traffic_file,
        };
        let response = hooks.on_handshake(&mut input);
        if response.is_none() {
            if let Some(me) = input.me {
                vars.me = Some(me);
            }
            if let Some(password) = input.password {
                vars.password = Some(password);
            }
        }
        response
    }

    fn after_handshake(&self, vars: &SessionVars) -> Option<String> {
        let Ok(hooks) = self.hooks.lock() else {
            return None;
        };
        hooks.after_handshake(&session(vars))
    }

    fn after_session(&self, vars: &SessionVars, ok: bool) {
        let Ok(hooks) = self.hooks.lock() else {
            return;
        };
        hooks.after_session(&session(vars), ok);
    }

    fn before_recv(&self, vars: &SessionVars, file: &FileVars) -> i32 {
        let Ok(hooks) = self.hooks.lock() else {
            return 0;
        };
        let input = File {
            path: file.path.clone(),
            name: file.name.clone(),
            size: file.size,
            time: file.time,
        };
        hooks.before_recv(&session(vars), &input, file.offset)
    }

    fn after_recv(&self, vars: &SessionVars, file: &mut ReceivedVars) -> i32 {
        let Ok(hooks) = self.hooks.lock() else {
            return 0;
        };
        let mut input = Received {
            name: file.name.clone(),
            size: file.size,
            time: file.time,
            tmp: file.tmp.clone(),
            file: file.file.clone(),
        };
        let result = hooks.after_recv(&session(vars), &mut input);
        file.file = input.file;
        result
    }

    fn before_send(&self, vars: &SessionVars, file: &mut FileVars) -> i32 {
        let Ok(hooks) = self.hooks.lock() else {
            return 0;
        };
        let mut input = File {
            path: file.path.clone(),
            name: file.name.clone(),
            size: file.size,
            time: file.time,
        };
        let result = hooks.before_send(&session(vars), &mut input);
        file.name = input.name;
        result
    }

    fn after_sent(&self, vars: &SessionVars, file: &mut SentVars) -> i32 {
        let Ok(hooks) = self.hooks.lock() else {
            return 0;
        };
        let mut input = Sent {
            path: file.path.clone(),
            name: file.name.clone(),
            size: file.size,
            time: file.time,
            start: file.start,
            action: file.action,
        };
        let result = hooks.after_sent(&session(vars), &mut input);
        file.action = input.action;
        result
    }

    fn on_send(&self, vars: &SessionVars, command: binkd_proto::Command, s1: &str, s2: &str) {
        let Ok(hooks) = self.hooks.lock() else {
            return;
        };
        hooks.on_send(&session(vars), command.code(), s1, s2);
    }

    fn on_recv(&self, vars: &SessionVars, data: &[u8]) {
        let Ok(hooks) = self.hooks.lock() else {
            return;
        };
        hooks.on_recv(&session(vars), data);
    }

    fn setup_rlimit(&self, vars: &SessionVars, file: &str, limit: &mut i64) {
        let Ok(hooks) = self.hooks.lock() else {
            return;
        };
        hooks.setup_rlimit(&session(vars), file, limit);
    }

    fn set_message_sink(&self, sink: Option<binkd_proto::MessageSink>) {
        let Ok(mut hooks) = self.hooks.lock() else {
            return;
        };
        let sender = sink.map(|sink| Arc::new(ProtoSink(sink)) as Arc<dyn binkd_perl::MessageSink>);
        hooks.set_message_sink(sender);
    }

    fn refresh_queue(&self, fallback: &binkd_ftn::addr::FtnAddr) -> Option<Vec<QueueFile>> {
        let Ok(hooks) = self.hooks.lock() else {
            return None;
        };
        hooks.refresh_queue(fallback).map(|queue| {
            queue
                .into_iter()
                .map(|entry| QueueFile {
                    file: entry.file,
                    size: entry.size,
                    time: entry.time,
                    sent: entry.sent,
                    flvr: entry.flvr,
                    action: entry.action,
                    type_: entry.type_,
                    addr: entry.addr,
                })
                .collect()
        })
    }
}
