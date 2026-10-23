//! The `-d` dump of the config, the port of `debug_readcfg()` of `readcfg.c`.

use binkd_ftn::node::{
    NodePassword, HC_OFF, HC_ON, MD_OFF, MD_ON, ND_ON, NP_ON, NR_ON, RIP_ON, RIP_SIP,
};
use binkd_spool::config::{addrtype, RenameStyle};

use crate::keywords::{self, Features};
use crate::model::{Config, DontSendEmpty};

/// The width of a keyword field: `%-24s ` of the C code.
const KEY_WIDTH: usize = 24;

/// Returns the dump of the config.
///
/// The port of `debug_readcfg()`. The output matches the output of the C binary
/// of the same build, byte for byte.
pub fn dump(config: &Config) -> String {
    dump_with(config, &Features::unix_default())
}

/// Returns the dump of the config for a build.
pub fn dump_with(config: &Config, features: &Features) -> String {
    let mut out = String::new();
    for keyword in keywords::active(features) {
        out.push_str(&format!("{:<width$} ", keyword.name, width = KEY_WIDTH));
        out.push_str(&value(config, keyword.name));
        out.push('\n');
    }
    out
}

/// Returns the text of the value of a keyword.
fn value(config: &Config, name: &str) -> String {
    let spool = &config.spool;
    match name {
        "include" => {
            let mut out = String::new();
            for file in &config.config_list {
                out.push_str(&format!("\n    {}", file.path));
            }
            out
        }
        "log" => quoted(&config.logpath),
        "loglevel" => config.loglevel.to_string(),
        "conlog" => config.conlog.to_string(),
        "binlog" => quoted(&config.binlogpath),
        "fdinhist" => quoted(&config.fdinhist),
        "fdouthist" => quoted(&config.fdouthist),
        "tzoff" => format_time(config.tzoff),
        "domain" => {
            let mut out = String::new();
            for domain in spool.domains.domains() {
                match domain.alias4 {
                    Some(index) => {
                        let target = &spool.domains.domains()[index];
                        out.push_str(&format!("\n    {} alias-for {}", domain.name, target.name));
                    }
                    None => out.push_str(&format!(
                        "\n    {} {}/{} {}",
                        domain.name, domain.path, domain.dir, domain.z[0]
                    )),
                }
            }
            out
        }
        "address" => {
            let mut out = String::new();
            for fa in &spool.aka {
                out.push_str(&format!("\n    {}", fa.format(false)));
            }
            out
        }
        "sysname" => quoted(&config.sysname),
        "bindaddr" => quoted(&config.bindaddr),
        "sysop" => quoted(&config.sysop),
        "location" => quoted(&config.location),
        "nodeinfo" => quoted(&config.nodeinfo),
        "iport" => quoted(&config.iport),
        "oport" => quoted(&config.oport),
        "rescan-delay" => format_time(config.rescan_delay),
        "call-delay" => format_time(config.call_delay),
        "timeout" => format_time(config.nettimeout),
        "oblksize" => config.oblksize.to_string(),
        "maxservers" => config.max_servers.to_string(),
        "maxclients" => config.max_clients.to_string(),
        "inbound" => quoted(&config.inbound),
        "inbound-nonsecure" => quoted(&spool.inbound_nonsecure),
        "temp-inbound" => quoted(&spool.temp_inbound),
        "node" => node_list(config),
        "defnode" => {
            if spool.havedefnode {
                "[see 0:0/0@defnode]".to_string()
            } else {
                "<not defined>".to_string()
            }
        }
        "kill-dup-partial-files" => yes_or_no(spool.kill_dup_partial_files),
        "kill-old-partial-files" => format_time(spool.kill_old_partial_files as i32),
        "kill-old-bsy" => format_time(spool.kill_old_bsy as i32),
        "percents" => yes_or_no(config.percents),
        "minfree" => spool.minfree.to_string(),
        "minfree-nonsecure" => spool.minfree_nonsecure.to_string(),
        "flag" => event_list(config, 'f'),
        "exec" => event_list(config, 'e'),
        "printq" => yes_or_no(config.printq),
        "try" => spool.tries.to_string(),
        "hold" => format_time(spool.hold as i32),
        "hold-skipped" => format_time(config.hold_skipped),
        "backresolv" => yes_or_no(config.backresolv),
        "pid-file" => quoted(&config.pid_file),
        "remove-try-files" => yes_or_no(spool.remove_try_files),
        "proxy" => quoted(&config.proxy),
        "socks" => quoted(&config.socks),
        "ftrans" => {
            let mut out = String::new();
            for rule in &config.rf_rules {
                out.push_str(&format!("\n    \"{}\" => \"{}\"", rule.from, rule.to));
            }
            out
        }
        "send-if-pwd" => yes_or_no(spool.send_if_pwd),
        "root-domain" => quoted(&config.root_domain),
        "prescan" => yes_or_no(config.prescan),
        "connect-timeout" => format_time(config.connect_timeout),
        "filebox" => quoted(&spool.tfilebox),
        "brakebox" => quoted(&spool.bfilebox),
        "deletebox" => yes_or_no(spool.deleteablebox),
        "skip" | "skipmask" => {
            let mut out = String::new();
            for skip in &config.skipmask {
                out.push_str(&format!(
                    "\n    {} {}{} \"{}\"",
                    describe_addrtype(skip.atype),
                    if skip.destr { '!' } else { ' ' },
                    if skip.size < 0 {
                        skip.size
                    } else {
                        skip.size >> 10
                    },
                    skip.mask
                ));
            }
            out
        }
        "inboundcase" => match spool.inboundcase {
            binkd_ftn::text::InbCase::Save => "save".to_string(),
            binkd_ftn::text::InbCase::Upper => "upper".to_string(),
            binkd_ftn::text::InbCase::Lower => "lower".to_string(),
            binkd_ftn::text::InbCase::Mixed => "mixed".to_string(),
        },
        "deletedirs" => yes_or_no(spool.deletedirs),
        "overwrite" => mask_list(&spool.overwrite),
        "rename-style" => match spool.renamestyle {
            RenameStyle::Postfix => "postfix".to_string(),
            RenameStyle::Extension => "extenstion".to_string(),
            RenameStyle::Body => "???".to_string(),
        },
        "share" => {
            let mut out = String::new();
            for share in &spool.shares {
                out.push_str(&format!("\n    {}", share.sha.format(false)));
                for fa in &share.addrs {
                    out.push_str(&format!(" {}", fa.format(false)));
                }
            }
            out
        }
        "check-pkthdr" => {
            if spool.pkthdr_type == 0 {
                "<disabled>".to_string()
            } else {
                format!(
                    "{} {}",
                    describe_addrtype(spool.pkthdr_type),
                    spool.pkthdr_bad.clone().unwrap_or_default()
                )
            }
        }
        "nolog" => mask_list(&config.nolog),
        "hide-aka" => "\n    [see present-aka]".to_string(),
        "present-aka" => {
            let mut out = String::new();
            for chain in &config.akamask {
                out.push_str(&format!(
                    "\n    {} {} {}{}",
                    if chain.kind == 1 {
                        "hide-aka"
                    } else {
                        "present-aka"
                    },
                    chain.fa.format(false),
                    if chain.invert { '!' } else { ' ' },
                    chain.mask
                ));
            }
            out
        }
        "zlevel" => config.zlevel.to_string(),
        "zminsize" => config.zminsize.to_string(),
        "zallow" => {
            // The C code prints every rule under the `zallow` keyword.
            let mut out = String::new();
            for rule in &config.zrules {
                out.push_str(&format!(
                    "\n    {} {}",
                    if rule.allow { "zallow" } else { "zdeny" },
                    rule.mask
                ));
            }
            out
        }
        "zdeny" => "\n    [see zallow]".to_string(),
        "zlib-dll" => quoted(&config.zlib_dll),
        "bzlib2-dll" => quoted(&config.bzlib2_dll),
        "limit-rate" => {
            let mut out = String::new();
            for rule in &config.rates {
                out.push_str(&format!(
                    "\n    {} {} \"{}\"",
                    describe_addrtype(rule.atype),
                    describe_rate(rule.rate),
                    rule.mask
                ));
            }
            out
        }
        "perl-hooks" => quoted(&config.perl_script),
        "perl-dll" => quoted(&config.perl_dll),
        "perl-strict" => yes_or_no(config.perl_strict),
        // The C code has no branch for these: it prints the note.
        _ => "[this item cannot be displayed]".to_string(),
    }
}

