//! The settings of a session from the config, the port of the part of
//! `init_protocol()` and of `protocol()` of `protocol.c` that reads the config.

use std::time::Duration;

use binkd_config::model::Config;
use binkd_ftn::node::FtnNode;
use binkd_proto::bw::{self, RateRule};
use binkd_proto::{Methods, Protocol, SessionConfig, SkipRule, ZRule};

/// The settings of one session that a manager passes, the port of the parts of
/// `STATE` that `do_server()` and `do_client()` set from the config.
#[derive(Clone)]
pub struct SessionOptions {
    /// The time that a session waits for the peer, the port of `nettimeout`.
    pub timeout: Duration,
    /// The session crypts its traffic, the port of the `no_crypt` field of the
    /// state, the `-r` switch.
    pub crypt: bool,
    /// The session records its frames, the port of the frame log of the C code at
    /// the loglevel 10.
    pub trace: bool,
    /// The source of the hooks of a session, or `None`.
    pub hooks: Option<binkd_proto::Factory>,
    /// Do not use CRAM-MD5, the port of `no_MD5` and of the `-m` switch.
    pub no_md5: bool,
}

impl SessionOptions {
    /// Creates the settings of a session of a manager.
    pub fn new(timeout: Duration, crypt: bool, trace: bool) -> SessionOptions {
        SessionOptions {
            timeout,
            crypt,
            trace,
            hooks: None,
            no_md5: false,
        }
    }

    /// Sets the source of the hooks of a session.
    pub fn with_hooks(mut self, hooks: binkd_proto::Factory) -> SessionOptions {
        self.hooks = Some(hooks);
        self
    }
}

/// The name and the version of the mailer of the banner, the port of `MYNAME`
/// and `MYVER` of `Config.h`.
///
/// The C code states the version in `Config.h`, and the release process updates
/// it. The port states the version of its own package.
pub fn mailer() -> String {
    format!("binkd/{}", env!("CARGO_PKG_VERSION"))
}

/// Returns the settings of a session, the port of the reads of the config in
/// `init_protocol()` and `protocol()`.
///
/// The C code keeps the config in a process wide value and reads it from every
/// function of the session. The port copies the values that a session needs into
/// the settings, so the session itself holds no config.
///
/// The caller of the session supplies the node of the peer, or `None` for a
/// session that the peer called: the port of the `to` argument of `protocol()`.
///
/// The `crypt` argument is the `no_crypt` of the C code, the `-r` switch.
pub fn session_config(
    config: &Config,
    node: Option<&FtnNode>,
    crypt: bool,
    trace: bool,
) -> SessionConfig {
    let (password, nr, nd, listed, skip, md_flag) = match node {
        Some(node) => (
            node.pwd.clone(),
            node.wants_nr(),
            node.wants_nd(),
            true,
            node_skip(config, node),
            node.md_flag,
        ),
        // The C code holds no password for a session that the peer called
        // until it reads the password of the node from the address.
        None => (String::new(), false, false, false, Vec::new(), 0),
    };
    SessionConfig {
        sysname: config.sysname.clone(),
        sysop: config.sysop.clone(),
        location: config.location.clone(),
        nodeinfo: config.nodeinfo.clone(),
        addresses: config.spool.aka.clone(),
        password,
        inbound: std::path::PathBuf::from(&config.inbound),
        version: mailer(),
        blksize: config.oblksize as usize,
        zminsize: config.zminsize.max(0) as u64,
        zlevel: config.zlevel,
        zrules: config.zrules.iter().map(zrule).collect(),
        methods: Methods::all(),
        crypt,
        no_md5: false,
        md_flag,
        nr,
        nd,
        domains: config.spool.domains.clone(),
        spool: config.spool.clone(),
        skip,
        listed,
        // The listener counts the sessions of a listener and gives the count
        // to every session that it starts, the port of `n_servers`.
        max_servers: config.max_servers,
        servers: 1,
        trace,
        // The port of the reads of the config in `protocol()`.
        // The C code answers `PRTCLVER` of `Config.h` at 1.1.
        protocol: Protocol::V11,
        rates: config
            .rates
            .iter()
            .map(|rule| RateRule {
                mask: rule.mask.clone(),
                atype: rule.atype,
                rate: rule.rate,
            })
            .collect(),
        bw_send: node.map(|node| node.bw_send).unwrap_or(bw::BW_DEF),
        bw_recv: node.map(|node| node.bw_recv).unwrap_or(bw::BW_DEF),
        prescan: config.prescan,
    }
}

/// The rules of the `skip` directive of a node, the port of `skipmask` of
/// `skip_test()` of `protocol.c`.
fn node_skip(config: &Config, _node: &FtnNode) -> Vec<SkipRule> {
    config
        .skipmask
        .iter()
        .map(|rule| SkipRule {
            mask: rule.mask.clone(),
            atype: rule.atype,
            size: rule.size,
            destr: rule.destr,
        })
        .collect()
}

/// Returns the rule of the session, the port of `struct zrule` of `readcfg.h`.
fn zrule(rule: &binkd_config::model::ZRule) -> ZRule {
    ZRule {
        mask: rule.mask.clone(),
        allow: rule.allow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use binkd_config::model::Config;

    #[test]
    fn settings_of_a_caller() {
        let mut config = Config {
            sysname: "test".to_string(),
            oblksize: 4096,
            ..Config::new()
        };
        config.spool.aka = vec![address("2:5047/13@fidonet")];
        let mut node = binkd_ftn::node::FtnNode::new(&address("2:5047/100@fidonet"));
        node.pwd = "secret".to_string();
        node.nr_flag = binkd_ftn::node::NR_ON;
        let settings = session_config(&config, Some(&node), true, false);
        assert_eq!(settings.sysname, "test");
        assert_eq!(settings.password, "secret");
        assert!(settings.nr);
        assert_eq!(settings.blksize, 4096);
        assert!(settings.crypt);
        // The banner holds the name and the version of the mailer.
        assert!(
            settings.version.starts_with("binkd"),
            "{}",
            settings.version
        );
    }

    #[test]
    fn settings_of_an_answerer() {
        let config = Config::new();
        let settings = session_config(&config, None, true, false);
        assert!(settings.password.is_empty());
        assert!(!settings.nr);
        assert!(!settings.nd);
    }

    fn address(s: &str) -> binkd_ftn::addr::FtnAddr {
        let table = binkd_ftn::domain::DomainTable::new(Vec::new());
        binkd_ftn::addr::FtnAddr::parse(s, &table).unwrap()
    }
}
