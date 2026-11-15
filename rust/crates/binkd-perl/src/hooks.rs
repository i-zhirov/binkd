//! The hooks of the process and of the session, the port of `perlhooks.c`.

use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Arc, Mutex, Once};

use binkd_config::model::Config;
use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::DomainTable;
use binkd_ftn::node::NodePassword;
use binkd_ftn::pattern::pmatch_ncase;
use binkd_ftn::text::InbCase;
use binkd_log::{Hook, ERROR};

use crate::bridge::{
    self, Interpreter, Value, AFTER_HANDSHAKE, AFTER_RECV, AFTER_SENT, AFTER_SESSION, BEFORE_RECV,
    BEFORE_SEND, CONFIG_LOADED, G_EVAL, G_SCALAR, NEED_RELOAD, ON_CALL, ON_ERROR, ON_EXIT,
    ON_HANDSHAKE, ON_LOG, ON_RECV, ON_SEND, ON_START, SETUP_RLIMIT,
};
use crate::ffi;

/// A sink of the lines of the helpers `Log()` and `binkd_warn()`, the port of
/// the C `Log()` of `tools.c`.
pub trait LogSink: Send + Sync {
    /// Writes a line at a level, the port of `Log(level, "%s", text)`.
    fn log(&self, level: i32, message: &str);
}

/// A sink of the messages of the helper `msg_send()`, the port of `msg_send2()`
/// of `protocol.c`.
///
/// The session of `binkd-proto` owns the queue of the binkp messages. The
/// binary supplies the sink of the session, so the helper stays in the crate of
/// the hooks. Every session clone holds its own sink, so two sessions do not
/// share the queue.
pub trait MessageSink: Send + Sync {
    /// Queues a binkp message of a kind and a text, the port of `msg_send2()`.
    fn send(&self, kind: u8, text: &str);
}

/// The context of the hooks, the port of the config and of the log of the C
/// `BINKD_CONFIG`.
#[derive(Clone, Default)]
pub struct Options {
    /// The log of the helpers, or `None` for no output.
    pub log: Option<Arc<dyn LogSink>>,
    /// The domain table of the config, the port of `cfg->pDomains`.
    pub domains: DomainTable,
    /// The main addresses of the config, the port of `cfg->pAddr`.
    pub aka: Vec<FtnAddr>,
    /// The queue of the session messages, the port of the `STATE` of the
    /// session.
    pub sender: Option<Arc<dyn MessageSink>>,
    /// The config of the process, the port of the `BINKD_CONFIG` of
    /// `perl_setup()`.
    pub config: Option<Config>,
}

/// The session options of `%opt`, the port of the hash of `setup_session()`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Opt {
    /// The ND mode, the port of `$opt{ND}`.
    pub nd: i32,
    /// The NR mode, the port of `$opt{NR}`.
    pub nr: i32,
    /// The MD mode, the port of `$opt{MD}`.
    pub md: i32,
    /// The crypt mode, the port of `$opt{crypt}`.
    pub crypt: i32,
    /// The compression methods of the peer, the port of `$opt{GZ}`.
    pub gz: i32,
}

/// A file of the session queue, the port of an entry of `@queue`.
#[derive(Clone, Debug, Default)]
pub struct QueueFile {
    /// The path of the file, the port of `$queue[n]{file}`.
    pub file: String,
    /// The size, the port of `$queue[n]{size}`.
    pub size: u64,
    /// The time, the port of `$queue[n]{time}`.
    pub time: i64,
    /// The sent flag, the port of `$queue[n]{sent}`.
    pub sent: i32,
    /// The flavour, the port of `$queue[n]{flvr}`.
    pub flvr: char,
    /// The action, the port of `$queue[n]{action}`.
    pub action: char,
    /// The type, the port of `$queue[n]{type}`.
    pub type_: char,
    /// The address of the receiver, the port of `$queue[n]{addr}`.
    pub addr: FtnAddr,
}

/// The values of a session, the port of the parts of `STATE` that
/// `setup_session()` sets in the Perl variables.
#[derive(Clone, Debug, Default)]
pub struct SessionVars {
    /// The session answers a call, the port of `$call`.
    pub call: bool,
    /// The start time, the port of `$start`.
    pub start: i64,
    /// The name of the peer, the port of `$host`.
    pub host: String,
    /// The address of the peer, the port of `$ip`.
    pub ip: String,
    /// Our address, the port of `$our_ip`.
    pub our_ip: String,
    /// Our port, the port of `$our_port`.
    pub our_port: i32,
    /// The security state, the port of `$secure`.
    pub secure: i32,
    /// The name of the peer system, the port of `$sysname`.
    pub sysname: String,
    /// The sysop of the peer, the port of `$sysop`.
    pub sysop: String,
    /// The location of the peer, the port of `$location`.
    pub location: String,
    /// The mail traffic of the queue, the port of `$traf_mail`.
    pub traffic_mail: u64,
    /// The file traffic of the queue, the port of `$traf_file`.
    pub traffic_file: u64,
    /// The addresses of the peer, the port of `@he`.
    pub he: Vec<FtnAddr>,
    /// Our addresses, the port of `@me`.
    pub me: Vec<FtnAddr>,
    /// The send rate, the port of `$bw_send`.
    pub bw_send: i64,
    /// The receive rate, the port of `$bw_recv`.
    pub bw_recv: i64,
    /// The received bytes, the port of `$bytes_rcvd`.
    pub bytes_rcvd: u64,
    /// The sent bytes, the port of `$bytes_sent`.
    pub bytes_sent: u64,
    /// The received files, the port of `$files_rcvd`.
    pub files_rcvd: u64,
    /// The sent files, the port of `$files_sent`.
    pub files_sent: u64,
    /// The compression method of the sent files, the port of `$z_send`.
    pub z_send: i32,
    /// The compression method of the received files, the port of `$z_recv`.
    pub z_recv: i32,
    /// The session options, the port of `%opt`.
    pub opt: Opt,
    /// The session queue, the port of `@queue`.
    pub queue: Vec<QueueFile>,
}

/// The values of the hook of the handshake, the port of `perl_on_handshake()`.
#[derive(Clone, Debug, Default)]
pub struct Handshake {
    /// The session values.
    pub session: SessionVars,
    /// The addresses of the peer, the port of `@he`.
    pub he: Vec<FtnAddr>,
    /// Our addresses that the hook sets, the port of `@me`.
    pub me: Option<Vec<FtnAddr>>,
    /// The expected password that the hook sets, the port of `$passwd`.
    pub password: Option<String>,
    /// The mail traffic of the queue, the port of `$traf_mail`.
    pub traffic_mail: u64,
    /// The file traffic of the queue, the port of `$traf_file`.
    pub traffic_file: u64,
}

/// The values of the hook of an outgoing call, the port of `perl_on_call()`.
#[derive(Clone, Debug)]
pub struct Call {
    /// The address of the node, the port of `$addr`.
    pub addr: FtnAddr,
    /// The hosts of the node, the port of `$hosts`.
    pub hosts: String,
    /// The proxy, the port of `$proxy`.
    pub proxy: String,
    /// The SOCKS proxy, the port of `$socks`.
    pub socks: String,
    /// The mail traffic of the queue, the port of `$traf_mail`.
    pub traffic_mail: u64,
    /// The file traffic of the queue, the port of `$traf_file`.
    pub traffic_file: u64,
}

/// The values of the hook of a failed call, the port of `perl_on_error()`.
#[derive(Clone, Debug, Default)]
pub struct CallError {
    /// The address of the node, the port of `$addr`.
    pub addr: FtnAddr,
    /// The error, the port of `$error`.
    pub error: String,
    /// The place of the error, the port of `$where`.
    pub location: i32,
}