/// Returns the quoted form of a string.
fn quoted(value: &str) -> String {
    format!("\"{value}\"")
}

/// Returns `[yes]` or `<not defined>`.
fn yes_or_no(value: bool) -> String {
    if value {
        "[yes]".to_string()
    } else {
        "<not defined>".to_string()
    }
}

/// Returns the list of the masks, one per line.
fn mask_list(masks: &[String]) -> String {
    let mut out = String::new();
    for mask in masks {
        out.push_str(&format!("\n    \"{mask}\""));
    }
    out
}

/// Returns the list of the `flag` and `exec` entries.
fn event_list(config: &Config, kind: char) -> String {
    let mut out = String::new();
    for event in &config.evt_flags {
        let body = if kind == 'f' {
            &event.path
        } else {
            &event.command
        };
        if let Some(body) = body {
            out.push_str(&format!(
                "\n    {}\"{}\" on \"{}\"",
                if event.imm { "[Imm] " } else { "" },
                body,
                event.pattern
            ));
        }
    }
    out
}

/// Returns the list of the nodes, the port of `print_node_info_1()`.
fn node_list(config: &Config) -> String {
    let mut nodes = config.spool.nodes.clone();
    let mut out = String::new();
    nodes.foreach(0, |node| {
        let mut pwd = node.pwd.clone();
        if node.pkt_pwd != NodePassword::Main || node.out_pwd != NodePassword::Main {
            let pkt = match &node.pkt_pwd {
                NodePassword::Main => node.pwd.clone(),
                NodePassword::None => "-".to_string(),
                NodePassword::Value(value) => value.clone(),
            };
            let out_pwd = match &node.out_pwd {
                NodePassword::Main => node.pwd.clone(),
                NodePassword::None => "-".to_string(),
                NodePassword::Value(value) => value.clone(),
            };
            pwd = format!("{pwd},{pkt},{out_pwd}");
        }

        let mut flags = String::new();
        if node.nr_flag == NR_ON {
            flags.push_str(" -nr");
        }
        if node.nd_flag == ND_ON {
            flags.push_str(" -nd");
        }
        if node.md_flag == MD_ON {
            flags.push_str(" -md");
        }
        if node.md_flag == MD_OFF {
            flags.push_str(" -nomd");
        }
        if node.hc_flag == HC_ON {
            flags.push_str(" -hc");
        }
        if node.hc_flag == HC_OFF {
            flags.push_str(" -nohc");
        }
        if node.np_flag == NP_ON {
            flags.push_str(" -noproxy");
        }
        // The C code prints the option for a node that differs from the default.
        if node.bw_send != binkd_ftn::node::BW_DEF || node.bw_recv != binkd_ftn::node::BW_DEF {
            flags.push_str(&format!(
                " -bw {}/{}",
                describe_rate(node.bw_send),
                describe_rate(node.bw_recv)
            ));
        }
        if node.restrict_ip == RIP_ON {
            flags.push_str(" -ip");
        }
        if node.restrict_ip == RIP_SIP {
            flags.push_str(" -sip");
        }

        out.push_str(&format!(
            "\n    {:<20.20} {} {} {} {} {}{}",
            node.fa.format(false),
            node.hosts.clone().unwrap_or_else(|| "-".to_string()),
            pwd,
            node.obox_flvr as char,
            node.obox.clone().unwrap_or_else(|| "-".to_string()),
            node.ibox.clone().unwrap_or_else(|| "-".to_string()),
            flags
        ));
        false
    });
    out
}

