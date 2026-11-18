//! The config model, the port of the `BINKD_CONFIG` structure of `readcfg.h`.

use binkd_ftn::addr::FtnAddr;
use binkd_spool::config::SpoolConfig;

/// A config file that the parser read.
#[derive(Clone, Debug)]
pub struct ConfigFile {
    /// The path of the file.
    pub path: String,
    /// The time of the last change, or 0.
    pub mtime: i64,
}

/// One entry of the `listen` directive.
#[derive(Clone, Debug, Default)]
pub struct ListenChain {
    /// The address, or empty for every address.
    pub addr: String,
    /// The port name.
    pub port: String,
}

/// One entry of the `hide-aka` and `present-aka` directives.
#[derive(Clone, Debug)]
pub struct AkaChain {
    /// The address.
    pub fa: FtnAddr,
    /// The mask.
    pub mask: String,
    /// The type: `ACT_HIDE` or `ACT_PRESENT`.
    pub kind: i32,
    /// The mask starts with `!`.
    pub invert: bool,
}

/// One entry of the `flag` and `exec` directives.
#[derive(Clone, Debug, Default)]
pub struct EvtFlag {
    /// The file name of a `flag` entry.
    pub path: Option<String>,
    /// The command of an `exec` entry.
    pub command: Option<String>,
    /// The pattern.
    pub pattern: String,
    /// The entry runs at once.
    pub imm: bool,
}

/// One entry of the `skip` directive.
#[derive(Clone, Debug)]
pub struct SkipChain {
    /// The mask.
    pub mask: String,
    /// The address type.
    pub atype: i32,
    /// The size in bytes, or -1.
    pub size: i64,
    /// The file is deleted, not skipped.
    pub destr: bool,
}

/// One entry of the `limit-rate` directive.
#[derive(Clone, Debug)]
pub struct RateChain {
    /// The mask.
    pub mask: String,
    /// The address type.
    pub atype: i32,
    /// The rate, or a negative percent value.
    pub rate: i64,
}

/// One entry of the `zallow` and `zdeny` directives.
#[derive(Clone, Debug)]
pub struct ZRule {
    /// The mask.
    pub mask: String,
    /// True for `zallow`.
    pub allow: bool,
}

/// One value of the `perl-var` directive.
#[derive(Clone, Debug)]
pub struct PerlVar {
    pub name: String,
    pub value: String,
}

/// The `dont-send-empty` values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DontSendEmpty {
    /// Send empty files.
    #[default]
    No,
    /// Send empty arc mail only.
    Arcmail,
    /// Do not send empty files.
    All,
}