/// The values of a file of a hook, the port of the `TFILE` reads.
#[derive(Clone, Debug, Default)]
pub struct File {
    /// The path, the port of `$file`.
    pub path: String,
    /// The netname, the port of `$name`.
    pub name: String,
    /// The size, the port of `$size`.
    pub size: u64,
    /// The time, the port of `$time`.
    pub time: i64,
}

/// The values of a received file, the port of `perl_after_recv()`.
#[derive(Clone, Debug, Default)]
pub struct Received {
    /// The netname, the port of `$name`.
    pub name: String,
    /// The size, the port of `$size`.
    pub size: u64,
    /// The time, the port of `$time`.
    pub time: i64,
    /// The temporary name, the port of `$tmpfile`.
    pub tmp: String,
    /// The real name, the port of `$file`.
    pub file: String,
}

/// The values of a sent file, the port of `perl_after_sent()`.
#[derive(Clone, Debug, Default)]
pub struct Sent {
    /// The path, the port of `$file`.
    pub path: String,
    /// The netname, the port of `$name`.
    pub name: String,
    /// The size, the port of `$size`.
    pub size: u64,
    /// The time, the port of `$time`.
    pub time: i64,
    /// The start time, the port of `$start`.
    pub start: i64,
    /// The action, the port of `$action`.
    pub action: char,
}

/// The hooks of a script, the port of the state of `perlhooks.c`.
///
/// The C code holds the interpreter and the set of the hooks that the script
/// answers in `BINKD_CONFIG`, the port of `cfg->perl` and of `cfg->perl_ok`. The
/// port holds the two in one value, so a caller may run one set of hooks beside
/// other code.
///
/// A value belongs to one thread at a time. The context of the helpers and the
/// guard against a recursive call of the log hook are thread local. The log of
/// the port serializes the master value with the mutex of `LogHook`, and every
/// session holds a clone from [`Hooks::clone_for_session`]. Do not share a
/// value directly across threads and do not make the guards process wide.
pub struct Hooks {
    /// The interpreter, the port of `cfg->perl`.
    perl: *mut Interpreter,
    /// The name of the script, the port of `cfg->perl_script`.
    pub script: String,
    /// The subs that the script defines, the port of `cfg->perl_ok`.
    ok: u32,
    /// The log of the helpers, the port of the C `Log()`.
    log: Option<Arc<dyn LogSink>>,
    /// The domain table, the port of `cfg->pDomains`.
    domains: DomainTable,
    /// The main addresses, the port of `cfg->pAddr`.
    aka: Vec<FtnAddr>,
    /// The queue of the session messages, the port of the session `STATE`.
    sender: Option<Arc<dyn MessageSink>>,
    /// The config of the process, the port of the `cfg` of `perl_setup()`.
    config: Option<Config>,
    /// The interpreter is a clone of a session, the port of
    /// `perl_init_clone()`.
    is_clone: bool,
}

// The interpreter of one value of the port belongs to one thread of the caller,
// the port of the interpreter of one process of the C code.
unsafe impl Send for Hooks {}

/// Starts the interpreter library once, the port of the first call of
/// `PERL_SYS_INIT3()` of `perl_init()`.
static INIT: Once = Once::new();

thread_local! {
    /// The hooks of the call of Perl that runs on this thread, the port of
    /// `current_config` and of the session `STATE` of the C code.
    static CURRENT: Cell<*const Hooks> = const { Cell::new(std::ptr::null()) };
}

thread_local! {
    /// The hook of the log runs on this thread, the port of the `__in_log_`
    /// value of `perl_on_log()`.
    static IN_LOG: Cell<bool> = const { Cell::new(false) };
}

/// Clears the mark of a running hook on the way out, even after a panic.
struct Running;

impl Drop for Running {
    fn drop(&mut self) {
        IN_LOG.with(|flag| flag.set(false));
    }
}

/// Sets the hooks of the call of Perl and restores the old value on the way out.
struct ContextGuard {
    previous: *const Hooks,
}

impl ContextGuard {
    fn enter(hooks: &Hooks) -> ContextGuard {
        let previous = CURRENT.with(|cell| cell.replace(hooks as *const Hooks));
        ContextGuard { previous }
    }
}

impl Drop for ContextGuard {
    fn drop(&mut self) {
        CURRENT.with(|cell| cell.set(self.previous));
    }
}

/// Calls a function with the hooks of the current call of Perl.
pub(crate) fn with_current<R>(f: impl FnOnce(&Hooks) -> R) -> Option<R> {
    let ptr = CURRENT.with(|cell| cell.get());
    if ptr.is_null() {
        None
    } else {
        // SAFETY: the guard sets the pointer for the run of the call on this
        // thread, and the value lives for the call.
        Some(unsafe { f(&*ptr) })
    }
}

impl Hooks {
    /// Loads a script without a config, the port of `perl_init()` of
    /// `perlhooks.c`.
    pub fn load(script: &str) -> Option<Hooks> {
        Hooks::load_with(script, Options::default())
    }

    /// Loads a script with the config and the sinks of the binary, the port of
    /// `perl_init()` of `perlhooks.c`.
    ///
    /// The C code creates the interpreter, it reads the script, and it runs the
    /// body, so the script holds its subs. The port does the same, and it holds
    /// the context of the helpers.
    pub fn load_with(script: &str, options: Options) -> Option<Hooks> {
        INIT.call_once(|| unsafe { ffi::binkd_perl_sys_init() });
        // The port of the `access(perlfile, R_OK)` check of `perl_init()`.
        if std::fs::metadata(script).is_err() {
            return None;
        }
        let path = CString::new(script).ok()?;
        let arg0 = CString::new("").ok()?;
        let arg1 = CString::new("-e").ok()?;
        let arg2 = CString::new("0").ok()?;
        let perl = unsafe { ffi::binkd_perl_alloc() };
        if perl.is_null() {
            return None;
        }
        unsafe {
            ffi::binkd_perl_construct(perl);
        }
        let mut args: Vec<*mut c_char> = vec![
            arg0.as_ptr() as *mut c_char,
            arg1.as_ptr() as *mut c_char,
            arg2.as_ptr() as *mut c_char,
            std::ptr::null_mut(),
        ];
        let rc = unsafe { ffi::binkd_perl_parse(perl, 3, args.as_mut_ptr(), std::ptr::null_mut()) };
        if rc != 0 {
            unsafe { ffi::binkd_perl_free(perl) };
            return None;
        }
        // The port of the warn and die hooks of `perl_init()`.
        unsafe { ffi::binkd_perl_set_warn_hook(perl) };
        let mut hooks = Hooks {
            perl,
            script: script.to_string(),
            ok: 0,
            log: options.log,
            domains: options.domains,
            aka: options.aka,
            sender: options.sender,
            config: options.config,
            is_clone: false,
        };
        // The port of `perl_setup()` of `perl_init()`: the globals hold the
        // config before the body of the script runs.
        let context = ContextGuard::enter(&hooks);
        hooks.setup_config();
        // The port of the `do '<file>'; $@ ? $@ : '';` of `perl_init()`. The
        // helpers of the script see the context of the hooks.
        let rc = unsafe { ffi::binkd_perl_do_file(perl, path.as_ptr()) };
        drop(context);
        if rc != 0 {
            // The value frees the interpreter on the way out.
            return None;
        }
        // The port of the scan of `perl_subnames[]` of `perl_init()`, the
        // port of the bits of `cfg->perl_ok`.
        for (i, name) in bridge::SUBNAMES.iter().enumerate() {
            if let Ok(name) = CString::new(*name) {
                if unsafe { ffi::binkd_perl_has_sub(perl, name.as_ptr()) != 0 } {
                    hooks.ok |= 1 << i;
                }
            }
        }
        Some(hooks)
    }

