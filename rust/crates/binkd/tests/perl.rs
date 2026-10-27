//! The hooks of the Perl oracle, the port of `perlhooks.c`.
//!
//! The C code links the hooks only with `./configure --with-perl`, whose default
//! is no, and no test of the C tree names a hook. The test runs the oracle of
//! `BINKD_C_PERL` with the script of `tests/perl/hooks.pl` and holds the line
//! that the hook of the log changed, the port of `perl_on_log()`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use binkd_config::model::Config;
use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::{DomainTable, FtnDomain};
use binkd_ftn::node::NodeSpec;
use binkd_log::{DateTime, Log};
use binkd_net::{Caller, SessionOptions};
use binkd_proto::SendFile;

/// Builds the config of the caller of the session.
fn client_config(dir: &Path, aka: &str, port: u16) -> Config {
    let mut config = Config::new();
    config.spool.domains = DomainTable::new(vec![FtnDomain {
        name: "fidonet".to_string(),
        path: dir.display().to_string(),
        dir: "outb".to_string(),
        z: [2, -1],
        idomain: None,
        alias4: None,
    }]);
    config.spool.aka = vec![FtnAddr::parse(aka, &config.spool.domains).unwrap()];
    config.inbound = dir.join("in").display().to_string();
    config.oport = "binkp".to_string();
    let server = FtnAddr::parse("2:5047/100@fidonet", &config.spool.domains).unwrap();
    config.spool.nodes.add(
        &server,
        &NodeSpec {
            hosts: Some(format!("127.0.0.1:{port}")),
            ..NodeSpec::default()
        },
        1_000_000_000,
    );
    config
}

/// Returns the path of the C binary of the Perl hooks, when the environment
/// names it.
fn oracle() -> Option<PathBuf> {
    std::env::var("BINKD_C_PERL").ok().map(PathBuf::from)
}

