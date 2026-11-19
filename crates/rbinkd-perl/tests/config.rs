//! The config variables of `perl_setup()`, the port of `perlhooks.c`.
//!
//! The script reads `%config`, `%domain`, `@addr`, `@listen`, `@ftrans`,
//! `@overwrite`, `@skip`, `%share`, and `%node` in its body. The variables are
//! set before the body runs, as in the C code.

use std::fs;
use std::path::PathBuf;

use rbinkd_config::model::{Config, ListenChain, PerlVar, SkipChain};
use rbinkd_ftn::addr::FtnAddr;
use rbinkd_ftn::domain::{DomainTable, FtnDomain};
use rbinkd_ftn::node::NodeSpec;
use rbinkd_ftn::text::InbCase;
use rbinkd_perl::{Hooks, Options};
use rbinkd_spool::config::Share;
use rbinkd_spool::flo::RfRule;

/// Returns an address of the test.
fn address(text: &str) -> FtnAddr {
    let table = DomainTable::new(Vec::new());
    FtnAddr::parse(text, &table).unwrap()
}

/// Returns a config with every field of `perl_setup()`.
fn config() -> Config {
    let domains = DomainTable::new(vec![
        FtnDomain {
            name: "fidonet".to_string(),
            path: "/spool/fido".to_string(),
            dir: "outb".to_string(),
            z: [2, -1],
            idomain: Some("fido.example".to_string()),
            alias4: None,
        },
        FtnDomain {
            name: "alias".to_string(),
            path: "/alias/path".to_string(),
            dir: "aliasdir".to_string(),
            z: [9, -1],
            idomain: Some("alias.example".to_string()),
            alias4: Some(0),
        },
        FtnDomain {
            name: "other".to_string(),
            path: "/spool/other".to_string(),
            dir: "outb".to_string(),
            z: [3, -1],
            idomain: None,
            alias4: None,
        },
    ]);
    let mut config = Config::new();
    config.spool.domains = domains;
    config.spool.aka = vec![address("2:5047/100@fidonet")];
    config.spool.inbound_nonsecure = "/inbound/nonsecure".to_string();
    config.spool.inboundcase = InbCase::Mixed;
    config.spool.temp_inbound = "/inbound/temp".to_string();
    config.spool.minfree = 1000;
    config.spool.minfree_nonsecure = 2000;
    config.spool.hold = 3600;
    config.spool.send_if_pwd = true;
    config.spool.tfilebox = "/box/file".to_string();
    config.spool.bfilebox = "/box/brake".to_string();
    config.spool.pkthdr_type = -1;
    config.spool.pkthdr_bad = Some("bad".to_string());
    config.spool.overwrite = vec!["*.tic".to_string(), "*.pkt".to_string()];
    config.spool.shares = vec![Share {
        sha: address("2:5047/100@fidonet"),
        addrs: vec![address("2:5047/101@fidonet"), address("2:5047/102@fidonet")],
    }];
    config.listen = vec![
        ListenChain {
            addr: "0.0.0.0".to_string(),
            port: "binkp".to_string(),
        },
        ListenChain {
            addr: String::new(),
            port: "24554".to_string(),
        },
    ];
    config.rf_rules = vec![RfRule {
        from: "*.pkt".to_string(),
        to: "*.cut".to_string(),
    }];
    config.skipmask = vec![SkipChain {
        mask: "*.su?".to_string(),
        atype: -1,
        size: 100,
        destr: true,
    }];
    config.perl_vars = vec![PerlVar {
        name: "custom".to_string(),
        value: "value".to_string(),
    }];
    config.logpath = "/log".to_string();
    config.loglevel = 5;
    config.conlog = 2;
    config.tzoff = 3600;
    config.sysname = "sys".to_string();
    config.sysop = "op".to_string();
    config.location = "loc".to_string();
    config.nodeinfo = "info".to_string();
    config.bindaddr = "127.0.0.1".to_string();
    config.iport = "24554".to_string();
    config.oport = "binkp".to_string();
    config.max_servers = 10;
    config.max_clients = 20;
    config.oblksize = 8192;
    config.hold_skipped = 7200;
    config.backresolv = true;
    config.root_domain = "fido.example".to_string();
    config.zminsize = 512;
    config.inbound = "/inbound".to_string();
    config.spool.add_node(
        &address("2:5047/13@fidonet"),
        &NodeSpec {
            hosts: Some("host1:24554,host2".to_string()),
            pwd: Some("secret".to_string()),
            out_pwd: Some("out".to_string()),
            obox_flvr: Some(b'd'),
            obox: Some("/box/out".to_string()),
            ibox: Some("/box/in".to_string()),
            nr_flag: Some(1),
            nd_flag: Some(0),
            md_flag: Some(1),
            hc_flag: Some(-1),
            restrict_ip: Some(2),
            np_flag: Some(1),
            ..NodeSpec::default()
        },
        0,
    );
    config
        .spool
        .add_node(&address("2:5047/14@fidonet"), &NodeSpec::default(), 0);
    config
}