    /// Clones the interpreter for one session, the port of
    /// `perl_init_clone()` of `perlhooks.c`.
    ///
    /// The clone shares the script and the subs, but it holds its own globals,
    /// so two sessions do not share the values of the script. The caller ends
    /// the clone with the value, the port of `perl_done_clone()`.
    pub fn clone_for_session(&self) -> Option<Hooks> {
        let _context = ContextGuard::enter(self);
        let perl = unsafe { ffi::binkd_perl_clone(self.perl) };
        if perl.is_null() {
            return None;
        }
        let clone = Hooks {
            perl,
            script: self.script.clone(),
            ok: self.ok,
            log: self.log.clone(),
            domains: self.domains.clone(),
            aka: self.aka.clone(),
            sender: self.sender.clone(),
            config: self.config.clone(),
            is_clone: true,
        };
        // The port of `perl_setup(cfg, p)` of `perl_init_clone()`: a clone
        // holds its own globals, so the config goes in again.
        {
            let _context = ContextGuard::enter(&clone);
            clone.setup_config();
        }
        Some(clone)
    }

    /// Sets the sink of the messages of `msg_send()`, the port of the
    /// `STATE` of `msg_send2()`.
    pub fn set_message_sink(&mut self, sender: Option<Arc<dyn MessageSink>>) {
        self.sender = sender;
    }

    /// Sets the globals of `perl_setup()` of `perlhooks.c`.
    fn setup_config(&self) {
        let Some(config) = &self.config else {
            return;
        };
        self.setup_config_hash(config);
        self.setup_domain_hash(config);
        self.setup_addr_array(config);
        self.setup_listen_array(config);
        self.setup_ftrans_array(config);
        self.setup_overwrite_array(config);
        self.setup_skip_array(config);
        self.setup_share_hash(config);
        self.setup_node_hash(config);
    }

    /// Sets `%config`, the port of the first block of `perl_setup()`.
    fn setup_config_hash(&self, config: &Config) {
        let hash = self.global_hash("config");
        self.store_string(hash, "log", &config.logpath);
        self.store_int(hash, "loglevel", config.loglevel as i64);
        self.store_int(hash, "conlog", config.conlog as i64);
        self.store_int(hash, "tzoff", config.tzoff as i64);
        self.store_string(hash, "sysname", &config.sysname);
        self.store_string(hash, "sysop", &config.sysop);
        self.store_string(hash, "location", &config.location);
        self.store_string(hash, "nodeinfo", &config.nodeinfo);
        self.store_string(hash, "bindaddr", &config.bindaddr);
        self.store_string(hash, "iport", &config.iport);
        self.store_string(hash, "oport", &config.oport);
        self.store_int(hash, "maxservers", config.max_servers as i64);
        self.store_int(hash, "maxclients", config.max_clients as i64);
        self.store_int(hash, "oblksize", config.oblksize as i64);
        self.store_string(hash, "inbound", &config.inbound);
        self.store_string(hash, "inbound_nonsecure", &config.spool.inbound_nonsecure);
        self.store_int(hash, "inboundcase", inb_case(config.spool.inboundcase));
        self.store_string(hash, "temp_inbound", &config.spool.temp_inbound);
        self.store_int(hash, "minfree", config.spool.minfree);
        self.store_int(hash, "minfree_nonsecure", config.spool.minfree_nonsecure);
        self.store_int(hash, "hold", config.spool.hold);
        self.store_int(hash, "hold_skipped", config.hold_skipped as i64);
        self.store_int(hash, "backresolv", config.backresolv as i64);
        self.store_int(hash, "send_if_pwd", config.spool.send_if_pwd as i64);
        self.store_string(hash, "filebox", &config.spool.tfilebox);
        self.store_string(hash, "brakebox", &config.spool.bfilebox);
        self.store_string(hash, "root_domain", &config.root_domain);
        // The C `VK_ADD_HASH_int` stores only a nonzero value.
        if config.spool.pkthdr_type != 0 {
            self.store_int(hash, "check_pkthdr", config.spool.pkthdr_type as i64);
        }
        match &config.spool.pkthdr_bad {
            Some(bad) => self.store_string(hash, "pkthdr_badext", bad),
            None => self.store_undef(hash, "pkthdr_badext"),
        }
        self.store_int(hash, "zminsize", config.zminsize as i64);
        for var in &config.perl_vars {
            self.store_string(hash, &var.name, &var.value);
        }
    }

    /// Sets `%domain`, the port of the domain block of `perl_setup()`.
    fn setup_domain_hash(&self, config: &Config) {
        let hash = self.global_hash("domain");
        let domains = config.spool.domains.domains();
        for domain in domains {
            // The C code takes the path, the dir, and the zone of the alias.
            let source = match domain.alias4 {
                Some(index) => domains.get(index).unwrap_or(domain),
                None => domain,
            };
            let inner = self.store_new_hash(hash, &domain.name, false);
            if inner.is_null() {
                continue;
            }
            self.store_string(inner, "path", &source.path);
            self.store_string(inner, "dir", &source.dir);
            if source.z[0] != 0 {
                self.store_int(inner, "defzone", source.z[0] as i64);
            }
            if let Some(idomain) = &domain.idomain {
                self.store_string(inner, "root_domain", idomain);
            }
        }
    }

    /// Sets `@addr`, the port of the address block of `perl_setup()`.
    fn setup_addr_array(&self, config: &Config) {
        let array = self.global_array("addr");
        for fa in &config.spool.aka {
            self.push_string(array, &fa.format(false));
        }
    }

    /// Sets `@listen`, the port of the listen block of `perl_setup()`.
    fn setup_listen_array(&self, config: &Config) {
        let array = self.global_array("listen");
        for entry in &config.listen {
            let inner = self.push_new_hash(array, true);
            if inner.is_null() {
                continue;
            }
            self.store_string(inner, "port", &entry.port);
            let addr = if entry.addr.is_empty() {
                "*"
            } else {
                &entry.addr
            };
            self.store_string(inner, "addr", addr);
        }
    }

    /// Sets `@ftrans`, the port of the ftrans block of `perl_setup()`.
    fn setup_ftrans_array(&self, config: &Config) {
        let array = self.global_array("ftrans");
        for rule in &config.rf_rules {
            let inner = self.push_new_hash(array, false);
            if inner.is_null() {
                continue;
            }
            self.store_string(inner, "from", &rule.from);
            self.store_string(inner, "to", &rule.to);
        }
    }

    /// Sets `@overwrite`, the port of the overwrite block of `perl_setup()`.
    fn setup_overwrite_array(&self, config: &Config) {
        let array = self.global_array("overwrite");
        for mask in &config.spool.overwrite {
            self.push_string(array, mask);
        }
    }

    /// Sets `@skip`, the port of the skip block of `perl_setup()`.
    fn setup_skip_array(&self, config: &Config) {
        let array = self.global_array("skip");
        for rule in &config.skipmask {
            let inner = self.push_new_hash(array, false);
            if inner.is_null() {
                continue;
            }
            self.store_string(inner, "mask", &rule.mask);
            self.store_int(inner, "type", rule.atype as i64);
            self.store_int(inner, "size", rule.size);
            self.store_int(inner, "destr", rule.destr as i64);
        }
    }

    /// Sets `%share`, the port of the share block of `perl_setup()`.
    fn setup_share_hash(&self, config: &Config) {
        let hash = self.global_hash("share");
        for share in &config.spool.shares {
            let array = self.store_new_array(hash, &share.sha.format(false));
            for fa in &share.addrs {
                self.push_string(array, &fa.format(false));
            }
        }
    }

