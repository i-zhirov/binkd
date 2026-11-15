//! The config parser, the port of `readcfg.c`.
//!
//! The C code keeps the config under construction in the `work_config` global
//! and the parser state in the `current_path` and `current_line` globals. The
//! port keeps both in the parser value.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::FtnDomain;
use binkd_ftn::node::{NodeSpec, AF_UNSPEC, HC_USE_OLD, MD_USE_OLD, USE_OLD};
use binkd_ftn::text::{last_slash, InbCase};
use binkd_ftn::token::{getwordx2, GWX_HASH, GWX_SUBST};
use binkd_spool::config::{addrtype, RenameStyle};

use crate::keywords::{self, Features};
use crate::model::{
    AkaChain, Config, ConfigFile, DontSendEmpty, EvtFlag, ListenChain, PerlVar, RateChain,
    SkipChain, ZRule,
};
use crate::port::{self, PortResolver};

/// The maximum number of the nested `include` directives.
pub const MAX_INCLUDE_LEVEL: usize = 8;

/// The maximum number of the words on a line.
const MAX_WORDS_ON_LINE: usize = 64;

/// The maximum length of a path.
pub const MAX_PATH_LEN: usize = 1024;

/// The `MAX_DOMAIN` define of `Config.h`.
const MAX_DOMAIN: usize = 32;

/// The `MAXSYSTEMNAME`, `MAXSYSOPNAME`, `MAXLOCATIONNAME`, and `MAXNODEINFO`
/// defines of `Config.h`.
const MAX_SYSNAME: usize = 120;
const MAX_SYSOP: usize = 120;
const MAX_LOCATION: usize = 120;
const MAX_NODEINFO: usize = 120;

/// The `DONT_CHECK` value of the C table.
const DONT_CHECK: i64 = 0x7fffffff;

/// The address families of the `-4` and the `-6` options.
///
/// The C code uses the values of the system; the port states the values of
/// Linux, and the network layer maps them.
pub const AF_INET: i32 = 2;
pub const AF_INET6: i32 = 10;

/// A config error.
///
/// The port of `ConfigError()` of `readcfg.c`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// The path of the file.
    pub path: String,
    /// The line number.
    pub line: usize,
    /// The message.
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: line {}: error in configuration files: {}",
            self.path, self.line, self.message
        )
    }
}

impl std::error::Error for Error {}

/// The config parser.
pub struct Parser<'a> {
    config: Config,
    path: String,
    line: usize,
    level: usize,
    ports: PortResolver<'a>,
    quiet: bool,
    verbose: i32,
    now: i64,
    /// The build features: the keywords that the table holds.
    features: Features,
    /// The directory of the relative paths of the config.
    base_dir: Option<std::path::PathBuf>,
}

impl Default for Parser<'static> {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser<'static> {
    /// Creates a parser with the default values and the default port resolver.
    pub fn new() -> Parser<'static> {
        Parser::with_ports(&port::resolve)
    }
}