/// Writes the script and returns its path and the mark.
fn script() -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("rbinkd-perl-config-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mark = dir.join("mark.txt");
    let path = dir.join("config.pl");
    fs::write(
        &path,
        format!(
            "my $m = \"{}\";\n\
             open(my $fh, \">>\", $m) or die \"open: $!\";\n\
             print $fh \"log=$config{{log}}\\n\";\n\
             print $fh \"loglevel=$config{{loglevel}}\\n\";\n\
             print $fh \"conlog=$config{{conlog}}\\n\";\n\
             print $fh \"tzoff=$config{{tzoff}}\\n\";\n\
             print $fh \"sysname=$config{{sysname}}\\n\";\n\
             print $fh \"sysop=$config{{sysop}}\\n\";\n\
             print $fh \"location=$config{{location}}\\n\";\n\
             print $fh \"nodeinfo=$config{{nodeinfo}}\\n\";\n\
             print $fh \"bindaddr=$config{{bindaddr}}\\n\";\n\
             print $fh \"iport=$config{{iport}}\\n\";\n\
             print $fh \"oport=$config{{oport}}\\n\";\n\
             print $fh \"maxservers=$config{{maxservers}}\\n\";\n\
             print $fh \"maxclients=$config{{maxclients}}\\n\";\n\
             print $fh \"oblksize=$config{{oblksize}}\\n\";\n\
             print $fh \"inbound=$config{{inbound}}\\n\";\n\
             print $fh \"inbound_nonsecure=$config{{inbound_nonsecure}}\\n\";\n\
             print $fh \"inboundcase=$config{{inboundcase}}\\n\";\n\
             print $fh \"temp_inbound=$config{{temp_inbound}}\\n\";\n\
             print $fh \"minfree=$config{{minfree}}\\n\";\n\
             print $fh \"minfree_nonsecure=$config{{minfree_nonsecure}}\\n\";\n\
             print $fh \"hold=$config{{hold}}\\n\";\n\
             print $fh \"hold_skipped=$config{{hold_skipped}}\\n\";\n\
             print $fh \"backresolv=$config{{backresolv}}\\n\";\n\
             print $fh \"send_if_pwd=$config{{send_if_pwd}}\\n\";\n\
             print $fh \"filebox=$config{{filebox}}\\n\";\n\
             print $fh \"brakebox=$config{{brakebox}}\\n\";\n\
             print $fh \"root_domain=$config{{root_domain}}\\n\";\n\
             print $fh \"check_pkthdr=\" . (exists $config{{check_pkthdr}} ? $config{{check_pkthdr}} : \"absent\") . \"\\n\";\n\
             print $fh \"pkthdr_badext=\" . (defined $config{{pkthdr_badext}} ? $config{{pkthdr_badext}} : \"undef\") . \"\\n\";\n\
             print $fh \"zminsize=$config{{zminsize}}\\n\";\n\
             print $fh \"custom=$config{{custom}}\\n\";\n\
             print $fh \"addr=\" . join(\",\", @addr) . \"\\n\";\n\
             print $fh \"listen=\" . join(\",\", map {{ \"$_->{{port}}|$_->{{addr}}\" }} @listen) . \"\\n\";\n\
             print $fh \"ftrans=\" . join(\",\", map {{ \"$_->{{from}}>$_->{{to}}\" }} @ftrans) . \"\\n\";\n\
             print $fh \"overwrite=\" . join(\",\", @overwrite) . \"\\n\";\n\
             print $fh \"skip=\" . join(\",\", map {{ \"$_->{{mask}}|$_->{{type}}|$_->{{size}}|$_->{{destr}}\" }} @skip) . \"\\n\";\n\
             for my $name (sort keys %domain) {{\n\
             \x20 my $d = $domain{{$name}};\n\
             \x20 print $fh \"domain:$name=$d->{{path}}|$d->{{dir}}|\" . (exists $d->{{defzone}} ? $d->{{defzone}} : \"absent\") . \"|\" . (defined $d->{{root_domain}} ? $d->{{root_domain}} : \"undef\") . \"\\n\";\n\
             }}\n\
             for my $sha (sort keys %share) {{\n\
             \x20 print $fh \"share:$sha=\" . join(\",\", @{{$share{{$sha}}}}) . \"\\n\";\n\
             }}\n\
             for my $fa (sort keys %node) {{\n\
             \x20 my $n = $node{{$fa}};\n\
             \x20 print $fh \"node:$fa=\" . join(\"|\", map {{ defined $n->{{$_}} ? $n->{{$_}} : \"undef\" }} qw(hosts pwd pkt_pwd out_pwd ibox obox obox_flvr)) . \"|\" . join(\",\", map {{ \"$_=\" . (exists $n->{{$_}} ? $n->{{$_}} : \"absent\") }} qw(NR ND MD HC IP NP)) . \"\\n\";\n\
             }}\n\
             close($fh);\n\
             1;\n",
            mark.display()
        ),
    )
    .unwrap();
    (path, mark)
}