    /// Sets `%node`, the port of the node block of `perl_setup()`.
    fn setup_node_hash(&self, config: &Config) {
        let hash = self.global_hash("node");
        for node in config.spool.nodes.nodes() {
            let inner = self.store_new_hash(hash, &node.fa.format(false), true);
            if inner.is_null() {
                continue;
            }
            match &node.hosts {
                Some(hosts) => self.store_string(inner, "hosts", hosts),
                None => self.store_undef(inner, "hosts"),
            }
            self.store_string(inner, "pwd", &node.pwd);
            self.store_password(inner, "pkt_pwd", &node.pkt_pwd, &node.pwd);
            self.store_password(inner, "out_pwd", &node.out_pwd, &node.pwd);
            match &node.ibox {
                Some(ibox) => self.store_string(inner, "ibox", ibox),
                None => self.store_undef(inner, "ibox"),
            }
            match &node.obox {
                Some(obox) => self.store_string(inner, "obox", obox),
                None => self.store_undef(inner, "obox"),
            }
            self.store_string(inner, "obox_flvr", &flavour(node.obox_flvr));
            for (key, value) in [
                ("NR", node.nr_flag),
                ("ND", node.nd_flag),
                ("MD", node.md_flag),
                ("HC", node.hc_flag),
                ("IP", node.restrict_ip),
                ("NP", node.np_flag),
            ] {
                // The C `VK_ADD_HASH_int` stores only a nonzero value.
                if value != 0 {
                    self.store_int(inner, key, value as i64);
                }
            }
        }
    }

    /// Stores a password of `%node`, the port of the `pkt_pwd` and `out_pwd`
    /// reads of `add_node_to_hv()`.
    fn store_password(&self, hash: *mut Value, key: &str, pwd: &NodePassword, main: &str) {
        match pwd {
            NodePassword::None => self.store_undef(hash, key),
            NodePassword::Main => self.store_string(hash, key, main),
            NodePassword::Value(value) => self.store_string(hash, key, value),
        }
    }

    /// Returns a global hash and clears it, the port of `perl_get_hv()` and
    /// `hv_clear()` of `perl_setup()`.
    fn global_hash(&self, name: &str) -> *mut Value {
        let name = cstr(name);
        unsafe {
            let hash = ffi::binkd_perl_get_hv(self.perl, name.as_ptr(), 1);
            if !hash.is_null() {
                ffi::binkd_perl_hv_clear(self.perl, hash);
            }
            hash
        }
    }

    /// Returns a global array and clears it, the port of `perl_get_av()` and
    /// `av_clear()` of `perl_setup()`.
    fn global_array(&self, name: &str) -> *mut Value {
        let name = cstr(name);
        unsafe {
            let array = ffi::binkd_perl_get_av(self.perl, name.as_ptr(), 1);
            if !array.is_null() {
                ffi::binkd_perl_av_clear(self.perl, array);
            }
            array
        }
    }

    /// Stores a string in a hash, the port of `VK_ADD_HASH_str`.
    fn store_string(&self, hash: *mut Value, key: &str, value: &str) {
        if hash.is_null() {
            return;
        }
        let key = cstr(key);
        let value = cstr(value);
        unsafe {
            ffi::binkd_perl_hv_store_string(self.perl, hash, key.as_ptr(), value.as_ptr());
        }
    }

    /// Stores an integer in a hash, the port of `VK_ADD_HASH_intz`.
    fn store_int(&self, hash: *mut Value, key: &str, value: i64) {
        if hash.is_null() {
            return;
        }
        let key = cstr(key);
        unsafe {
            ffi::binkd_perl_hv_store_int_ro(self.perl, hash, key.as_ptr(), value);
        }
    }

    /// Stores an undefined value in a hash, the port of the undef branch of
    /// `VK_ADD_HASH_str`.
    fn store_undef(&self, hash: *mut Value, key: &str) {
        if hash.is_null() {
            return;
        }
        let key = cstr(key);
        unsafe {
            ffi::binkd_perl_hv_store_undef(self.perl, hash, key.as_ptr());
        }
    }

    /// Stores a new hash as a reference and answers it, the port of
    /// `VK_ADD_HASH_sv` with `newHV()`.
    fn store_new_hash(&self, hash: *mut Value, key: &str, readonly: bool) -> *mut Value {
        if hash.is_null() {
            return std::ptr::null_mut();
        }
        let key = cstr(key);
        unsafe {
            ffi::binkd_perl_hv_store_new_hash(
                self.perl,
                hash,
                key.as_ptr(),
                if readonly { 1 } else { 0 },
            )
        }
    }

    /// Stores a new array as a reference and answers it.
    fn store_new_array(&self, hash: *mut Value, key: &str) -> *mut Value {
        if hash.is_null() {
            return std::ptr::null_mut();
        }
        let key = cstr(key);
        unsafe { ffi::binkd_perl_hv_store_new_array(self.perl, hash, key.as_ptr()) }
    }

    /// Appends a read-only string to an array, the port of `av_push()` with
    /// `newSVpv()`.
    fn push_string(&self, array: *mut Value, value: &str) {
        if array.is_null() {
            return;
        }
        let value = cstr(value);
        unsafe {
            ffi::binkd_perl_av_push_string_ro(self.perl, array, value.as_ptr());
        }
    }

    /// Appends a new hash as a reference and answers it.
    fn push_new_hash(&self, array: *mut Value, readonly: bool) -> *mut Value {
        if array.is_null() {
            return std::ptr::null_mut();
        }
        unsafe { ffi::binkd_perl_av_push_new_hash(self.perl, array, if readonly { 1 } else { 0 }) }
    }

    /// Calls the hook of the log, the port of `perl_on_log()` of `perlhooks.c`.
    ///
    /// The C code sets `$_` to the message and `$lvl` to the level, it calls the
    /// sub, and a true return replaces the two. The port does the same. The
    /// answer is true when the message goes to the output, and false when the
    /// hook suppresses it: the port of the `rc` of the C code.
    ///
    /// The guard against a recursive call is thread local, the port of the
    /// `__in_log_` value of the C code with one value for the thread. Two
    /// sessions on two threads may call the hook; `LogHook` serializes the
    /// master value. Do not make this guard process wide: the second session
    /// would skip the hook.
    pub fn on_log(&self, message: &mut Vec<u8>, level: &mut i32) -> bool {
        // The port of the `cfg->perl_ok & (1 << PERL_ON_LOG)` check of the C
        // code: a script without the sub keeps the message.
        if !self.defined(ON_LOG) {
            return true;
        }
        // The port of the check of `perl_on_log()` against the recursion. The
        // C code holds `__in_log_<pid>` for the process; the port holds the
        // mark for the thread, so two sessions may call the hook.
        let enter = IN_LOG.with(|flag| {
            if flag.get() {
                false
            } else {
                flag.set(true);
                true
            }
        });
        if !enter {
            return true;
        }
        let _running = Running;
        let _context = ContextGuard::enter(self);
        self.call_on_log(message, level)
    }