impl<'a> Parser<'a> {
    /// Creates a parser with a resolver of the port names.
    pub fn with_ports(ports: PortResolver<'a>) -> Parser<'a> {
        Parser {
            config: Config::new(),
            path: String::new(),
            line: 0,
            level: 0,
            ports,
            quiet: false,
            verbose: 0,
            now: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            features: Features::unix_default(),
            base_dir: None,
        }
    }

    /// Sets the build features: the keywords that the table holds.
    pub fn set_features(&mut self, features: Features) {
        self.features = features;
    }

    /// Returns the build features.
    pub fn features(&self) -> &Features {
        &self.features
    }

    /// Sets the directory of the relative paths of the config.
    ///
    /// The C code resolves a relative path against the working directory. The
    /// port resolves it against this directory.
    pub fn set_base_dir(&mut self, dir: &Path) {
        self.base_dir = Some(dir.to_path_buf());
    }

    /// Sets the quiet mode of the command line.
    pub fn set_quiet(&mut self, quiet: bool) {
        self.quiet = quiet;
    }

    /// Sets the verbose level of the command line.
    pub fn set_verbose(&mut self, verbose: i32) {
        self.verbose = verbose;
    }

    /// Sets the time of the parse. The node table stores it.
    pub fn set_now(&mut self, now: i64) {
        self.now = now;
    }

    /// Returns the config under construction.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Parses a config file.
    ///
    /// The port of `readcfg0()`.
    pub fn parse_file(&mut self, path: &str) -> Result<(), Error> {
        // The C code resolves a relative path against the working directory. The
        // port resolves it against the base directory of the parser.
        let full = match &self.base_dir {
            Some(dir) => dir.join(path),
            None => PathBuf::from(path),
        };
        let text = fs::read_to_string(&full).map_err(|e| Error {
            path: path.to_string(),
            line: 0,
            message: format!("cannot open {path}: {e}"),
        })?;

        let old_path = std::mem::replace(&mut self.path, path.to_string());
        let old_line = std::mem::replace(&mut self.line, 0);

        let mtime = fs::metadata(&full)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.config.config_list.push(ConfigFile {
            path: path.to_string(),
            mtime,
        });

        let mut result = Ok(());
        for (i, line) in text.lines().enumerate() {
            self.line = i + 1;
            if let Err(e) = self.parse_line(line) {
                result = Err(e);
                break;
            }
        }

        self.path = old_path;
        self.line = old_line;
        result
    }

    /// Parses and checks a config file.
    ///
    /// The port of `readcfg()`: the file, the required values, the command
    /// line overrides, the default listen entry, the passwords file, and the
    /// directory checks.
    pub fn parse(&mut self, path: &str) -> Result<Config, Error> {
        self.parse_file(path)?;
        self.finish()?;
        Ok(self.config.clone())
    }

    /// Checks the parsed config.
    ///
    /// The port of the checks of `readcfg()`.
    pub fn finish(&mut self) -> Result<(), Error> {
        let sysname = self.config.sysname.clone();
        let sysop = self.config.sysop.clone();
        let location = self.config.location.clone();
        let nodeinfo = self.config.nodeinfo.clone();
        self.is_defined(&sysname, "sysname")?;
        self.is_defined(&sysop, "sysop")?;
        self.is_defined(&location, "location")?;
        self.is_defined(&nodeinfo, "nodeinfo")?;

        if self.config.spool.aka.is_empty() {
            return Err(self.error("your address should be defined".to_string()));
        }
        if self.config.spool.domains.domains().is_empty() {
            return Err(self.error("at least one domain should be defined".to_string()));
        }

        // The command line overrides.
        if self.quiet {
            self.config.percents = false;
            self.config.conlog = 0;
            self.config.printq = false;
        }
        match self.verbose {
            0 => {}
            1 => {
                self.config.percents = true;
                self.config.printq = true;
                self.config.loglevel = 4;
                self.config.conlog = 4;
            }
            _ => {
                self.config.percents = true;
                self.config.printq = true;
                self.config.loglevel = 6;
                self.config.conlog = 6;
            }
        }

        // The default listen entry.
        if self.config.listen.is_empty() {
            self.config.listen.push(ListenChain::default());
        }
        let iport = self.config.iport.clone();
        for entry in &mut self.config.listen {
            if entry.port.is_empty() {
                entry.port = iport.clone();
            }
        }

        if self.config.spool.inbound_nonsecure.is_empty() {
            self.config.spool.inbound_nonsecure = self.config.inbound.clone();
        }

        if !self.config.passwords.is_empty() {
            let path = self.config.passwords.clone();
            self.read_passwords(&path)?;
        }

        self.check_directories()?;
        Ok(())
    }

    /// Parses one line of a config file.
    fn parse_line(&mut self, line: &str) -> Result<(), Error> {
        let mut words: Vec<String> = Vec::new();
        for n in 1..=MAX_WORDS_ON_LINE {
            match getwordx2(
                Some(line.as_bytes()),
                n as i32,
                GWX_SUBST | GWX_HASH,
                b" \t\n\r",
                b" \t\n\r",
            ) {
                Some(word) => words.push(String::from_utf8_lossy(&word).into_owned()),
                None => break,
            }
        }
        if words.is_empty() {
            return Ok(());
        }

        let name = words[0].clone();
        if !keywords::is_keyword(&name, &self.features) {
            return Err(self.error(format!("{name}: unknown keyword")));
        }
        self.apply(&name, &words[1..])
    }

    /// Dispatches one keyword.
    fn apply(&mut self, name: &str, args: &[String]) -> Result<(), Error> {
        match name.to_ascii_lowercase().as_str() {
            "include" => self.include(args),
            "passwords" => {
                self.config.passwords = self.string_arg(args, None, MAX_PATH_LEN)?;
                Ok(())
            }
            "log" => {
                self.config.logpath = self.string_arg(args, None, MAX_PATH_LEN)?;
                Ok(())
            }
            "loglevel" => {
                self.config.loglevel = self.int_arg(args, DONT_CHECK, DONT_CHECK)?;
                Ok(())
            }
            "conlog" => {
                self.config.conlog = self.int_arg(args, DONT_CHECK, DONT_CHECK)?;
                Ok(())
            }
            "binlog" => {
                self.config.binlogpath = self.string_arg(args, None, MAX_PATH_LEN)?;
                Ok(())
            }
            "fdinhist" => {
                self.config.fdinhist = self.string_arg(args, None, MAX_PATH_LEN)?;
                Ok(())
            }
            "fdouthist" => {
                self.config.fdouthist = self.string_arg(args, None, MAX_PATH_LEN)?;
                Ok(())
            }
            "tzoff" => {
                self.config.tzoff = self.time_arg(args, -12 * 60 * 60, 12 * 60 * 60)?;
                Ok(())
            }
            "domain" => self.domain(args),
            "address" => self.aka_list(args),
            "sysname" => {
                self.config.sysname = self.string_arg(args, None, MAX_SYSNAME)?;
                Ok(())
            }
            "bindaddr" => {
                self.config.bindaddr = self.string_arg(args, None, 42)?;
                Ok(())
            }
            "listen" => self.listen(args),
            "sysop" => {
                self.config.sysop = self.string_arg(args, None, MAX_SYSOP)?;
                Ok(())
            }
            "location" => {
                self.config.location = self.string_arg(args, None, MAX_LOCATION)?;
                Ok(())
            }
            "nodeinfo" => {
                self.config.nodeinfo = self.string_arg(args, None, MAX_NODEINFO)?;
                Ok(())
            }
            "iport" => {
                self.config.iport = self.port_arg(args)?;
                Ok(())
            }
            "oport" => {
                self.config.oport = self.port_arg(args)?;
                Ok(())
            }
            "rescan-delay" => {
                self.config.rescan_delay = self.time_arg(args, 1, DONT_CHECK)?;
                Ok(())
            }
            "call-delay" => {
                self.config.call_delay = self.time_arg(args, 1, DONT_CHECK)?;
                Ok(())
            }
            "timeout" => {
                self.config.nettimeout = self.time_arg(args, 1, DONT_CHECK)?;
                Ok(())
            }
            "oblksize" => {
                self.config.oblksize = self.int_arg(args, 128, 0x7fff)?;
                Ok(())
            }
            "maxservers" => {
                self.config.max_servers = self.int_arg(args, 0, DONT_CHECK)?;
                Ok(())
            }
            "maxclients" => {
                self.config.max_clients = self.int_arg(args, 0, DONT_CHECK)?;
                Ok(())
            }
            "inbound" => {
                self.config.inbound = self.string_arg(args, Some('d'), MAX_PATH_LEN)?;
                Ok(())
            }
            "inbound-nonsecure" => {
                self.config.spool.inbound_nonsecure =
                    self.string_arg(args, Some('d'), MAX_PATH_LEN)?;
                Ok(())
            }
            "temp-inbound" => {
                self.config.spool.temp_inbound = self.string_arg(args, Some('d'), MAX_PATH_LEN)?;
                Ok(())
            }
            "node" => self.node(args, false),
            "defnode" => self.node(args, true),
            "kill-dup-partial-files" => {
                self.config.spool.kill_dup_partial_files = self.bool_arg(args)?;
                Ok(())
            }
            "kill-old-partial-files" => {
                self.config.spool.kill_old_partial_files =
                    self.time_arg(args, 1, DONT_CHECK)? as i64;
                Ok(())
            }
            "kill-old-bsy" => {
                self.config.spool.kill_old_bsy = self.time_arg(args, 1, DONT_CHECK)? as i64;
                Ok(())
            }
            "percents" => {
                self.config.percents = self.bool_arg(args)?;
                Ok(())
            }
            "minfree" => {
                self.config.spool.minfree = self.int_arg(args, 0, DONT_CHECK)? as i64;
                Ok(())
            }
            "minfree-nonsecure" => {
                self.config.spool.minfree_nonsecure = self.int_arg(args, 0, DONT_CHECK)? as i64;
                Ok(())
            }
            "flag" => self.flag_exec(args, 'f'),
            "exec" => self.flag_exec(args, 'e'),
            "printq" => {
                self.config.printq = self.bool_arg(args)?;
                Ok(())
            }
            "try" => {
                self.config.spool.tries = self.int_arg(args, 0, 0xffff)? as u32;
                Ok(())
            }
            "hold" => {
                self.config.spool.hold = self.time_arg(args, 0, DONT_CHECK)? as i64;
                Ok(())
            }
            "hold-skipped" => {
                self.config.hold_skipped = self.time_arg(args, 0, DONT_CHECK)?;
                Ok(())
            }
            "backresolv" => {
                self.config.backresolv = self.bool_arg(args)?;
                Ok(())
            }
            "pid-file" => {
                self.config.pid_file = self.string_arg(args, Some('f'), MAX_PATH_LEN)?;
                Ok(())
            }
            "remove-try-files" => {
                self.config.spool.remove_try_files = self.bool_arg(args)?;
                Ok(())
            }
            "proxy" => {
                self.config.proxy = self.string_arg(args, None, MAX_PATH_LEN)?;
                Ok(())
            }
            "socks" => {
                self.config.socks = self.string_arg(args, None, MAX_PATH_LEN)?;
                Ok(())
            }
            "syslog" => {
                self.config.syslog_facility = self.syslog_arg(args)?;
                Ok(())
            }
            "ftrans" => self.rfrule(args),
            "send-if-pwd" => {
                self.config.spool.send_if_pwd = self.bool_arg(args)?;
                Ok(())
            }
            "root-domain" => {
                self.config.root_domain = self.string_arg(args, None, 255)?;
                Ok(())
            }
            "prescan" => {
                self.config.prescan = self.bool_arg(args)?;
                Ok(())
            }
            "connect-timeout" => {
                self.config.connect_timeout = self.time_arg(args, 0, DONT_CHECK)?;
                Ok(())
            }
            "filebox" => {
                self.config.spool.tfilebox = self.string_arg(args, Some('d'), MAX_PATH_LEN)?;
                Ok(())
            }
            "brakebox" => {
                self.config.spool.bfilebox = self.string_arg(args, Some('d'), MAX_PATH_LEN)?;
                Ok(())
            }
            "deletebox" => {
                self.config.spool.deleteablebox = self.bool_arg(args)?;
                Ok(())
            }
            "skip" => self.skip(args, false),
            "skipmask" => self.skip(args, true),
            "inboundcase" => {
                self.config.spool.inboundcase = self.inboundcase_arg(args)?;
                Ok(())
            }
            "deletedirs" => {
                self.config.spool.deletedirs = self.bool_arg(args)?;
                Ok(())
            }
            "overwrite" => {
                self.config.spool.overwrite = self.mask_arg(args)?;
                Ok(())
            }
            "dont-send-empty" => {
                self.config.dontsendempty = self.dontsendempty_arg(args)?;
                Ok(())
            }
            "rename-style" => {
                self.config.spool.renamestyle = self.renamestyle_arg(args)?;
                Ok(())
            }
            "share" => self.share(args),
            "check-pkthdr" => self.check_pkthdr(args),
            "aso" => {
                // The Amiga style outbound is not ported; the option is read.
                self.bool_arg(args)?;
                Ok(())
            }
            "perl-hooks" => {
                self.config.perl_script = self.string_arg(args, Some('f'), MAX_PATH_LEN)?;
                Ok(())
            }
            "perl-dll" => {
                self.config.perl_dll = self.string_arg(args, Some('f'), MAX_PATH_LEN)?;
                Ok(())
            }
            "perl-strict" => {
                self.config.perl_strict = self.bool_arg(args)?;
                Ok(())
            }
            "perl-var" => self.perlvar(args),
            "nolog" => {
                self.config.nolog = self.mask_arg(args)?;
                Ok(())
            }
            "hide-aka" => self.akachain(args, 1),
            "present-aka" => self.akachain(args, 2),
            "zlevel" => {
                self.config.zlevel = self.int_arg(args, 0, 9)?;
                Ok(())
            }
            "zminsize" => {
                self.config.zminsize = self.int_arg(args, 0, DONT_CHECK)?;
                Ok(())
            }
            "zallow" => self.zrule(args, true),
            "zdeny" => self.zrule(args, false),
            "zlib-dll" => {
                self.config.zlib_dll = self.string_arg(args, Some('f'), MAX_PATH_LEN)?;
                Ok(())
            }
            "bzlib2-dll" => {
                self.config.bzlib2_dll = self.string_arg(args, Some('f'), MAX_PATH_LEN)?;
                Ok(())
            }
            "limit-rate" => self.rate(args),
            _ => Err(self.error(format!("{name}: unknown keyword"))),
        }
    }

    /// Returns an error at the current position.
    fn error(&self, message: String) -> Error {
        Error {
            path: self.path.clone(),
            line: self.line,
            message,
        }
    }

    /// The port of `isArgCount()`.
    fn argc(&self, expected: usize, got: usize) -> Result<(), Error> {
        if expected != got {
            return Err(self.error(format!("{expected} argument(s) required")));
        }
        Ok(())
    }

    /// The port of `isDefined()`.
    fn is_defined(&self, value: &str, name: &str) -> Result<(), Error> {
        if value.is_empty() {
            return Err(self.error(format!("'{name}' must be defined")));
        }
        Ok(())
    }

    /// The port of `SyntaxError()`.
    fn syntax(&self, name: &str) -> Error {
        self.error(format!("the syntax is incorrect for '{name}'"))
    }

    /// The port of `ConfigNeedNumber()`.
    fn need_number(&self, word: &str) -> Error {
        self.error(format!("{word}: expecting a number"))
    }

    /// The port of `ConfigNeedTime()`.
    fn need_time(&self, word: &str) -> Error {
        self.error(format!("{word}: expecting a time interval"))
    }

    /// The port of `Config_ParseAddress()`.
    fn parse_address(&self, s: &str) -> Result<FtnAddr, Error> {
        FtnAddr::parse(s, &self.config.spool.domains)
            .ok_or_else(|| self.error(format!("{s}: the address cannot be parsed")))
    }

    /// The port of `check_dir_path()`.
    fn check_dir_path(s: &str) -> String {
        s.trim_end_matches(['/', '\\']).to_string()
    }

    /// The port of the `read_string()` callback.
    ///
    /// `kind` is `None` for a plain string, `Some('d')` for a directory,
    /// and `Some('f')` for a file name.
    fn string_arg(
        &self,
        words: &[String],
        kind: Option<char>,
        max: usize,
    ) -> Result<String, Error> {
        self.argc(1, words.len())?;
        let mut value: String = words[0].chars().take(max).collect();
        if let Some(kind) = kind {
            value = Self::check_dir_path(&value);
            if kind == 'd' {
                let path = match &self.base_dir {
                    Some(dir) => dir.join(&value),
                    None => PathBuf::from(&value),
                };
                if !path.is_dir() {
                    return Err(self.error(format!("{value}: incorrect directory")));
                }
            }
        }
        Ok(value)
    }

    /// The port of the `read_int()` callback.
    fn int_arg(&self, words: &[String], min: i64, max: i64) -> Result<i32, Error> {
        self.argc(1, words.len())?;
        let word = &words[0];
        for (i, c) in word.chars().enumerate() {
            if c.is_ascii_digit() || (i == 0 && c == '-') {
                continue;
            }
            return Err(self.need_number(word));
        }
        let value: i64 = word.parse().unwrap_or(0);
        if (min != DONT_CHECK && value < min) || (max != DONT_CHECK && value > max) {
            return Err(self.error(format!("{value}: incorrect value")));
        }
        Ok(value as i32)
    }

    /// The port of the `read_time()` callback.
    fn time_arg(&self, words: &[String], min: i64, max: i64) -> Result<i32, Error> {
        self.argc(1, words.len())?;
        let word = &words[0];
        let mut target: i64 = 0;
        let mut curnum: i64 = -1;
        let mut chars = word.chars().peekable();
        if chars.peek() == Some(&'-') {
            chars.next();
        }
        for c in chars {
            if c.is_ascii_digit() {
                if curnum == -1 {
                    curnum = 0;
                } else {
                    curnum *= 10;
                }
                curnum += c as i64 - '0' as i64;
                continue;
            }
            if curnum == -1 {
                return Err(self.need_time(word));
            }
            match c.to_ascii_lowercase() {
                'w' => curnum *= 7 * 24 * 60 * 60,
                'd' => curnum *= 24 * 60 * 60,
                'h' => curnum *= 60 * 60,
                'm' => curnum *= 60,
                's' => {}
                _ => return Err(self.need_time(word)),
            }
            target += curnum;
            curnum = -1;
        }
        if curnum != -1 {
            target += curnum;
        }
        if word.starts_with('-') {
            target = -target;
        }
        if (min != DONT_CHECK && target < min) || (max != DONT_CHECK && target > max) {
            return Err(self.error(format!("{word}: incorrect value")));
        }
        Ok(target as i32)
    }

    /// The port of the `read_bool()` callback.
    fn bool_arg(&self, words: &[String]) -> Result<bool, Error> {
        if !words.is_empty() {
            return Err(self.syntax(&words[0]));
        }
        Ok(true)
    }

    /// The port of the `read_port()` callback.
    fn port_arg(&self, words: &[String]) -> Result<String, Error> {
        self.argc(1, words.len())?;
        (self.ports)(&words[0]).ok_or_else(|| self.error(format!("{}: bad port number", words[0])))
    }

    /// The port of the `read_inboundcase()` callback.
    fn inboundcase_arg(&self, words: &[String]) -> Result<InbCase, Error> {
        self.argc(1, words.len())?;
        match words[0].to_ascii_lowercase().as_str() {
            "save" => Ok(InbCase::Save),
            "upper" => Ok(InbCase::Upper),
            "lower" => Ok(InbCase::Lower),
            "mixed" => Ok(InbCase::Mixed),
            _ => Err(self.syntax(&words[0])),
        }
    }

    /// The port of the `read_renamestyle()` callback.
    fn renamestyle_arg(&self, words: &[String]) -> Result<RenameStyle, Error> {
        self.argc(1, words.len())?;
        match words[0].to_ascii_lowercase().as_str() {
            "postfix" => Ok(RenameStyle::Postfix),
            "extenstion" => Ok(RenameStyle::Extension),
            _ => Err(self.syntax(&words[0])),
        }
    }

    /// The port of the `read_dontsendempty()` callback.
    fn dontsendempty_arg(&self, words: &[String]) -> Result<DontSendEmpty, Error> {
        self.argc(1, words.len())?;
        match words[0].to_ascii_lowercase().as_str() {
            "no" => Ok(DontSendEmpty::No),
            "arcmail" => Ok(DontSendEmpty::Arcmail),
            "yes" => Ok(DontSendEmpty::All),
            _ => Err(self.syntax(&words[0])),
        }
    }

    /// The port of the `read_syslog_facility()` callback.
    fn syslog_arg(&self, words: &[String]) -> Result<i32, Error> {
        self.argc(1, words.len())?;
        // The C code asks the facility table of the system.
        match words[0].to_ascii_lowercase().as_str() {
            "auth" | "authpriv" => Ok(32),
            "cron" => Ok(72),
            "daemon" => Ok(24),
            "kern" => Ok(0),
            "local0" => Ok(128),
            "local1" => Ok(136),
            "local2" => Ok(144),
            "local3" => Ok(152),
            "local4" => Ok(160),
            "local5" => Ok(168),
            "local6" => Ok(176),
            "local7" => Ok(184),
            "lpr" => Ok(48),
            "mail" => Ok(16),
            "news" => Ok(56),
            "syslog" => Ok(40),
            "user" => Ok(8),
            "uucp" => Ok(64),
            _ => Err(self.error(format!("{}: incorrect facility name", words[0]))),
        }
    }

    /// The port of the `include()` callback.
    fn include(&mut self, words: &[String]) -> Result<(), Error> {
        self.argc(1, words.len())?;
        if self.level == MAX_INCLUDE_LEVEL {
            return Err(self.error("too many nested include commands".to_string()));
        }
        self.level += 1;
        let result = self.parse_file(&words[0]);
        self.level -= 1;
        result
    }

    /// The port of the `read_domain_info()` callback.
    fn domain(&mut self, words: &[String]) -> Result<(), Error> {
        if words.len() != 3 && words.len() != 4 {
            return Err(self.error("3 or 4 arguments required".to_string()));
        }
        if self.config.spool.domains.get(&words[0]).is_some() {
            return Err(self.error(format!("{}: duplicate domain", words[0])));
        }

        let mut domain = FtnDomain {
            name: words[0].chars().take(MAX_DOMAIN).collect(),
            path: String::new(),
            dir: String::new(),
            z: [0, 0],
            idomain: None,
            alias4: None,
        };

        if words[1].eq_ignore_ascii_case("alias-for") {
            let index = self
                .config
                .spool
                .domains
                .index_of(&words[2])
                .ok_or_else(|| self.error(format!("{}: undefined domain", words[2])))?;
            self.argc(3, words.len())?;
            domain.alias4 = Some(index);
        } else {
            let zone = &words[2];
            let digits = zone.chars().take_while(|c| c.is_ascii_digit()).count();
            if digits != zone.len() || zone.parse::<i64>().unwrap_or(0) <= 0 {
                return Err(self.error(format!("{zone}: invalid zone")));
            }
            domain.z = [zone.parse().unwrap_or(0), 0];

            let path = Self::check_dir_path(&words[1]);
            let (dir, path) = match last_slash(path.as_bytes()) {
                Some(suffix) => {
                    // The C code takes `s + 1`: the name after the separator.
                    let dir = String::from_utf8_lossy(&suffix[1..]).into_owned();
                    let cut = path.len() - suffix.len();
                    (dir, path[..cut].to_string())
                }
                None => (path.clone(), ".".to_string()),
            };
            if dir.contains('.') {
                return Err(self
                    .error("there should be no extension for the base outbound name".to_string()));
            }
            domain.dir = dir;
            domain.path = path;
            if words.len() == 4 {
                domain.idomain = Some(words[3].clone());
            }
        }

        self.config.spool.domains.push(domain);
        Ok(())
    }

    /// The port of the `read_aka_list()` callback.
    fn aka_list(&mut self, words: &[String]) -> Result<(), Error> {
        if words.is_empty() {
            return Err(self.syntax("address"));
        }
        for word in words {
            let mut fa = self.parse_address(word)?;
            if fa.z == -1 || fa.net == -1 || fa.node == -1 || fa.p == -1 {
                return Err(self.error(format!("{word}: must be at least a 4D address")));
            }
            if fa.domain.is_empty() {
                if self.config.spool.domains.domains().is_empty() {
                    return Err(self.error("at least one domain must be defined first".to_string()));
                }
                fa.domain = self
                    .config
                    .spool
                    .domains
                    .matched(fa.z, &self.config.spool.aka);
            }
            if self.config.spool.aka.iter().any(|a| a.compare(&fa) == 0) {
                // The C code logs and ignores a duplicate address.
                continue;
            }
            self.config.spool.aka.push(fa);
        }
        Ok(())
    }

    /// The port of the `read_node_info()` callback.
    fn node(&mut self, words: &[String], defnode: bool) -> Result<(), Error> {
        let mut w: Vec<Option<String>> = vec![None; 6];
        let mut i = 0usize;
        let mut nr_flag = USE_OLD;
        let mut nd_flag = USE_OLD;
        let mut hc_flag = HC_USE_OLD;
        let mut md_flag = MD_USE_OLD;
        let mut np_flag = USE_OLD;
        let mut bw_send = binkd_ftn::node::BW_DEF;
        let mut bw_recv = binkd_ftn::node::BW_DEF;
        let mut restrict_ip = USE_OLD;
        let mut ip_afamily = USE_OLD;
        let mut pipe: Option<String> = None;

        if defnode {
            w[i] = Some("0:0/0.0@defnode".to_string());
            i += 1;
            self.config.spool.havedefnode = true;
        }

        let mut j = 0usize;
        while j < words.len() {
            let tmp = words[j].clone();
            if tmp.starts_with('-') && tmp.len() > 1 {
                match tmp.to_ascii_lowercase().as_str() {
                    "-md" => md_flag = 1,
                    "-nomd" => md_flag = -1,
                    "-nr" => nr_flag = 1,
                    "-nd" => {
                        nr_flag = 1;
                        nd_flag = 1;
                    }
                    "-ip" => restrict_ip = 1,
                    "-sip" => restrict_ip = 2,
                    "-hc" => hc_flag = 1,
                    "-nohc" => hc_flag = -1,
                    "-noproxy" => np_flag = 1,
                    // The C code holds the option with `BW_LIM` only.
                    "-bw" if self.features.bwlim => {
                        // The port of the `-bw` option: one value asks for both
                        // directions, and two values split at the first slash.
                        if j == words.len() - 1 {
                            return Err(self.error("`-bw' option requires parameter".to_string()));
                        }
                        j += 1;
                        let value = words[j].clone();
                        let (send, recv) = match value.split_once('/') {
                            Some((send, recv)) => (send.to_string(), recv.to_string()),
                            None => (value.clone(), value),
                        };
                        let mut err = None;
                        bw_send = Self::parse_rate(&send, &mut err);
                        if err.is_some() {
                            return Err(self.error("syntax error near `-bw'".to_string()));
                        }
                        bw_recv = Self::parse_rate(&recv, &mut err);
                        if err.is_some() {
                            return Err(self.error("syntax error near `-bw'".to_string()));
                        }
                    }
                    "-crypt" => {
                        // The C code logs that the option is obsolete.
                    }
                    "-4" => {
                        if ip_afamily <= AF_UNSPEC {
                            ip_afamily = AF_INET;
                        }
                    }
                    "-6" => {
                        if ip_afamily <= AF_UNSPEC {
                            ip_afamily = AF_INET6;
                        }
                    }
                    "-pipe" => {
                        if j == words.len() - 1 {
                            return Err(self.error("`-pipe' option requires parameter".to_string()));
                        }
                        j += 1;
                        pipe = Some(words[j].clone());
                    }
                    _ => {
                        return Err(self.error(format!("{tmp}: unknown option for `node' keyword")))
                    }
                }
            } else if i >= 6 {
                return Err(self.error("too many argumets for `node' keyword".to_string()));
            } else {
                w[i] = if tmp == "-" { None } else { Some(tmp) };
                i += 1;
            }
            j += 1;
        }

        let Some(address) = w[0].clone() else {
            return Err(self.error("missing node address".to_string()));
        };
        let mut fa = self.parse_address(&address)?;
        fa.expand(&self.config.spool.aka, &self.config.spool.domains);

        if w[2].as_deref() == Some("") {
            return Err(self.error("empty password".to_string()));
        }
        if let Some(flvr) = &w[3] {
            if !flvr
                .chars()
                .next()
                .is_some_and(|c| "IiCcDdOoFfHh".contains(c))
            {
                return Err(self.error(format!("{flvr}: incorrect flavour")));
            }
        }
        let obox = w[4].clone().map(|s| Self::check_dir_path(&s));
        let ibox = w[5].clone().map(|s| Self::check_dir_path(&s));

        let (pwd, pkt_pwd, out_pwd) = split_passwords(w[2].clone());
        let spec = NodeSpec {
            hosts: w[1].clone(),
            pwd,
            pkt_pwd,
            out_pwd,
            obox_flvr: w[3].as_ref().and_then(|s| s.bytes().next()),
            obox,
            ibox,
            nr_flag: Some(nr_flag),
            nd_flag: Some(nd_flag),
            md_flag: Some(md_flag),
            restrict_ip: Some(restrict_ip),
            hc_flag: Some(hc_flag),
            np_flag: Some(np_flag),
            bw_send: Some(bw_send),
            bw_recv: Some(bw_recv),
            pipe,
            ip_afamily: Some(ip_afamily),
        };
        let index = self.config.spool.nodes.add(&fa, &spec, self.now);
        self.config.spool.nodes.nodes_mut()[index].listed = 1;
        Ok(())
    }

    /// The port of the `read_skip()` callback.
    fn skip(&mut self, words: &[String], obsolete: bool) -> Result<(), Error> {
        let mut destr = obsolete;
        let mut at = addrtype::A_ALL;
        let mut size: i64 = 0;
        let mut mask_only = 0;
        for (i, word) in words.iter().enumerate() {
            let mut w = word.as_str();
            if i == 0 && w.starts_with(|c: char| c.is_ascii_alphabetic()) && !obsolete {
                at = parse_addrtype(w)
                    .ok_or_else(|| self.error(format!("incorrect address type '{w}'")))?;
                continue;
            }
            if i < 2 && mask_only == 0 && !obsolete {
                if w == "-" {
                    size = -1;
                    continue;
                }
                if let Some(rest) = w.strip_prefix('!') {
                    destr = true;
                    w = rest;
                }
                if !w.starts_with(|c: char| c.is_ascii_digit()) {
                    return Err(self.need_number(w));
                }
                size = w
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0);
                mask_only = 1;
                continue;
            }
            self.config.skipmask.push(SkipChain {
                mask: w.to_string(),
                atype: at,
                size: if size > 0 { size << 10 } else { size },
                destr,
            });
            mask_only = 2;
        }
        if mask_only != 2 {
            return Err(self.error("expecting at least one filemask".to_string()));
        }
        Ok(())
    }