#[test]
fn the_config_variables_reach_the_script() {
    let (path, mark) = script();
    let config = config();
    let options = Options {
        log: None,
        domains: config.spool.domains.clone(),
        aka: config.spool.aka.clone(),
        sender: None,
        config: Some(config),
    };
    let hooks = Hooks::load_with(&path.display().to_string(), options).unwrap();
    let text = fs::read_to_string(&mark).unwrap();
    for line in [
        "log=/log",
        "loglevel=5",
        "conlog=2",
        "tzoff=3600",
        "sysname=sys",
        "sysop=op",
        "location=loc",
        "nodeinfo=info",
        "bindaddr=127.0.0.1",
        "iport=24554",
        "oport=binkp",
        "maxservers=10",
        "maxclients=20",
        "oblksize=8192",
        "inbound=/inbound",
        "inbound_nonsecure=/inbound/nonsecure",
        "inboundcase=3",
        "temp_inbound=/inbound/temp",
        "minfree=1000",
        "minfree_nonsecure=2000",
        "hold=3600",
        "hold_skipped=7200",
        "backresolv=1",
        "send_if_pwd=1",
        "filebox=/box/file",
        "brakebox=/box/brake",
        "root_domain=fido.example",
        "check_pkthdr=-1",
        "pkthdr_badext=bad",
        "zminsize=512",
        "custom=value",
        "addr=2:5047/100@fidonet",
        "listen=binkp|0.0.0.0,24554|*",
        "ftrans=*.pkt>*.cut",
        "overwrite=*.tic,*.pkt",
        "skip=*.su?|-1|100|1",
        "domain:fidonet=/spool/fido|outb|2|fido.example",
        "domain:alias=/spool/fido|outb|2|alias.example",
        "domain:other=/spool/other|outb|3|undef",
        "share:2:5047/100@fidonet=2:5047/101@fidonet,2:5047/102@fidonet",
        "node:2:5047/13@fidonet=host1:24554,host2|secret|secret|out|/box/in|/box/out|d|NR=1,ND=absent,MD=1,HC=-1,IP=2,NP=1",
        "node:2:5047/14@fidonet=undef|-|undef|undef|undef|undef|f|NR=absent,ND=absent,MD=absent,HC=absent,IP=absent,NP=absent",
    ] {
        assert!(text.lines().any(|l| l == line), "{line}: {text}");
    }
    drop(hooks);
    let _ = fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_config_without_a_pkthdr_type_leaves_the_key_out() {
    let (path, mark) = script();
    let mut config = config();
    config.spool.pkthdr_type = 0;
    config.spool.pkthdr_bad = None;
    let options = Options {
        log: None,
        domains: config.spool.domains.clone(),
        aka: config.spool.aka.clone(),
        sender: None,
        config: Some(config),
    };
    let hooks = Hooks::load_with(&path.display().to_string(), options).unwrap();
    let text = fs::read_to_string(&mark).unwrap();
    assert!(text.contains("check_pkthdr=absent"), "{text}");
    assert!(text.contains("pkthdr_badext=undef"), "{text}");
    drop(hooks);
    let _ = fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn the_config_variables_reach_a_session_clone() {
    let dir = std::env::temp_dir().join(format!("rbinkd-perl-clone-config-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mark = dir.join("mark.txt");
    let path = dir.join("clone.pl");
    fs::write(
        &path,
        format!(
            "sub on_start {{\n\
             \x20 open(my $fh, \">>\", \"{}\") or die \"open: $!\";\n\
             \x20 print $fh \"sysname=$config{{sysname}}|inbound=$config{{inbound}}\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 0;\n\
             }}\n\
             1;\n",
            mark.display()
        ),
    )
    .unwrap();
    let config = config();
    let options = Options {
        log: None,
        domains: config.spool.domains.clone(),
        aka: config.spool.aka.clone(),
        sender: None,
        config: Some(config),
    };
    let master = Hooks::load_with(&path.display().to_string(), options).unwrap();
    let clone = master.clone_for_session().unwrap();
    clone.on_start();
    let text = fs::read_to_string(&mark).unwrap();
    assert_eq!(text.trim_end(), "sysname=sys|inbound=/inbound");
    drop(clone);
    drop(master);
    let _ = fs::remove_dir_all(dir);
}