/// Runs the oracle with a config of the test and a bound on the run.
fn run_oracle(oracle: &PathBuf, dir: &Path, node: bool, args: &[&str]) -> String {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/perl/hooks.pl");
    let mark = dir.join("mark.txt");
    let log = dir.join("binkd.log");
    let _ = fs::create_dir_all(dir.join("in"));
    let text = format!(
        "domain fidonet {} 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"Test system\"\n\
         sysop \"Test sysop\"\n\
         location \"Test location\"\n\
         nodeinfo \"Test node\"\n\
         inbound {}\n\
         perl-hooks {}\n\
         node 2:5047/13 127.0.0.1:1 -\n\
         loglevel 8\n\
         conlog 8\n\
         log {}\n",
        dir.join("outb").display(),
        dir.join("in").display(),
        script.display(),
        log.display()
    );
    let text = if node {
        text
    } else {
        text.replace("node 2:5047/13 127.0.0.1:1 -\n", "")
    };
    let config = dir.join("binkd.conf");
    fs::write(&config, text).unwrap();
    let out = Command::new(oracle)
        .env("BINKD_PERL_MARK", &mark)
        .args(args)
        .arg(&config)
        .output()
        .expect("cannot run the oracle");
    let text = fs::read_to_string(&mark).unwrap_or_default();
    assert!(
        !text.is_empty() || !out.status.success(),
        "the oracle holds no mark: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    text
}

/// The port of `on_start()` of `perlhooks.c`: the C code calls the sub once
/// when the managers start, and the sub answers no value. The `-n` switch ends
/// the program before the managers run, so the case holds a poll.
#[test]
fn the_oracle_of_the_perl_hook_of_the_start() {
    let Some(oracle) = oracle() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("binkd-perl-start-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let text = run_oracle(&oracle, &dir, true, &["-p", "-P", "2:5047/13"]);
    assert!(text.contains("start"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

/// The port of `on_call()` of `perlhooks.c`: the C code sets `$traf_mail`,
/// `$traf_file`, and `$addr` before the call of the node, and the return value
/// answers the C code.
#[test]
fn the_oracle_of_the_perl_hook_of_the_call() {
    let Some(oracle) = oracle() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("binkd-perl-call-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let text = run_oracle(&oracle, &dir, true, &["-p", "-P", "2:5047/13"]);
    assert!(text.contains("call"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

/// The port of `on_error()` of `perlhooks.c`: the C code sets `$addr`,
/// `$error`, and `$where` for a call that failed.
#[test]
fn the_oracle_of_the_perl_hook_of_the_error() {
    let Some(oracle) = oracle() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("binkd-perl-error-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let text = run_oracle(&oracle, &dir, true, &["-p", "-P", "2:5047/13"]);
    assert!(text.contains("call"), "{text}");
    assert!(text.contains("error"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

/// The port of `config_loaded()` and of `need_reload()` of `perlhooks.c`:
/// the C code calls the two while it reads the config, so the `-n` switch
/// reaches them, and `need_reload` answers no reload.
#[test]
fn the_oracle_of_the_perl_hooks_of_the_config() {
    let Some(oracle) = oracle() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("binkd-perl-config-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let text = run_oracle(&oracle, &dir, false, &["-n"]);
    assert!(text.contains("config_loaded"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

/// The port of the hooks of a session of `perlhooks.c`: the handshake, the
/// files, and the end, the port of the calls of `protocol.c`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_oracle_of_the_perl_hooks_of_the_session() {
    let Some(oracle) = oracle() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("binkd-perl-session-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mark = dir.join("mark.txt");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/perl/hooks.pl");
    let log = dir.join("binkd.log");
    let text = format!(
        "domain fidonet {} 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"Test system\"\n\
         sysop \"Test sysop\"\n\
         location \"Test location\"\n\
         nodeinfo \"Test node\"\n\
         inbound {}\n\
         perl-hooks {}\n\
         listen 127.0.0.1:{port}\n\
         node 2:5047/13 127.0.0.1 -\n\
         loglevel 8\n\
         conlog 8\n\
         log {}\n",
        dir.join("outb").display(),
        dir.join("in").display(),
        script.display(),
        log.display()
    );
    let config = dir.join("binkd.conf");
    fs::write(&config, text).unwrap();
    let mut child = Command::new(&oracle)
        .env("BINKD_PERL_MARK", &mark)
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the oracle");
    // The caller of the port calls the oracle and sends its packet.
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        && start.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut client = client_config(&dir, "2:5047/13@fidonet", port);
    let packet = vec![b'c'; 300];
    fs::write(dir.join("outb").join("13b70064.cut"), &packet).unwrap();
    let _ = binkd_spool::queue::Queue::scan(&mut client.spool, 1_000_000_000);
    let files = vec![SendFile {
        netname: "13b70064.cut".to_string(),
        path: Some(dir.join("outb").join("13b70064.cut")),
        size: packet.len() as u64,
        time: 1_000_000_000,
    }];
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
    };
    let mut caller = Caller::new();
    let _ = caller
        .call_next(
            &mut client,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(10), true, false),
            &|_| files.clone(),
        )
        .await;
    // The mark holds the hooks of the session.
    let start = Instant::now();
    while !mark.is_file() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let text = fs::read_to_string(&mark).unwrap_or_default();
    let _ = child.kill();
    let _ = child.wait();
    for name in [
        "on_handshake",
        "after_handshake",
        "before_send",
        "after_sent",
        "after_session",
    ] {
        assert!(text.contains(name), "{name}: {text}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_oracle_of_the_perl_hook_changes_the_line_of_the_log() {
    let Some(oracle) = oracle() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("binkd-perl-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/perl/hooks.pl");
    let log = dir.join("binkd.log");
    let text = format!(
        "domain fidonet {} 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"Test system\"\n\
         sysop \"Test sysop\"\n\
         location \"Test location\"\n\
         nodeinfo \"Test node\"\n\
         inbound {}\n\
         perl-hooks {}\n\
         loglevel 8\n\
         conlog 8\n\
         log {}\n",
        dir.join("outb").display(),
        dir.join("in").display(),
        script.display(),
        log.display()
    );
    let config = dir.join("binkd.conf");
    fs::write(&config, text).unwrap();

    let mark = dir.join("mark.txt");
    let out = Command::new(&oracle)
        .env("BINKD_PERL_MARK", &mark)
        .arg("-n")
        .arg(&config)
        .output()
        .expect("cannot run the oracle");
    let text = fs::read_to_string(&log).unwrap_or_default();
    // The hook of the port changes the message of a line of the log, the port
    // of the `$_` of `perl_on_log()` of `perlhooks.c`. The C code writes
    // the `BEGIN` line before the hooks run, so the hook changes the line of
    // the `-n` switch, whose level the hook also raises from 0 to 1.
    assert!(
        text.contains("Exit on option (perl) '-n'"),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}