    /// The port of the `read_check_pkthdr()` callback.
    fn check_pkthdr(&mut self, words: &[String]) -> Result<(), Error> {
        if words.is_empty() {
            return Err(self.syntax("check-pkthdr"));
        }
        let mut w = words[0].clone();
        match parse_addrtype(&w) {
            Some(t) => {
                self.config.spool.pkthdr_type = t;
                self.argc(2, words.len())?;
                w = words[1].clone();
            }
            None => {
                self.config.spool.pkthdr_type = addrtype::A_ALL;
                self.argc(1, words.len())?;
            }
        }
        self.config.spool.pkthdr_bad = Some(w.trim_start_matches('.').to_string());
        Ok(())
    }

    /// The port of the `read_mask()` callback.
    fn mask_arg(&self, words: &[String]) -> Result<Vec<String>, Error> {
        if words.is_empty() {
            return Err(self.syntax("mask"));
        }
        Ok(words.to_vec())
    }

    /// The port of the `read_akachain()` callback.
    fn akachain(&mut self, words: &[String], kind: i32) -> Result<(), Error> {
        self.argc(2, words.len())?;
        let mut fa = self.parse_address(&words[0])?;
        fa.expand(&self.config.spool.aka, &self.config.spool.domains);
        let mut mask = words[1].clone();
        let mut invert = false;
        if let Some(rest) = mask.strip_prefix('!') {
            invert = true;
            mask = rest.to_string();
        }
        self.config.akamask.push(AkaChain {
            fa,
            mask,
            kind,
            invert,
        });
        Ok(())
    }