    /// Calls the sub of the log, the port of the body of `perl_on_log()`.
    fn call_on_log(&self, message: &mut Vec<u8>, level: &mut i32) -> bool {
        let text = match CString::new(message.clone()) {
            Ok(text) => text,
            Err(_) => return true,
        };
        let name = match CString::new(bridge::SUBNAMES[ON_LOG]) {
            Ok(name) => name,
            Err(_) => return true,
        };
        let message_name = match CString::new("_") {
            Ok(name) => name,
            Err(_) => return true,
        };
        let level_name = match CString::new("lvl") {
            Ok(name) => name,
            Err(_) => return true,
        };
        unsafe {
            // The C code sets `$_` and `$lvl` before the call.
            let value = ffi::binkd_perl_get_sv(self.perl, message_name.as_ptr(), 1);
            if value.is_null() {
                return true;
            }
            ffi::binkd_perl_set_string(self.perl, value, text.as_ptr());
            let lvl = ffi::binkd_perl_get_sv(self.perl, level_name.as_ptr(), 1);
            if !lvl.is_null() {
                ffi::binkd_perl_set_int(self.perl, lvl, *level as i64);
            }
            let mut result: i64 = 0;
            let defined =
                ffi::binkd_perl_call(self.perl, name.as_ptr(), G_EVAL | G_SCALAR, &mut result);
            if ffi::binkd_perl_error(self.perl) != 0 {
                return true;
            }
            // The C code answers 0 for an undefined value and for the value 0.
            if defined == 0 || result == 0 {
                return false;
            }
            let mut len: bridge::Length = 0;
            let text = ffi::binkd_perl_string(self.perl, value, &mut len);
            if text.is_null() || len == 0 {
                return false;
            }
            let bytes = std::slice::from_raw_parts(text as *const u8, len).to_vec();
            // The C code answers 0 for an empty `$_`.
            if bytes.is_empty() || bytes[0] == 0 {
                return false;
            }
            *message = bytes;
            let lvl = ffi::binkd_perl_get_sv(self.perl, level_name.as_ptr(), 0);
            if !lvl.is_null() {
                *level = ffi::binkd_perl_int(self.perl, lvl) as i32;
            }
        }
        true
    }

    /// Writes a line through the log sink, the port of `perl_Log()`.
    pub(crate) fn log(&self, level: i32, message: &str) {
        if let Some(sink) = &self.log {
            sink.log(level, message);
        }
    }

    /// Compares an address with a list of addresses or one mask, the port of
    /// `perl_aeq()`.
    ///
    /// The answer is `None` for an undefined result, the port of `XSRETURN_UNDEF`.
    pub fn aeq(&self, first: &str, others: &[&str]) -> Option<bool> {
        if first.is_empty() {
            return None;
        }
        let mask_mode = first.contains('*');
        let a = if mask_mode {
            None
        } else {
            let mut a = FtnAddr::parse(first, &self.domains)?;
            a.expand(&self.aka, &self.domains);
            Some(a)
        };
        for other in others {
            if other.is_empty() {
                continue;
            }
            let Some(mut b) = FtnAddr::parse(other, &self.domains) else {
                continue;
            };
            b.expand(&self.aka, &self.domains);
            let matched = if mask_mode {
                pmatch_ncase(first.as_bytes(), b.format(true).as_bytes())
            } else {
                a.as_ref().map(|a| a.compare(&b) == 0).unwrap_or(false)
            };
            if matched {
                return Some(true);
            }
        }
        Some(false)
    }

    /// Removes the addresses of a list from an array, the port of `perl_arm()`.
    ///
    /// The answer holds one flag for every element: true keeps the element and
    /// false removes it.
    pub fn arm(&self, elements: &[Option<&str>], args: &[&str]) -> Vec<bool> {
        let mut addresses = Vec::new();
        let mut masks = Vec::new();
        for arg in args {
            if arg.is_empty() {
                continue;
            }
            if arg.contains('*') {
                masks.push(*arg);
                continue;
            }
            if let Some(mut fa) = FtnAddr::parse(arg, &self.domains) {
                fa.expand(&self.aka, &self.domains);
                addresses.push(fa);
            }
        }
        let mut keep = Vec::with_capacity(elements.len());
        for element in elements {
            let Some(text) = element else {
                keep.push(true);
                continue;
            };
            if text.is_empty() {
                keep.push(true);
                continue;
            }
            let Some(mut fa) = FtnAddr::parse(text, &self.domains) else {
                keep.push(true);
                continue;
            };
            fa.expand(&self.aka, &self.domains);
            let found = addresses.iter().any(|b| fa.compare(b) == 0)
                || masks
                    .iter()
                    .any(|mask| pmatch_ncase(mask.as_bytes(), fa.format(true).as_bytes()));
            keep.push(!found);
        }
        keep
    }

    /// Queues a message, the port of `perl_msg_send()`.
    pub(crate) fn msg_send(&self, kind: u8, text: &str) {
        match &self.sender {
            Some(sender) => sender.send(kind, text),
            // The port of the `FIND_STATE()` error of `perlhooks.c`.
            None => self.log(ERROR, "can't find $__state pointer"),
        }
    }

    /// Logs the lines of a Perl warning or error, the port of `perl_warn_str()`.
    pub(crate) fn warn(&self, text: &str) {
        for line in text.split_terminator('\n') {
            self.log(ERROR, &format!("Perl error: {line}"));
        }
    }

    /// Returns true when the script defines a sub, the port of a bit of
    /// `cfg->perl_ok`.
    fn defined(&self, index: usize) -> bool {
        self.ok & (1 << index) != 0
    }

    /// Calls a sub in scalar context and returns its integer, the port of the
    /// `G_EVAL|G_SCALAR` calls of `perlhooks.c`.
    fn call_int(&self, index: usize) -> Option<i64> {
        let _context = ContextGuard::enter(self);
        let name = CString::new(bridge::SUBNAMES[index]).ok()?;
        let mut result: i64 = 0;
        let defined = unsafe {
            ffi::binkd_perl_call(self.perl, name.as_ptr(), G_EVAL | G_SCALAR, &mut result)
        };
        if unsafe { ffi::binkd_perl_error(self.perl) } != 0 {
            self.sub_error(index);
            return Some(result);
        }
        if defined == 0 {
            None
        } else {
            Some(result)
        }
    }

    /// Calls a sub in void context, the port of the `G_EVAL|G_VOID` calls of
    /// `perlhooks.c`.
    fn call_void(&self, index: usize) {
        let _context = ContextGuard::enter(self);
        let Ok(name) = CString::new(bridge::SUBNAMES[index]) else {
            return;
        };
        if unsafe { ffi::binkd_perl_call_void(self.perl, name.as_ptr()) } != 0 {
            self.sub_error(index);
        }
    }

    /// Calls a sub in scalar context and returns the true string, the port of
    /// the string returns of `perl_on_handshake()` and
    /// `perl_after_handshake()`.
    fn call_string(&self, index: usize) -> Result<Option<String>, ()> {
        let _context = ContextGuard::enter(self);
        let name = CString::new(bridge::SUBNAMES[index]).map_err(|_| ())?;
        let mut defined: i32 = 0;
        let mut len: bridge::Length = 0;
        let text = unsafe {
            ffi::binkd_perl_call_string(self.perl, name.as_ptr(), &mut defined, &mut len)
        };
        if defined < 0 {
            self.sub_error(index);
            return Err(());
        }
        if defined == 0 || text.is_null() {
            return Ok(None);
        }
        let bytes = unsafe { std::slice::from_raw_parts(text as *const u8, len).to_vec() };
        unsafe { ffi::binkd_perl_free_string(text) };
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }

    /// Logs the error of a sub, the port of `sub_err()`.
    fn sub_error(&self, index: usize) {
        let text = unsafe { ffi::binkd_perl_error_string(self.perl) };
        if text.is_null() {
            return;
        }
        let message = unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned();
        unsafe { ffi::binkd_perl_free_string(text) };
        for line in message.split_terminator('\n') {
            self.log(
                ERROR,
                &format!("Perl {} error: {}", bridge::SUBNAMES[index], line),
            );
        }
    }