/// Returns the text of an address type.
///
/// The port of `describe_addrtype()`.
fn describe_addrtype(atype: i32) -> &'static str {
    match atype {
        addrtype::A_ALL => "all",
        addrtype::A_PROT => "secure",
        addrtype::A_UNPROT => "unsecure",
        addrtype::A_LST => "listed",
        addrtype::A_UNLST => "unlisted",
        _ => "???",
    }
}

/// Returns the text of a rate.
///
/// The port of `describe_rate()`.
fn describe_rate(rate: i64) -> String {
    if rate == 0 {
        "-".to_string()
    } else if rate < 0 {
        format!("{}%", -rate)
    } else if rate >= (1 << 20) && rate & ((1 << 20) - 1) == 0 {
        format!("{}M", rate >> 20)
    } else if rate >= (1 << 10) && rate & ((1 << 10) - 1) == 0 {
        format!("{}k", rate >> 10)
    } else {
        rate.to_string()
    }
}

/// Returns the text of a time interval.
///
/// The port of the `read_time` branch of `debug_readcfg()`.
fn format_time(value: i32) -> String {
    let mut out = String::new();
    let mut i = value;
    if i < 0 {
        out.push('-');
        i = -i;
    }
    const WEEK: i32 = 7 * 24 * 60 * 60;
    const DAY: i32 = 24 * 60 * 60;
    const HOUR: i32 = 60 * 60;
    if i >= WEEK {
        out.push_str(&format!("{}w", i / WEEK));
        i %= WEEK;
    }
    if i >= DAY {
        out.push_str(&format!("{}d", i / DAY));
        i %= DAY;
    }
    if i >= HOUR {
        out.push_str(&format!("{}h", i / HOUR));
        i %= HOUR;
    }
    if i >= 60 {
        out.push_str(&format!("{}m", i / 60));
        i %= 60;
    }
    if i > 0 || value == 0 {
        out.push_str(&format!("{i}s"));
    }
    out
}

/// Keeps the `DontSendEmpty` import used by the model.
#[allow(dead_code)]
fn _dont_send_empty(value: DontSendEmpty) -> bool {
    matches!(value, DontSendEmpty::All)
}