    /// The port of the `read_listen()` callback.
    fn listen(&mut self, words: &[String]) -> Result<(), Error> {
        if words.is_empty() {
            return Err(self.syntax("listen"));
        }
        for word in words {
            let mut addr: String = word.chars().take(42).collect();
            let mut port = String::new();
            if word.starts_with('[') {
                if let Some(end) = word.find(']') {
                    if word[end + 1..].starts_with(':') {
                        port = word[end + 2..].to_string();
                        addr = word[..=end].to_string();
                    }
                }
            } else if let Some(colon) = word.find(':') {
                port = word[colon + 1..].to_string();
                addr = word[..colon].to_string();
            }
            if (self.ports)(&port).is_none() {
                return Err(self.error(format!("{port}: bad port number")));
            }
            if addr == "*" {
                addr.clear();
            }
            self.config.listen.push(ListenChain { addr, port });
        }
        Ok(())
    }

    /// The port of the `read_rfrule()` callback.
    fn rfrule(&mut self, words: &[String]) -> Result<(), Error> {
        self.argc(2, words.len())?;
        self.config.rf_rules.push(binkd_spool::flo::RfRule {
            from: words[0].clone(),
            to: words[1].clone(),
        });
        Ok(())
    }

    /// The port of the `read_flag_exec_info()` callback.
    fn flag_exec(&mut self, words: &[String], kind: char) -> Result<(), Error> {
        if words.len() < 2 {
            return Err(self.syntax(if kind == 'f' { "flag" } else { "exec" }));
        }
        let path = words[0].clone();
        for pattern in &words[1..] {
            let mut event = EvtFlag {
                pattern: pattern.clone(),
                ..EvtFlag::default()
            };
            if let Some(rest) = path.strip_prefix('!') {
                event.imm = true;
                if kind == 'f' {
                    event.path = Some(rest.to_string());
                } else {
                    event.command = Some(rest.to_string());
                }
            } else if kind == 'f' {
                event.path = Some(path.clone());
            } else {
                event.command = Some(path.clone());
            }
            self.config.evt_flags.push(event);
        }
        Ok(())
    }