    /// Sets a global string, the port of `VK_ADD_str()`.
    fn set_str(&self, name: &str, value: &str) {
        let (Ok(name), Ok(value)) = (CString::new(name), CString::new(value)) else {
            return;
        };
        unsafe {
            let sv = ffi::binkd_perl_get_sv(self.perl, name.as_ptr(), 1);
            if !sv.is_null() {
                ffi::binkd_perl_set_string(self.perl, sv, value.as_ptr());
            }
        }
    }

    /// Sets a global integer, the port of `VK_ADD_intz()`.
    fn set_int(&self, name: &str, value: i64) {
        let Ok(name) = CString::new(name) else {
            return;
        };
        unsafe {
            let sv = ffi::binkd_perl_get_sv(self.perl, name.as_ptr(), 1);
            if !sv.is_null() {
                ffi::binkd_perl_set_int(self.perl, sv, value);
            }
        }
    }

    /// Sets a global value to bytes, the port of `sv_setpvn()`.
    fn set_bytes(&self, name: &str, value: &[u8]) {
        let Ok(name) = CString::new(name) else {
            return;
        };
        unsafe {
            let sv = ffi::binkd_perl_get_sv(self.perl, name.as_ptr(), 1);
            if !sv.is_null() {
                ffi::binkd_perl_set_bytes(
                    self.perl,
                    sv,
                    value.as_ptr() as *const c_char,
                    value.len(),
                );
            }
        }
    }