/// The parsed config.
///
/// The port of `BINKD_CONFIG`. The C code keeps the data in one structure and
/// hands out pointers to it; the port owns the value.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// The part of the config that the spool layer uses.
    pub spool: SpoolConfig,
    /// The config files that the parser read.
    pub config_list: Vec<ConfigFile>,
    /// The `listen` entries.
    pub listen: Vec<ListenChain>,
    /// The `nolog` masks.
    pub nolog: Vec<String>,
    /// The `hide-aka` and `present-aka` entries.
    pub akamask: Vec<AkaChain>,
    /// The `flag` and `exec` entries.
    pub evt_flags: Vec<EvtFlag>,
    /// The `ftrans` rules.
    pub rf_rules: Vec<binkd_spool::flo::RfRule>,
    /// The `zallow` and `zdeny` rules.
    pub zrules: Vec<ZRule>,
    /// The `limit-rate` rules.
    pub rates: Vec<RateChain>,
    /// The `skip` and `skipmask` entries.
    pub skipmask: Vec<SkipChain>,
    /// The `perl-var` values.
    pub perl_vars: Vec<PerlVar>,

    /// `iport`: the port of an incoming session.
    pub iport: String,
    /// `oport`: the port of an outgoing session.
    pub oport: String,
    /// `oblksize`: the block size of the protocol.
    pub oblksize: i32,
    /// `zminsize`: the smallest file to compress.
    pub zminsize: i32,
    /// `zlevel`: the compression level.
    pub zlevel: i32,
    /// `timeout`: the network timeout in seconds.
    pub nettimeout: i32,
    /// `connect-timeout`: the connect timeout in seconds.
    pub connect_timeout: i32,
    /// `rescan-delay`: the delay before the next scan.
    pub rescan_delay: i32,
    /// `call-delay`: the delay between two calls.
    pub call_delay: i32,
    /// `maxservers`: the number of server sessions.
    pub max_servers: i32,
    /// `maxclients`: the number of client sessions.
    pub max_clients: i32,
    /// `hold-skipped`: the hold time of a skipped node.
    pub hold_skipped: i32,
    /// `tzoff`: the time zone offset in seconds, or -1.
    pub tzoff: i32,
    /// `loglevel`: the log level.
    pub loglevel: i32,
    /// `conlog`: the console log level.
    pub conlog: i32,
    /// `printq`: print the queue.
    pub printq: bool,
    /// `percents`: print the progress.
    pub percents: bool,
    /// `backresolv`: resolve the address of the peer.
    pub backresolv: bool,
    /// `prescan`: scan the outbound before the call.
    pub prescan: bool,
    /// `debugcfg`: the `-d` option.
    pub debugcfg: bool,
    /// `root-domain`: the root domain of the addresses.
    pub root_domain: String,
    /// `sysname`: the name of the system.
    pub sysname: String,
    /// `bindaddr`: the address to bind.
    pub bindaddr: String,
    /// `sysop`: the name of the sysop.
    pub sysop: String,
    /// `location`: the location.
    pub location: String,
    /// `nodeinfo`: the information about the node.
    pub nodeinfo: String,
    /// `inbound`: the inbound directory.
    pub inbound: String,
    /// `log`: the log file.
    pub logpath: String,
    /// `binlog`: the binary log file.
    pub binlogpath: String,
    /// `fdinhist`: the FrontDoor inbound history file.
    pub fdinhist: String,
    /// `fdouthist`: the FrontDoor outbound history file.
    pub fdouthist: String,
    /// `pid-file`: the pid file.
    pub pid_file: String,
    /// `passwords`: the passwords file.
    pub passwords: String,
    /// `proxy`: the HTTP proxy.
    pub proxy: String,
    /// `socks`: the SOCKS proxy.
    pub socks: String,
    /// `zlib-dll`: the zlib library of the Windows build.
    pub zlib_dll: String,
    /// `bzlib2-dll`: the bzip2 library of the Windows build.
    pub bzlib2_dll: String,
    /// `perl-hooks`: the Perl script.
    pub perl_script: String,
    /// `perl-dll`: the Perl library of the Windows build.
    pub perl_dll: String,
    /// `perl-strict`: a broken script stops the start.
    pub perl_strict: bool,
    /// `syslog`: the syslog facility.
    pub syslog_facility: i32,
    /// `dont-send-empty`: the empty file policy.
    pub dontsendempty: DontSendEmpty,
}

impl Config {
    /// Creates a config with the defaults of `lock_config_structure()`.
    ///
    /// The C code starts from `lock_config_structure()`. The derived `Default`
    /// of the port gives a zero for every value, which is not that state, so a
    /// test that needs the defaults of the C code calls `Config::new()`.
    pub fn new() -> Config {
        Config {
            iport: "binkp".to_string(),
            oport: "binkp".to_string(),
            oblksize: 4096,
            zminsize: 1024,
            nettimeout: 5 * 60,
            call_delay: 60,
            rescan_delay: 60,
            max_servers: 100,
            max_clients: 100,
            hold_skipped: 60 * 60,
            tzoff: -1,
            loglevel: 4,
            conlog: 1,
            root_domain: "binkp.net".to_string(),
            syslog_facility: -1,
            spool: binkd_spool::config::SpoolConfig {
                minfree: -1,
                minfree_nonsecure: -1,
                ..binkd_spool::config::SpoolConfig::new()
            },
            ..Config::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let config = Config::new();
        assert_eq!(config.iport, "binkp");
        assert_eq!(config.oblksize, 4096);
        assert_eq!(config.loglevel, 4);
        assert_eq!(config.tzoff, -1);
        assert_eq!(config.root_domain, "binkp.net");
        assert_eq!(config.spool.minfree, -1);
    }
}