    /// The port of the `read_zrule()` callback.
    fn zrule(&mut self, words: &[String], allow: bool) -> Result<(), Error> {
        if words.is_empty() {
            return Err(self.error("at least one mask expected".to_string()));
        }
        for word in words {
            self.config.zrules.push(ZRule {
                mask: word.clone(),
                allow,
            });
        }
        Ok(())
    }

    /// The port of `parse_rate()`.
    fn parse_rate(w: &str, err: &mut Option<String>) -> i64 {
        if w == "-" {
            return 0;
        }
        let digits: String = w.chars().take_while(|c| c.is_ascii_digit()).collect();
        let mut rate: i64 = digits.parse().unwrap_or(0);
        let rest = &w[digits.len()..];
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            match c {
                'k' | 'K' => {
                    rate <<= 10;
                }
                'm' | 'M' => {
                    rate <<= 20;
                }
                '%' => {
                    rate = -rate;
                }
                _ => {
                    *err = Some(rest.to_string());
                    return 0;
                }
            }
            let rest = chars.as_str();
            if !rest.is_empty() {
                *err = Some(rest.to_string());
                return 0;
            }
        }
        rate
    }

    /// The port of the `read_rate()` callback.
    fn rate(&mut self, words: &[String]) -> Result<(), Error> {
        let mut at = addrtype::A_ALL;
        let mut rate: i64 = 0;
        let mut mask_only = 0;
        for (i, word) in words.iter().enumerate() {
            let w = word.as_str();
            if i == 0 && w.starts_with(|c: char| c.is_ascii_alphabetic()) {
                at = parse_addrtype(w)
                    .ok_or_else(|| self.error(format!("incorrect address type '{w}'")))?;
                continue;
            }
            if i < 2 && mask_only == 0 {
                let mut err = None;
                rate = Self::parse_rate(w, &mut err);
                if let Some(e) = err {
                    return Err(self.error(format!("syntax error near '{e}'")));
                }
                mask_only = 1;
                continue;
            }
            self.config.rates.push(RateChain {
                mask: w.to_string(),
                atype: at,
                rate,
            });
            mask_only = 2;
        }
        if mask_only != 2 {
            return Err(self.error("expecting at least one filemask".to_string()));
        }
        Ok(())
    }

    /// The port of the `read_perlvar()` callback.
    fn perlvar(&mut self, words: &[String]) -> Result<(), Error> {
        if words.len() != 2 {
            return Err(self.syntax("perl-var"));
        }
        self.config.perl_vars.push(PerlVar {
            name: words[0].clone(),
            value: words[1].clone(),
        });
        Ok(())
    }

    /// The port of the `read_shares()` callback.
    fn share(&mut self, words: &[String]) -> Result<(), Error> {
        if words.len() < 2 {
            return Err(self.syntax("share"));
        }
        let mut sha = self.parse_address(&words[0])?;
        sha.expand(&self.config.spool.aka, &self.config.spool.domains);

        // The C code simulates a `node <share_addr>` line for the outgoing mail.
        self.node(&[words[0].clone()], false)?;

        let mut addrs = Vec::new();
        for word in &words[1..] {
            let mut fa = self.parse_address(word)?;
            fa.expand(&self.config.spool.aka, &self.config.spool.domains);
            addrs.push(fa);
        }
        self.config
            .spool
            .shares
            .push(binkd_spool::config::Share { sha, addrs });
        Ok(())
    }

    /// The port of `read_passwords()`.
    fn read_passwords(&mut self, path: &str) -> Result<(), Error> {
        let full = match &self.base_dir {
            Some(dir) => dir.join(path),
            None => PathBuf::from(path),
        };
        let text = fs::read_to_string(&full)
            .map_err(|_| self.error(format!("unable to open password file ({path})")))?;
        // The C code adds the file to the config list of the reload.
        let mtime = fs::metadata(&full)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.config.config_list.push(ConfigFile {
            path: path.to_string(),
            mtime,
        });
        let old_path = std::mem::replace(&mut self.path, path.to_string());
        let old_line = std::mem::replace(&mut self.line, 0);

        for (i, line) in text.lines().enumerate() {
            self.line = i + 1;
            let mut words = line.split_whitespace();
            let mut node = words.next();
            if node.is_some_and(|n| n.eq_ignore_ascii_case("password")) {
                // The ifcico and qico password file.
                node = words.next();
            }
            let Some(node) = node else {
                continue;
            };
            let Some(password) = words.next() else {
                continue;
            };
            let Ok(mut fa) = self.parse_address(node) else {
                // The C code ignores a line with garbage.
                continue;
            };
            let (pwd, pkt_pwd, out_pwd) = split_passwords(Some(password.to_string()));
            fa.expand(&self.config.spool.aka, &self.config.spool.domains);
            let spec = NodeSpec {
                pwd,
                pkt_pwd,
                out_pwd,
                obox_flvr: Some(b'-'),
                ..NodeSpec::default()
            };
            let index = self.config.spool.nodes.add(&fa, &spec, self.now);
            if self.config.spool.nodes.nodes()[index].listed == 0 {
                self.config.spool.nodes.nodes_mut()[index].listed = 2;
            }
        }

        self.path = old_path;
        self.line = old_line;
        Ok(())
    }

    /// The port of `check_config()`: the inbound and the temporary inbound
    /// directory must be in the same partition.
    ///
    /// The check needs the device number of the file system; the port keeps it
    /// on UNIX.
    #[cfg(unix)]
    fn check_directories(&self) -> Result<(), Error> {
        use std::os::unix::fs::MetadataExt;

        if !self.config.spool.temp_inbound.is_empty() {
            let temp = fs::metadata(&self.config.spool.temp_inbound)
                .map(|m| m.dev())
                .ok();
            if let (Some(temp), Ok(inbound)) = (temp, fs::metadata(&self.config.inbound)) {
                if inbound.dev() != temp {
                    return Err(self.error(
                        "Inbound and temp-inbound must be in the same partition".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// The directory check needs the device number of the file system.
    #[cfg(not(unix))]
    fn check_directories(&self) -> Result<(), Error> {
        Ok(())
    }
}

/// Splits the `in,pkt,out` password of the `read_node_info()` callback.
///
/// The port of `split_passwords()`. The C code replaces the commas with NUL
/// bytes, so the main password is the part before the first comma.
fn split_passwords(password: Option<String>) -> (Option<String>, Option<String>, Option<String>) {
    let Some(password) = password else {
        return (None, None, None);
    };
    let mut parts = password.splitn(3, ',');
    let main = parts.next().unwrap_or_default().to_string();
    let pkt = parts.next().map(|s| s.to_string());
    let out = parts.next().map(|s| s.to_string());
    (
        Some(main),
        pkt.filter(|s| !s.is_empty()),
        out.filter(|s| !s.is_empty()),
    )
}

/// Parses an address type of the `skip`, `check-pkthdr`, and `limit-rate`
/// directives.
///
/// The port of `parse_addrtype()`.
fn parse_addrtype(w: &str) -> Option<i32> {
    if w.eq_ignore_ascii_case("all") {
        Some(addrtype::A_ALL)
    } else if w.eq_ignore_ascii_case("secure") {
        Some(addrtype::A_PROT)
    } else if w.eq_ignore_ascii_case("unsecure") {
        Some(addrtype::A_UNPROT)
    } else if w.eq_ignore_ascii_case("listed") {
        Some(addrtype::A_LST)
    } else if w.eq_ignore_ascii_case("unlisted") {
        Some(addrtype::A_UNLST)
    } else {
        None
    }
}

/// Parses a config file and checks it.
///
/// The port of `readcfg()`.
pub fn parse(path: &str) -> Result<Config, Error> {
    Parser::new().parse(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_case(name: &str, text: &str) -> String {
        let mut path = std::env::temp_dir();
        path.push(format!("binkd-config-{}-{name}.conf", std::process::id()));
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(text.as_bytes()).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn base(dir: &str) -> String {
        format!(
            "sysname test\nsysop test\nlocation test\nnodeinfo test\n\
             domain fidonet {dir} 2\naddress 2:5047/13@fidonet\ninbound {dir}\n"
        )
    }

    /// The `-bw` option of a node, the port of the `read_node_info()` case.
    #[test]
    fn the_bw_option_of_a_node() {
        let dir = std::env::temp_dir();
        let dir = dir.to_string_lossy();
        let text = format!(
            "{}node 2:5047/100@fidonet host.example.com secret -bw 2k/50%%\n\
             node 2:5047/101@fidonet host.example.com secret -bw -\n",
            base(&dir)
        );
        let path = write_case("bw", &text);
        let mut parser = Parser::new();
        let mut features = crate::keywords::Features::unix_default();
        features.bwlim = true;
        parser.set_features(features);
        let config = parser.parse(&path).unwrap();
        let nodes = config.spool.nodes.nodes();
        let node = nodes.iter().find(|node| node.fa.node == 100).unwrap();
        // One value asks for both directions, `2k` is 2048 bytes per second,
        // and `%%` makes the value a percent. The tokenizer of the C code eats a
        // single `%`, the port of the `%VAR%` substitution of `getwordx2()`, so a
        // config needs the doubled `%%`.
        assert_eq!(node.bw_send, 2048);
        assert_eq!(node.bw_recv, -50);
        let node = nodes.iter().find(|node| node.fa.node == 101).unwrap();
        assert_eq!(node.bw_send, 0);
        assert_eq!(node.bw_recv, 0);
        // The C code holds the option with `BW_LIM` only.
        let mut parser = Parser::new();
        assert!(parser.parse(&path).is_err());
    }

    #[test]
    fn minimal_config() {
        let dir = std::env::temp_dir();
        let dir = dir.to_string_lossy();
        let path = write_case("minimal", &base(&dir));
        let config = parse(&path).unwrap();
        assert_eq!(config.sysname, "test");
        assert_eq!(config.spool.aka.len(), 1);
        assert_eq!(config.spool.domains.domains().len(), 1);
        assert_eq!(config.spool.domains.domains()[0].name, "fidonet");
        // The C code splits the outbound path: the `path` is the parent and
        // the `dir` is the last component.
        let base = std::path::PathBuf::from(&*dir);
        let trimmed = dir.trim_end_matches('/');
        assert_eq!(
            config.spool.domains.domains()[0].path,
            base.parent().unwrap().to_string_lossy()
        );
        assert_eq!(
            config.spool.domains.domains()[0].dir,
            base.file_name().unwrap().to_string_lossy()
        );
        assert_eq!(config.spool.inbound_nonsecure, trimmed);
        assert_eq!(config.loglevel, 4);
        assert_eq!(config.listen.len(), 1);
        assert_eq!(config.listen[0].port, "binkp");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn unknown_keyword() {
        let dir = std::env::temp_dir();
        let dir = dir.to_string_lossy();
        let path = write_case("unknown", &format!("{}nosuchkeyword 1\n", base(&dir)));
        let e = parse(&path).unwrap_err();
        assert_eq!(e.line, 8);
        assert!(e.message.contains("unknown keyword"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn node_and_time() {
        let dir = std::env::temp_dir();
        let dir = dir.to_string_lossy();
        let text = format!(
            "{}\nnode 2:5047/100@fidonet host.example.com secret i - - -nd -hc\n\
             try 5\nhold 1h30m\nkill-old-bsy 2d\nskip 100 *.pkt\n",
            base(&dir)
        );
        let path = write_case("node", &text);
        let config = parse(&path).unwrap();
        assert_eq!(config.spool.nodes.nodes().len(), 1);
        let node = &config.spool.nodes.nodes()[0];
        assert_eq!(node.hosts.as_deref(), Some("host.example.com"));
        assert_eq!(node.pwd, "secret");
        assert_eq!(node.obox_flvr, b'i');
        assert_eq!(node.nd_flag, 1);
        assert_eq!(node.hc_flag, 1);
        assert_eq!(config.spool.tries, 5);
        assert_eq!(config.spool.hold, 90 * 60);
        assert_eq!(config.spool.kill_old_bsy, 2 * 24 * 60 * 60);
        assert_eq!(config.skipmask.len(), 1);
        assert_eq!(config.skipmask[0].size, 100 * 1024);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_required_value() {
        let dir = std::env::temp_dir();
        let dir = dir.to_string_lossy();
        let path = write_case(
            "missing",
            &format!("domain fidonet {dir} 2\naddress 2:5047/13@fidonet\n"),
        );
        let e = parse(&path).unwrap_err();
        assert!(e.message.contains("'sysname' must be defined"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn include_file() {
        let dir = std::env::temp_dir();
        let dir = dir.to_string_lossy();
        let included = write_case("included", "sysname from-include\n");
        let text = format!(
            "include {included}\nsysop test\nlocation test\nnodeinfo test\n\
             domain fidonet {dir} 2\naddress 2:5047/13@fidonet\ninbound {dir}\n"
        );
        let path = write_case("include", &text);
        let config = parse(&path).unwrap();
        assert_eq!(config.sysname, "from-include");
        assert_eq!(config.config_list.len(), 2);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&included);
    }

    #[test]
    fn time_interval() {
        let parser = Parser::new();
        assert_eq!(
            parser
                .time_arg(&["1h30m".to_string()], 0, DONT_CHECK)
                .unwrap(),
            5400
        );
        assert_eq!(
            parser.time_arg(&["2d".to_string()], 0, DONT_CHECK).unwrap(),
            172800
        );
        assert_eq!(
            parser
                .time_arg(&["-5m".to_string()], DONT_CHECK, DONT_CHECK)
                .unwrap(),
            -300
        );
        assert!(parser.time_arg(&["x".to_string()], 0, DONT_CHECK).is_err());
    }
}