    /// Returns a global string, the port of `SvPV()`.
    fn get_str(&self, name: &str) -> Option<String> {
        let name = CString::new(name).ok()?;
        unsafe {
            let sv = ffi::binkd_perl_get_sv(self.perl, name.as_ptr(), 0);
            if sv.is_null() || ffi::binkd_perl_ok(self.perl, sv) == 0 {
                return None;
            }
            let mut len = 0;
            let text = ffi::binkd_perl_string(self.perl, sv, &mut len);
            if text.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(text as *const u8, len).to_vec();
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
    }

    /// Returns a global integer, the port of `SvIV()`.
    fn get_int(&self, name: &str) -> Option<i64> {
        let name = CString::new(name).ok()?;
        unsafe {
            let sv = ffi::binkd_perl_get_sv(self.perl, name.as_ptr(), 0);
            if sv.is_null() || ffi::binkd_perl_ok(self.perl, sv) == 0 {
                return None;
            }
            Some(ffi::binkd_perl_int(self.perl, sv))
        }
    }

    /// Sets a global array of strings, the port of `av_clear()` and `av_push()`.
    fn set_array(&self, name: &str, values: &[String]) {
        let Ok(name) = CString::new(name) else {
            return;
        };
        unsafe {
            let array = ffi::binkd_perl_get_av(self.perl, name.as_ptr(), 1);
            if array.is_null() {
                return;
            }
            ffi::binkd_perl_av_clear(self.perl, array);
            for value in values {
                if let Ok(text) = CString::new(value.as_str()) {
                    ffi::binkd_perl_av_push_string(self.perl, array, text.as_ptr());
                }
            }
        }
    }

    /// Returns a global array of strings, or `None` for an undefined array.
    fn get_array(&self, name: &str) -> Option<Vec<String>> {
        let name = CString::new(name).ok()?;
        unsafe {
            let array = ffi::binkd_perl_get_av(self.perl, name.as_ptr(), 0);
            if array.is_null() {
                return None;
            }
            let len = ffi::binkd_perl_av_len(self.perl, array);
            let mut out = Vec::new();
            for i in 0..=len {
                let mut defined = 0;
                let mut length = 0;
                let text =
                    ffi::binkd_perl_av_fetch_string(self.perl, array, i, &mut defined, &mut length);
                if defined == 0 || text.is_null() {
                    continue;
                }
                let bytes = std::slice::from_raw_parts(text as *const u8, length).to_vec();
                ffi::binkd_perl_free_string(text);
                out.push(String::from_utf8_lossy(&bytes).into_owned());
            }
            Some(out)
        }
    }

    /// Undefines a global array, the port of `av_undef()`.
    fn undef_array(&self, name: &str) {
        let Ok(name) = CString::new(name) else {
            return;
        };
        unsafe {
            let array = ffi::binkd_perl_get_av(self.perl, name.as_ptr(), 0);
            if !array.is_null() {
                ffi::binkd_perl_av_undef(self.perl, array);
            }
        }
    }

    /// Sets the session values of `setup_session()`, the port of the C code.
    fn set_session(&self, vars: &SessionVars) {
        self.set_int("call", if vars.call { 1 } else { 0 });
        self.set_int("start", vars.start);
        self.set_str("host", &vars.host);
        self.set_str("ip", &vars.ip);
        self.set_str("our_ip", &vars.our_ip);
        self.set_int("our_port", vars.our_port as i64);
        self.set_int("secure", vars.secure as i64);
        self.set_str("sysname", &vars.sysname);
        self.set_str("sysop", &vars.sysop);
        self.set_str("location", &vars.location);
        self.set_int("traf_mail", vars.traffic_mail as i64);
        self.set_int("traf_file", vars.traffic_file as i64);
        self.set_array("he", &addresses(&vars.he));
        self.set_array("me", &addresses(&vars.me));
        self.set_int("bw_send", vars.bw_send);
        self.set_int("bw_recv", vars.bw_recv);
        self.set_int("bytes_rcvd", vars.bytes_rcvd as i64);
        self.set_int("bytes_sent", vars.bytes_sent as i64);
        self.set_int("files_rcvd", vars.files_rcvd as i64);
        self.set_int("files_sent", vars.files_sent as i64);
        self.set_int("z_send", vars.z_send as i64);
        self.set_int("z_recv", vars.z_recv as i64);
        self.set_opt(&vars.opt);
        self.set_queue(&vars.queue);
    }

    /// Sets the array `@queue` of the session, the port of `setup_queue()`.
    fn set_queue(&self, queue: &[QueueFile]) {
        let Ok(name) = CString::new("queue") else {
            return;
        };
        unsafe {
            let array = ffi::binkd_perl_get_av(self.perl, name.as_ptr(), 1);
            if array.is_null() {
                return;
            }
            ffi::binkd_perl_av_clear(self.perl, array);
            for entry in queue {
                let Ok(file) = CString::new(entry.file.as_str()) else {
                    continue;
                };
                let flvr = char_string(entry.flvr);
                let action = char_string(entry.action);
                let type_ = char_string(entry.type_);
                let addr = CString::new(entry.addr.format(false)).unwrap_or_default();
                ffi::binkd_perl_av_push_hash(
                    self.perl,
                    array,
                    file.as_ptr(),
                    entry.size as i64,
                    entry.time,
                    entry.sent as i64,
                    flvr.as_ptr(),
                    action.as_ptr(),
                    type_.as_ptr(),
                    addr.as_ptr(),
                );
            }
        }
    }

    /// Reads the queue that the script changed, the port of `refresh_queue()`
    /// of `perlhooks.c`.
    ///
    /// The answer is `None` when `@queue` is not defined, and the caller keeps
    /// its queue then. An entry without a `file` is skipped, and the `fallback`
    /// answers the address of an entry without a valid `addr`.
    pub fn refresh_queue(&self, fallback: &FtnAddr) -> Option<Vec<QueueFile>> {
        let name = CString::new("queue").ok()?;
        let array = unsafe { ffi::binkd_perl_get_av(self.perl, name.as_ptr(), 0) };
        if array.is_null() {
            return None;
        }
        let last = unsafe { ffi::binkd_perl_av_len(self.perl, array) };
        let mut queue = Vec::new();
        let mut index = 0;
        while index <= last {
            let hash = unsafe { ffi::binkd_perl_av_fetch_hash(self.perl, array, index) };
            index += 1;
            if hash.is_null() {
                continue;
            }
            let Some(file) = self.hash_string(hash, "file") else {
                continue;
            };
            if file.is_empty() {
                continue;
            }
            let size = self.hash_int(hash, "size").unwrap_or(0).max(0) as u64;
            let time = self.hash_int(hash, "time").unwrap_or(0);
            let sent = self.hash_int(hash, "sent").unwrap_or(0) as i32;
            let flvr = self.hash_char(hash, "flvr", 'f');
            let action = self.hash_char(hash, "action", '\0');
            let type_ = self.hash_char(hash, "type", '\0');
            let addr = self
                .hash_string(hash, "addr")
                .and_then(|text| {
                    let mut fa = FtnAddr::parse(&text, &self.domains)?;
                    fa.expand(&self.aka, &self.domains);
                    Some(fa)
                })
                .unwrap_or_else(|| fallback.clone());
            queue.push(QueueFile {
                file,
                size,
                time,
                sent,
                flvr,
                action,
                type_,
                addr,
            });
        }
        Some(queue)
    }

    /// Returns a string of a queue hash, the port of the `hv_fetch()` and
    /// `SvPV()` reads of `refresh_queue()`.
    fn hash_string(&self, hash: *mut crate::bridge::Value, key: &str) -> Option<String> {
        let key = CString::new(key).ok()?;
        let mut defined = 0;
        let mut len = 0;
        let text = unsafe {
            ffi::binkd_perl_hv_fetch_string(self.perl, hash, key.as_ptr(), &mut defined, &mut len)
        };
        if text.is_null() || defined == 0 {
            if !text.is_null() {
                unsafe { ffi::binkd_perl_free_string(text) };
            }
            return None;
        }
        let value = unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned();
        unsafe { ffi::binkd_perl_free_string(text) };
        Some(value)
    }

    /// Returns an integer of a queue hash, the port of the `hv_fetch()` and
    /// `SvIV()` reads of `refresh_queue()`.
    fn hash_int(&self, hash: *mut crate::bridge::Value, key: &str) -> Option<i64> {
        let key = CString::new(key).ok()?;
        let mut defined = 0;
        let value =
            unsafe { ffi::binkd_perl_hv_fetch_int(self.perl, hash, key.as_ptr(), &mut defined) };
        if defined == 0 {
            None
        } else {
            Some(value)
        }
    }

    /// Returns the first character of a queue hash, the port of the `s[0]`
    /// reads of `refresh_queue()`.
    fn hash_char(&self, hash: *mut crate::bridge::Value, key: &str, default: char) -> char {
        match self.hash_string(hash, key) {
            // The C code reads the first byte of a defined value, so an empty
            // value answers the NUL character.
            Some(text) => text.chars().next().unwrap_or('\0'),
            None => default,
        }
    }

    /// Sets the hash `%opt` of the session, the port of the `%opt` block of
    /// `setup_session()`.
    fn set_opt(&self, opt: &Opt) {
        let Ok(name) = CString::new("opt") else {
            return;
        };
        unsafe {
            let hash = ffi::binkd_perl_get_hv(self.perl, name.as_ptr(), 1);
            if hash.is_null() {
                return;
            }
            ffi::binkd_perl_hv_clear(self.perl, hash);
            for (key, value) in [
                ("ND", opt.nd),
                ("NR", opt.nr),
                ("MD", opt.md),
                ("crypt", opt.crypt),
                ("GZ", opt.gz),
            ] {
                if let Ok(key) = CString::new(key) {
                    ffi::binkd_perl_hv_store_int(self.perl, hash, key.as_ptr(), value as i64);
                }
            }
        }
    }

    /// Reports whether the script defines `on_handshake()`, the port of the
    /// `PERL_ON_HANDSHAKE` bit of the C code.
    pub fn has_on_handshake(&self) -> bool {
        self.defined(ON_HANDSHAKE)
    }

    /// Calls `on_start()`, the port of `perl_on_start()`.
    pub fn on_start(&self) {
        if self.defined(ON_START) {
            self.call_void(ON_START);
        }
    }

    /// Calls `on_exit()`, the port of `perl_on_exit()`.
    pub fn on_exit(&self) {
        if self.defined(ON_EXIT) {
            self.call_void(ON_EXIT);
        }
    }

    /// Calls `config_loaded()`, the port of `perl_config_loaded()`.
    pub fn config_loaded(&self) {
        if self.defined(CONFIG_LOADED) {
            self.call_void(CONFIG_LOADED);
        }
    }

    /// Calls `need_reload()`, the port of `perl_need_reload()`.
    ///
    /// The answer is 1 to force a reload, 2 to suppress it, and 0 to leave the
    /// value of the check.
    pub fn need_reload(&self, conflist: &[String], need_reload: bool) -> i32 {
        if !self.defined(NEED_RELOAD) {
            return 0;
        }
        self.set_array("conflist", conflist);
        self.set_int("_", if need_reload { 1 } else { 0 });
        self.call_int(NEED_RELOAD).unwrap_or(0) as i32
    }

    /// Calls `on_call()`, the port of `perl_on_call()`.
    pub fn on_call(&self, call: &mut Call) -> bool {
        if !self.defined(ON_CALL) {
            return true;
        }
        self.set_int("traf_mail", call.traffic_mail as i64);
        self.set_int("traf_file", call.traffic_file as i64);
        self.set_str("addr", &call.addr.format(false));
        self.set_str("hosts", &call.hosts);
        self.set_str("proxy", &call.proxy);
        self.set_str("socks", &call.socks);
        let Some(rc) = self.call_int(ON_CALL) else {
            return true;
        };
        if rc == 0 {
            return false;
        }
        if let Some(hosts) = self.get_str("hosts") {
            call.hosts = hosts;
        }
        if let Some(proxy) = self.get_str("proxy") {
            call.proxy = proxy;
        }
        if let Some(socks) = self.get_str("socks") {
            call.socks = socks;
        }
        true
    }

    /// Calls `on_error()`, the port of `perl_on_error()`.
    pub fn on_error(&self, error: &CallError) -> bool {
        if !self.defined(ON_ERROR) {
            return true;
        }
        self.set_str("addr", &error.addr.format(false));
        self.set_str("error", &error.error);
        self.set_int("where", error.location as i64);
        self.call_int(ON_ERROR).unwrap_or(1) != 0
    }

    /// Calls `on_handshake()`, the port of `perl_on_handshake()`.
    ///
    /// The answer is the abort text of the hook, or `None` to continue. A
    /// successful call may replace `@me` and `$passwd` in `vars`.
    pub fn on_handshake(&self, vars: &mut Handshake) -> Option<String> {
        if !self.defined(ON_HANDSHAKE) {
            return None;
        }
        self.set_session(&vars.session);
        self.set_array("he", &addresses(&vars.he));
        self.undef_array("me");
        self.set_int("traf_mail", vars.traffic_mail as i64);
        self.set_int("traf_file", vars.traffic_file as i64);
        match self.call_string(ON_HANDSHAKE) {
            Ok(Some(response)) => return Some(response),
            Err(()) => return None,
            Ok(None) => {}
        }
        if let Some(values) = self.get_array("me") {
            let mut me = Vec::new();
            for text in values {
                if let Some(mut fa) = FtnAddr::parse(&text, &self.domains) {
                    fa.expand(&self.aka, &self.domains);
                    me.push(fa);
                }
            }
            vars.me = Some(me);
        }
        if let Some(password) = self.get_str("passwd") {
            vars.password = Some(password);
        }
        None
    }

    /// Calls `after_handshake()`, the port of `perl_after_handshake()`.
    pub fn after_handshake(&self, vars: &SessionVars) -> Option<String> {
        if !self.defined(AFTER_HANDSHAKE) {
            return None;
        }
        self.set_session(vars);
        match self.call_string(AFTER_HANDSHAKE) {
            Ok(Some(response)) => Some(response),
            _ => None,
        }
    }

    /// Calls `after_session()`, the port of `perl_after_session()`.
    pub fn after_session(&self, vars: &SessionVars, ok: bool) {
        if !self.defined(AFTER_SESSION) {
            return;
        }
        self.set_session(vars);
        self.set_int("rc", if ok { 1 } else { 0 });
        self.call_void(AFTER_SESSION);
    }

    /// Calls `before_recv()`, the port of `perl_before_recv()`.
    pub fn before_recv(&self, vars: &SessionVars, file: &File, offset: u64) -> i32 {
        if !self.defined(BEFORE_RECV) {
            return 0;
        }
        self.set_session(vars);
        self.set_str("name", &file.name);
        self.set_int("size", file.size as i64);
        self.set_int("time", file.time);
        self.set_int("offs", offset as i64);
        self.call_int(BEFORE_RECV).unwrap_or(0) as i32
    }

    /// Calls `after_recv()`, the port of `perl_after_recv()`.
    ///
    /// A result of 1 with a non-empty `$file` updates `file.file` and answers 0,
    /// the port of the C code.
    pub fn after_recv(&self, vars: &SessionVars, file: &mut Received) -> i32 {
        if !self.defined(AFTER_RECV) {
            return 0;
        }
        self.set_session(vars);
        self.set_str("name", &file.name);
        self.set_int("size", file.size as i64);
        self.set_int("time", file.time);
        self.set_str("tmpfile", &file.tmp);
        self.set_str("file", &file.file);
        let rc = self.call_int(AFTER_RECV).unwrap_or(0) as i32;
        if rc == 1 {
            match self.get_str("file") {
                Some(name) if !name.is_empty() => {
                    file.file = name;
                    0
                }
                _ => 2,
            }
        } else {
            rc
        }
    }

    /// Calls `before_send()`, the port of `perl_before_send()`.
    pub fn before_send(&self, vars: &SessionVars, file: &mut File) -> i32 {
        if !self.defined(BEFORE_SEND) {
            return 0;
        }
        self.set_session(vars);
        self.set_str("file", &file.path);
        self.set_str("name", &file.name);
        self.set_int("size", file.size as i64);
        self.set_int("time", file.time);
        let rc = self.call_int(BEFORE_SEND).unwrap_or(0) as i32;
        if rc == 0 {
            if let Some(name) = self.get_str("name") {
                file.name = name;
            }
        }
        rc
    }

    /// Calls `after_sent()`, the port of `perl_after_sent()`.
    pub fn after_sent(&self, vars: &SessionVars, file: &mut Sent) -> i32 {
        if !self.defined(AFTER_SENT) {
            return 0;
        }
        self.set_session(vars);
        self.set_str("file", &file.path);
        self.set_str("name", &file.name);
        self.set_int("size", file.size as i64);
        self.set_int("time", file.time);
        self.set_int("start", file.start);
        self.set_str("action", &file.action.to_string());
        let rc = self.call_int(AFTER_SENT).unwrap_or(0) as i32;
        if rc != 0 {
            if let Some(action) = self.get_str("action") {
                file.action = action.chars().next().unwrap_or('\0');
            }
        }
        rc
    }

    /// Calls `on_send()`, the port of `perl_on_send()`.
    pub fn on_send(&self, vars: &SessionVars, kind: u8, s1: &str, s2: &str) {
        if !self.defined(ON_SEND) {
            return;
        }
        self.set_session(vars);
        self.set_int("type", kind as i64);
        self.set_str("s1", s1);
        self.set_str("s2", s2);
        self.call_void(ON_SEND);
    }

    /// Calls `on_recv()`, the port of `perl_on_recv()`.
    pub fn on_recv(&self, vars: &SessionVars, data: &[u8]) {
        if !self.defined(ON_RECV) {
            return;
        }
        self.set_session(vars);
        self.set_bytes("s", data);
        self.call_void(ON_RECV);
    }

    /// Calls `setup_rlimit()`, the port of `perl_setup_rlimit()`.
    pub fn setup_rlimit(&self, vars: &SessionVars, file: &str, limit: &mut i64) -> i32 {
        if !self.defined(SETUP_RLIMIT) {
            return 1;
        }
        self.set_session(vars);
        self.set_str("file", file);
        self.set_int("rlimit", *limit);
        let rc = self.call_int(SETUP_RLIMIT).unwrap_or(1) as i32;
        if rc != 0 {
            if let Some(value) = self.get_int("rlimit") {
                *limit = value;
            }
        }
        rc
    }
}

/// Returns the strings of a list of addresses, the port of
/// `ftnaddress_to_str()`.
fn addresses(addrs: &[FtnAddr]) -> Vec<String> {
    addrs.iter().map(|a| a.format(false)).collect()
}

/// Returns a one-character string of an entry field, the port of the
/// `buf[0]` of `setup_queue()`.
fn char_string(value: char) -> CString {
    let text = if value == '\0' {
        String::new()
    } else {
        value.to_string()
    };
    CString::new(text).unwrap_or_default()
}

impl Drop for Hooks {
    fn drop(&mut self) {
        // The helpers of a `DESTROY` or a warning see this value, the port of
        // the `SET_CONFIG` of `perl_done_clone()`.
        let _context = ContextGuard::enter(self);
        if self.is_clone {
            unsafe { ffi::binkd_perl_free_clone(self.perl) };
        } else {
            unsafe { ffi::binkd_perl_free(self.perl) };
        }
    }
}

/// The hook of the log of a script, the port of `perl_on_log()` as a value of
/// the log.
///
/// The Perl interpreter of one value belongs to one thread at a time, so the
/// adapter holds a mutex. The log of the port is shared by the sessions, so the
/// adapter makes the calls of the hook sequential. The recursion guard of the
/// value is thread local, so a hook of one thread does not suppress the hook of
/// another thread.
pub struct LogHook {
    hooks: Arc<Mutex<Hooks>>,
}

impl LogHook {
    /// Wraps the hooks of a script for the log.
    pub fn new(hooks: Hooks) -> LogHook {
        LogHook::from_shared(Arc::new(Mutex::new(hooks)))
    }

    /// Wraps the hooks that the sessions share for the log.
    pub fn from_shared(hooks: Arc<Mutex<Hooks>>) -> LogHook {
        LogHook { hooks }
    }
}

impl Hook for LogHook {
    fn on_log(&self, message: &mut Vec<u8>, level: &mut i32) -> bool {
        match self.hooks.lock() {
            Ok(hooks) => hooks.on_log(message, level),
            // A poisoned lock holds no safe interpreter, so the hook keeps the
            // message.
            Err(_) => true,
        }
    }
}

/// Returns the C string of a value, or an empty string at the first NUL.
fn cstr(value: &str) -> CString {
    let bytes: Vec<u8> = value.bytes().take_while(|byte| *byte != 0).collect();
    CString::new(bytes).unwrap_or_default()
}

/// Returns the value of `enum inbcasetype` of `btypes.h`.
fn inb_case(case: InbCase) -> i64 {
    match case {
        InbCase::Save => 0,
        InbCase::Upper => 1,
        InbCase::Lower => 2,
        InbCase::Mixed => 3,
    }
}

/// Returns the one-character string of a flavour, the port of the `buf` of
/// `add_node_to_hv()`.
fn flavour(value: u8) -> String {
    if value == 0 {
        String::new()
    } else {
        (value as char).to_string()
    }
}
