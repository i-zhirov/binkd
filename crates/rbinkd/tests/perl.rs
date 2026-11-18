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
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/perl/hooks.pl");
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
    // The C code passes `BAD_CALL` for a call that cannot connect.
    assert!(text.contains("error where=1"), "{text}");
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
    let client_dir = dir.join("client");
    fs::create_dir_all(client_dir.join("in")).unwrap();
    fs::create_dir_all(client_dir.join("outb")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mark = dir.join("mark.txt");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/perl/hooks.pl");
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
    // The packet of the oracle for the caller: the oracle sends it, so the
    // hooks of the send of a file run, the port of `perl_before_send()` and of
    // `perl_after_sent()` of `perlhooks.c`.
    fs::write(dir.join("outb").join("13b7000d.cut"), vec![b'd'; 400]).unwrap();
    let mut child = Command::new(&oracle)
        .env("BINKD_PERL_MARK", &mark)
        .arg("-s")
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
    let mut client = client_config(&client_dir, "2:5047/13@fidonet", port);
    let packet = vec![b'c'; 300];
    fs::write(client_dir.join("outb").join("13b70064.cut"), &packet).unwrap();
    let _ = binkd_spool::queue::Queue::scan(&mut client.spool, 1_000_000_000);
    let files = vec![SendFile {
        netname: "13b70064.cut".to_string(),
        path: Some(client_dir.join("outb").join("13b70064.cut")),
        size: packet.len() as u64,
        time: 1_000_000_000,
    }];
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
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
    // The mark holds the hooks of the session: the caller ends its session, and
    // the oracle holds `after_sent` when its own send of the file ends, so the
    // test waits for the hook before it ends the oracle, the port of the
    // `SIGTERM` of the C code.
    let start = Instant::now();
    let mut text = String::new();
    while !text.contains("after_session") && start.elapsed() < Duration::from_secs(10) {
        text = fs::read_to_string(&mark).unwrap_or_default();
        std::thread::sleep(Duration::from_millis(20));
    }
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
    // The C code splits the `SYS` message, the port of `msg_send2()` with two
    // parts, so the hook holds `SYS ` and the name apart.
    assert!(text.contains("on_send 0|SYS |Test system"), "{text}");
    // The answerer runs `on_handshake()` after the `ADR` of the caller, the
    // port of `ADR()`, so `@he` holds the caller's address.
    assert!(text.contains("on_handshake he=2:5047/13@fidonet"), "{text}");
    // The `$action` of `after_sent()` removed the file of the spool.
    assert!(
        !dir.join("outb").join("13b7000d.cut").exists(),
        "the action did not remove the file"
    );
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
    fs::create_dir_all(dir.join("outb")).unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/perl/hooks.pl");
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

/// The port of the hook of the log: the binary with the feature `perl` changes
/// the line of the `-n` switch, the port of `perl_on_log()` of `perlhooks.c`.
#[cfg(feature = "perl")]
#[test]
fn the_port_of_the_perl_hook_changes_the_line_of_the_log() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-port-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/perl/hooks.pl");
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
    let out = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .arg("-n")
        .arg(&config)
        .output()
        .expect("cannot run the port");
    let text = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        text.contains("Exit on option (perl) '-n'"),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Sends a signal to a process, the port of the `kill` utility.
#[cfg(feature = "perl")]
fn kill(signal: &str, pid: u32) -> bool {
    Command::new("kill")
        .arg(signal)
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// The port of the call hooks: `on_start`, `config_loaded`, `on_call`,
/// `on_error`, and `on_exit`.
#[cfg(feature = "perl")]
#[test]
fn the_port_of_the_perl_hooks_of_the_call() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-port-call-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    let mark = dir.join("mark.txt");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/perl/hooks.pl");
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
         node 2:5047/13 127.0.0.1:1 -\n\
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
    // The queue of the client manager holds a file of the node, so the pass
    // of the caller runs the hooks.
    fs::write(dir.join("outb").join("13b7000d.cut"), vec![b'd'; 100]).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .env("BINKD_PERL_MARK", &mark)
        .args(["-p"])
        .arg(&config)
        .output()
        .expect("cannot run the port");
    let text = fs::read_to_string(&mark).unwrap_or_default();
    for name in ["start", "config_loaded", "call", "error", "on_exit"] {
        assert!(text.lines().any(|line| line == name), "{name}: {text}");
    }
    // The port passes `BAD_CALL` for a call that cannot connect, the value of
    // the C `bad_try()` of `client.c`.
    assert!(text.contains("error where=1"), "{text}");
    assert!(out.status.code().is_some(), "the port did not end");
    let _ = fs::remove_dir_all(dir);
}

/// The decision of `on_log()`: the daemon runs the hook on the master
/// interpreter, not on the clone of a session. The script sets a global in a
/// session hook and logs: the master does not hold the value.
#[cfg(feature = "perl")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_on_log_hook_runs_on_the_master() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-on-log-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    let client_dir = dir.join("client");
    fs::create_dir_all(client_dir.join("in")).unwrap();
    fs::create_dir_all(client_dir.join("outb")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mark = dir.join("mark.txt");
    let script = dir.join("hooks.pl");
    fs::write(
        &script,
        format!(
            "sub mark {{\n\
             \x20 my ($line) = @_;\n\
             \x20 open(my $fh, \">>\", \"{}\") or die \"open: $!\";\n\
             \x20 print $fh \"$line\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 0;\n\
             }}\n\
             sub on_handshake {{\n\
             \x20 $who = \"session\";\n\
             \x20 Log(\"hook-log\");\n\
             \x20 return \"\";\n\
             }}\n\
             sub on_log {{\n\
             \x20 mark(\"on_log who=$who msg=$_\");\n\
             \x20 return 1;\n\
             }}\n\
             1;\n",
            mark.display()
        ),
    )
    .unwrap();
    let config = dir.join("binkd.conf");
    let log = dir.join("binkd.log");
    fs::write(
        &config,
        format!(
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
        ),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .env("BINKD_PERL_MARK", &mark)
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the port");
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        && start.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    fs::write(client_dir.join("outb").join("13b70064.cut"), vec![b'c'; 10]).unwrap();
    let mut client = client_config(&client_dir, "2:5047/13@fidonet", port);
    let _ = binkd_spool::queue::Queue::scan(&mut client.spool, 1_000_000_000);
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
    };
    let mut caller = Caller::new();
    let _ = caller
        .call_next(
            &mut client,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(2), true, false),
            &|_| Vec::new(),
        )
        .await;
    let start = Instant::now();
    let mut text = String::new();
    while !text.contains("msg=hook-log") && start.elapsed() < Duration::from_secs(10) {
        text = fs::read_to_string(&mark).unwrap_or_default();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    let _ = child.wait();
    let line = text
        .lines()
        .find(|line| line.contains("msg=hook-log"))
        .unwrap_or_else(|| panic!("no log hook line: {text}"));
    assert_eq!(line, "on_log who= msg=hook-log", "{text}");
    let _ = fs::remove_dir_all(dir);
}

/// The port of the session hooks: the handshake, the files, the messages, and
/// the end of the session.
#[cfg(feature = "perl")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_port_of_the_perl_hooks_of_the_session() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-port-session-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    let client_dir = dir.join("client");
    fs::create_dir_all(client_dir.join("in")).unwrap();
    fs::create_dir_all(client_dir.join("outb")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mark = dir.join("mark.txt");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/perl/hooks.pl");
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
    // The packet of the server: the server sends it, so the hooks of the send
    // of a file run.
    fs::write(dir.join("outb").join("13b7000d.cut"), vec![b'd'; 400]).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .env("BINKD_PERL_MARK", &mark)
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the port");
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        && start.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut client = client_config(&client_dir, "2:5047/13@fidonet", port);
    let packet = vec![b'c'; 300];
    fs::write(client_dir.join("outb").join("13b70064.cut"), &packet).unwrap();
    let _ = binkd_spool::queue::Queue::scan(&mut client.spool, 1_000_000_000);
    let files = vec![SendFile {
        netname: "13b70064.cut".to_string(),
        path: Some(client_dir.join("outb").join("13b70064.cut")),
        size: packet.len() as u64,
        time: 1_000_000_000,
    }];
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
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
    // Wait for the end of the server session before the signal.
    let start = Instant::now();
    let mut text = String::new();
    while !text.contains("after_session") && start.elapsed() < Duration::from_secs(10) {
        text = fs::read_to_string(&mark).unwrap_or_default();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    let _ = child.wait();
    for name in [
        "on_handshake",
        "after_handshake",
        "before_send",
        "after_sent",
        "before_recv",
        "on_send",
        "on_recv",
        "setup_rlimit",
        "after_session",
    ] {
        assert!(text.lines().any(|line| line == name), "{name}: {text}");
    }
    assert!(text.contains("config Test system"), "{text}");
    // The port splits the `SYS` message, the port of `msg_send2()` with two
    // parts.
    assert!(text.contains("on_send 0|SYS |Test system"), "{text}");
    // The answerer runs `on_handshake()` after the `ADR` of the caller, the
    // port of `ADR()`, so `@he` holds the caller's address.
    assert!(text.contains("on_handshake he=2:5047/13@fidonet"), "{text}");
    // The `$action` of `after_sent()` removed the file of the spool.
    assert!(
        !dir.join("outb").join("13b7000d.cut").exists(),
        "the action did not remove the file"
    );
    let _ = fs::remove_dir_all(dir);
}

/// The port of `need_reload()` and `config_loaded()` on a reload of the config.
#[cfg(feature = "perl")]
#[test]
fn the_port_of_the_perl_hook_of_the_reload() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-port-reload-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mark = dir.join("mark.txt");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/perl/hooks.pl");
    let log = dir.join("binkd.log");
    let config = dir.join("binkd.conf");
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
         rescan-delay 1\n\
         loglevel 8\n\
         conlog 8\n\
         log {}\n\
         pid-file {}\n",
        dir.join("outb").display(),
        dir.join("in").display(),
        script.display(),
        log.display(),
        dir.join("binkd.pid").display()
    );
    fs::write(&config, &text).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .env("BINKD_PERL_MARK", &mark)
        .arg("-C")
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the port");
    let start = Instant::now();
    while !dir.join("binkd.pid").is_file() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        dir.join("binkd.pid").is_file(),
        "the pid file did not appear"
    );
    std::thread::sleep(Duration::from_millis(1100));
    fs::write(&config, format!("{text}# reload\n")).unwrap();
    let start = Instant::now();
    let mut text;
    loop {
        text = fs::read_to_string(&mark).unwrap_or_default();
        let reload = text.lines().any(|line| line == "need_reload");
        let loaded = text.lines().filter(|line| *line == "config_loaded").count() >= 2;
        if (reload && loaded) || start.elapsed() >= Duration::from_secs(10) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        text.lines().any(|line| line == "need_reload"),
        "no need_reload: {text}"
    );
    assert!(
        text.lines().filter(|line| *line == "config_loaded").count() >= 2,
        "no config_loaded of the reload: {text}"
    );
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    let _ = child.wait();
    let text = fs::read_to_string(&mark).unwrap_or_default();
    assert!(
        text.lines().any(|line| line == "on_exit"),
        "no on_exit: {text}"
    );
    let _ = fs::remove_dir_all(dir);
}

/// The per-session clones: two sessions of one daemon hold their own values of
/// the script.
#[cfg(feature = "perl")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_port_of_the_perl_clones_of_two_sessions() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-clones-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mark = dir.join("mark.txt");
    let script = dir.join("clones.pl");
    fs::write(
        &script,
        format!(
            "sub before_recv {{\n\
             \x20 sleep(1);\n\
             \x20 $counter = ($counter // 0) + 1;\n\
             \x20 open(my $fh, \">>\", \"{mark}\") or die \"open: $!\";\n\
             \x20 print $fh \"$size $counter\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 0;\n\
             }}\n\
             1;\n",
            mark = mark.display()
        ),
    )
    .unwrap();
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
        dir.join("server-outb").display(),
        dir.join("in").display(),
        script.display(),
        log.display()
    );
    let config = dir.join("binkd.conf");
    fs::write(&config, text).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the port");
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        && start.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
    }

    let mut client_a = client_config(&dir.join("client-a"), "2:5047/13@fidonet", port);
    let mut client_b = client_config(&dir.join("client-b"), "2:5047/13@fidonet", port);
    fs::create_dir_all(dir.join("client-a").join("outb")).unwrap();
    fs::create_dir_all(dir.join("client-b").join("outb")).unwrap();
    let packet_a = vec![b'a'; 100];
    let packet_b = vec![b'b'; 200];
    let path_a = dir.join("client-a").join("outb").join("13b70064.cut");
    let path_b = dir.join("client-b").join("outb").join("13b70064.cut");
    fs::write(&path_a, &packet_a).unwrap();
    fs::write(&path_b, &packet_b).unwrap();
    let _ = binkd_spool::queue::Queue::scan(&mut client_a.spool, 1_000_000_000);
    let _ = binkd_spool::queue::Queue::scan(&mut client_b.spool, 1_000_000_000);
    let send_a = vec![SendFile {
        netname: "13b70064.cut".to_string(),
        path: Some(path_a),
        size: packet_a.len() as u64,
        time: 1_000_000_000,
    }];
    let send_b = vec![SendFile {
        netname: "13b70064.cut".to_string(),
        path: Some(path_b),
        size: packet_b.len() as u64,
        time: 1_000_000_000,
    }];
    let files_a = |_: &FtnAddr| send_a.clone();
    let files_b = |_: &FtnAddr| send_b.clone();
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
    };
    let mut caller_a = Caller::new();
    let mut caller_b = Caller::new();
    let options = SessionOptions::new(Duration::from_secs(5), true, false);
    let _ = tokio::join!(
        caller_a.call_next(&mut client_a, &log, &now, options.clone(), &files_a),
        caller_b.call_next(&mut client_b, &log, &now, options, &files_b),
    );
    // Wait for the two sessions of the daemon.
    let start = Instant::now();
    let mut lines = Vec::new();
    while lines.len() < 2 && start.elapsed() < Duration::from_secs(10) {
        lines = fs::read_to_string(&mark)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
            })
            .collect();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(lines.len(), 2, "{lines:?}");
    lines.sort();
    assert_eq!(lines, vec![(100, 1), (200, 1)], "{lines:?}");
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    let _ = child.wait();
    let _ = fs::remove_dir_all(dir);
}

/// The script rebuilds the queue of the session: `before_send()` appends a
/// file to `@queue`, and the session sends the file, the port of
/// `refresh_queue()` of `perlhooks.c`.
#[cfg(feature = "perl")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_port_of_the_rebuilt_session_queue() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-port-queue-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    fs::create_dir_all(dir.join("server-outb")).unwrap();
    fs::create_dir_all(dir.join("extra")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let added = vec![b'a'; 200];
    let added_path = dir.join("extra").join("13b7000e.cut");
    fs::write(&added_path, &added).unwrap();
    // The file of the scan of the server: the queue of the session before the
    // hook.
    fs::write(
        dir.join("server-outb").join("13b7000d.cut"),
        vec![b'd'; 400],
    )
    .unwrap();
    // The file of the scan of the caller: it gives the caller its node, and
    // the port of the session does not send it, because the closure of the
    // files of the caller is empty.
    fs::write(dir.join("outb").join("13b70064.cut"), vec![b'c'; 10]).unwrap();
    let script = dir.join("rebuild.pl");
    fs::write(
        &script,
        format!(
            "sub before_send {{\n\
             \x20 if (scalar(@queue) == 1) {{\n\
             \x20   push @queue, {{\n\
             \x20     file => \"{}\",\n\
             \x20     size => {},\n\
             \x20     time => 1000000000,\n\
             \x20   }};\n\
             \x20 }}\n\
             \x20 return 0;\n\
             }}\n\
             1;\n",
            added_path.display(),
            added.len()
        ),
    )
    .unwrap();
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
        dir.join("server-outb").display(),
        dir.join("in").display(),
        script.display(),
        log.display()
    );
    let config = dir.join("binkd.conf");
    fs::write(&config, text).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the port");
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        && start.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut client = client_config(&dir, "2:5047/13@fidonet", port);
    let _ = binkd_spool::queue::Queue::scan(&mut client.spool, 1_000_000_000);
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
    };
    let mut caller = Caller::new();
    let result = caller
        .call_next(
            &mut client,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(2), true, false),
            &|_| Vec::new(),
        )
        .await;
    let report = match result {
        Some(Ok(report)) => report,
        other => panic!("the call failed: {other:?}"),
    };
    assert_eq!(report.received.len(), 2, "{:?}", report.received);
    let mut got_added = false;
    for file in &report.received {
        if fs::read(&file.path).unwrap() == added {
            got_added = true;
        }
    }
    assert!(got_added, "{:?}", report.received);
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    let _ = child.wait();
    let _ = fs::remove_dir_all(dir);
}

/// The script removes a file from the queue of the session: `before_send()`
/// deletes the other entry of `@queue`, and the peer never receives the file,
/// the port of `refresh_queue()` of `perlhooks.c`.
#[cfg(feature = "perl")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_port_of_the_removed_file_of_the_session_queue() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-port-remove-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    fs::create_dir_all(dir.join("server-outb")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let cut_content = vec![b'd'; 400];
    let dut_content = vec![b'f'; 300];
    let cut = dir.join("server-outb").join("13b7000d.cut");
    let dut = dir.join("server-outb").join("13b7000d.dut");
    fs::write(&cut, &cut_content).unwrap();
    fs::write(&dut, &dut_content).unwrap();
    // The file of the scan of the caller: it gives the caller its node, and
    // the closure of the files of the caller sends no file.
    fs::write(dir.join("outb").join("13b70064.cut"), vec![b'c'; 10]).unwrap();
    let mark = dir.join("removed.txt");
    let script = dir.join("remove.pl");
    fs::write(
        &script,
        format!(
            "sub before_send {{\n\
             \x20 my $current = $file;\n\
             \x20 for my $entry (@queue) {{\n\
             \x20   next if $entry->{{file}} eq $current;\n\
             \x20   open(my $fh, \">>\", \"{}\") or die \"open: $!\";\n\
             \x20   print $fh \"$entry->{{file}}\\n\";\n\
             \x20   close($fh);\n\
             \x20 }}\n\
             \x20 @queue = grep {{ $_->{{file}} eq $current }} @queue;\n\
             \x20 return 0;\n\
             }}\n\
             1;\n",
            mark.display()
        ),
    )
    .unwrap();
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
        dir.join("server-outb").display(),
        dir.join("in").display(),
        script.display(),
        log.display()
    );
    let config = dir.join("binkd.conf");
    fs::write(&config, text).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the port");
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        && start.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut client = client_config(&dir, "2:5047/13@fidonet", port);
    let _ = binkd_spool::queue::Queue::scan(&mut client.spool, 1_000_000_000);
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
    };
    let mut caller = Caller::new();
    let result = caller
        .call_next(
            &mut client,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(2), true, false),
            &|_| Vec::new(),
        )
        .await;
    let report = match result {
        Some(Ok(report)) => report,
        other => panic!("the call failed: {other:?}"),
    };
    assert_eq!(report.received.len(), 1, "{:?}", report.received);
    let removed = fs::read_to_string(&mark).unwrap_or_default();
    let removed = removed.lines().next().expect("the script removed no file");
    let removed_content = if removed.ends_with("13b7000d.cut") {
        &cut_content
    } else {
        &dut_content
    };
    let received = fs::read(&report.received[0].path).unwrap();
    assert_ne!(received.as_slice(), removed_content.as_slice(), "{removed}");
    assert!(
        received == cut_content || received == dut_content,
        "{received:?}"
    );
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    let _ = child.wait();
    let _ = fs::remove_dir_all(dir);
}

/// The script deletes `@queue` in `before_recv()`: the C code does not refresh
/// the queue after the hook, so the incoming file is still received.
#[cfg(feature = "perl")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_port_of_the_queue_of_before_recv() {
    let dir = std::env::temp_dir().join(format!(
        "binkd-perl-port-before-recv-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    fs::create_dir_all(dir.join("server-outb")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let content = vec![b'r'; 100];
    let mark = dir.join("mark.txt");
    let script = dir.join("before-recv.pl");
    fs::write(
        &script,
        format!(
            "sub before_recv {{\n\
             \x20 @queue = ();\n\
             \x20 open(my $fh, \">>\", \"{mark}\") or die \"open: $!\";\n\
             \x20 print $fh \"before_recv \" . scalar(@queue) . \"\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 0;\n\
             }}\n\
             sub after_recv {{\n\
             \x20 open(my $fh, \">>\", \"{mark}\") or die \"open: $!\";\n\
             \x20 print $fh \"after_recv $name\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 0;\n\
             }}\n\
             1;\n",
            mark = mark.display()
        ),
    )
    .unwrap();
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
        dir.join("server-outb").display(),
        dir.join("in").display(),
        script.display(),
        log.display()
    );
    let config = dir.join("binkd.conf");
    fs::write(&config, text).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the port");
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        && start.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let path = dir.join("outb").join("13b70064.cut");
    fs::write(&path, &content).unwrap();
    let mut client = client_config(&dir, "2:5047/13@fidonet", port);
    let _ = binkd_spool::queue::Queue::scan(&mut client.spool, 1_000_000_000);
    let files = vec![SendFile {
        netname: "13b70064.cut".to_string(),
        path: Some(path),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
    };
    let mut caller = Caller::new();
    let result = caller
        .call_next(
            &mut client,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(2), true, false),
            &|_| files.clone(),
        )
        .await;
    let report = match result {
        Some(Ok(report)) => report,
        other => panic!("the call failed: {other:?}"),
    };
    assert_eq!(report.files_sent, 1);
    let start = Instant::now();
    let mut text = String::new();
    while !text.contains("after_recv") && start.elapsed() < Duration::from_secs(5) {
        text = fs::read_to_string(&mark).unwrap_or_default();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(text.contains("before_recv 0"), "{text}");
    assert!(text.contains("after_recv"), "{text}");
    // The hook accepted the file, so the inbound of the server holds it.
    let received: Vec<_> = fs::read_dir(dir.join("in"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().is_file())
        .collect();
    assert_eq!(received.len(), 1, "{received:?}");
    assert_eq!(fs::read(received[0].path()).unwrap(), content);
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    let _ = child.wait();
    let _ = fs::remove_dir_all(dir);
}

/// The script deletes `@queue` in `after_recv()`: the C code refreshes the
/// queue after the hook, so the entry is gone, the port of `refresh_queue()`.
#[cfg(feature = "perl")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_port_of_the_removed_file_of_after_recv() {
    let dir =
        std::env::temp_dir().join(format!("binkd-perl-port-after-recv-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    fs::create_dir_all(dir.join("server-outb")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let first = vec![b'1'; 100];
    let second = vec![b'2'; 200];
    let first_path = dir.join("outb").join("13b70064.pkt");
    let second_path = dir.join("outb").join("13b70065.pkt");
    fs::write(&first_path, &first).unwrap();
    fs::write(&second_path, &second).unwrap();
    // The file of the scan of the caller: it gives the caller its node.
    fs::write(dir.join("outb").join("13b70064.cut"), vec![b'c'; 10]).unwrap();
    let mark = dir.join("mark.txt");
    let script = dir.join("after-recv.pl");
    fs::write(
        &script,
        format!(
            "my $count = 0;\n\
             sub after_recv {{\n\
             \x20 $count++;\n\
             \x20 if ($count == 1) {{\n\
             \x20   push @queue, {{\n\
             \x20     file => \"/tmp/never.cut\",\n\
             \x20     size => 10,\n\
             \x20     time => 1,\n\
             \x20     sent => 1,\n\
             \x20   }};\n\
             \x20 }} else {{\n\
             \x20   @queue = ();\n\
             \x20 }}\n\
             \x20 return 0;\n\
             }}\n\
             sub before_recv {{\n\
             \x20 if ($count == 1) {{\n\
             \x20   open(my $fh, \">>\", \"{mark}\") or die \"open: $!\";\n\
             \x20   print $fh \"before_second \" . scalar(@queue) . \"\\n\";\n\
             \x20   close($fh);\n\
             \x20 }}\n\
             \x20 return 0;\n\
             }}\n\
             sub after_session {{\n\
             \x20 open(my $fh, \">>\", \"{mark}\") or die \"open: $!\";\n\
             \x20 print $fh \"after_session \" . scalar(@queue) . \"\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 0;\n\
             }}\n\
             1;\n",
            mark = mark.display()
        ),
    )
    .unwrap();
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
        dir.join("server-outb").display(),
        dir.join("in").display(),
        script.display(),
        log.display()
    );
    let config = dir.join("binkd.conf");
    fs::write(&config, text).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the port");
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        && start.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut client = client_config(&dir, "2:5047/13@fidonet", port);
    let _ = binkd_spool::queue::Queue::scan(&mut client.spool, 1_000_000_000);
    let files = vec![
        SendFile {
            netname: "13b70064.pkt".to_string(),
            path: Some(first_path),
            size: first.len() as u64,
            time: 1_000_000_000,
        },
        SendFile {
            netname: "13b70065.pkt".to_string(),
            path: Some(second_path),
            size: second.len() as u64,
            time: 1_000_000_000,
        },
    ];
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
    };
    let mut caller = Caller::new();
    let result = caller
        .call_next(
            &mut client,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(2), true, false),
            &|_| files.clone(),
        )
        .await;
    let report = match result {
        Some(Ok(report)) => report,
        other => panic!("the call failed: {other:?}"),
    };
    assert_eq!(report.files_sent, 2);
    let start = Instant::now();
    let mut text = String::new();
    while !text.contains("after_session") && start.elapsed() < Duration::from_secs(5) {
        text = fs::read_to_string(&mark).unwrap_or_default();
        std::thread::sleep(Duration::from_millis(20));
    }
    // The first `after_recv()` added the entry and the session refreshed the
    // queue, so the second `before_recv()` sees it.
    assert!(text.contains("before_second 1"), "{text}");
    // The second `after_recv()` deleted the entry and the session refreshed the
    // queue, so the end of the session holds no queue.
    assert!(text.contains("after_session 0"), "{text}");
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    let _ = child.wait();
    let _ = fs::remove_dir_all(dir);
}

/// The script deletes `@queue` in `after_sent()`: the C code does not refresh
/// the queue after the hook, so the file is still sent and the deletion has no
/// effect.
#[cfg(feature = "perl")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_port_of_the_queue_of_after_sent() {
    let dir =
        std::env::temp_dir().join(format!("binkd-perl-port-after-sent-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    fs::create_dir_all(dir.join("server-outb")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let content = vec![b'd'; 400];
    fs::write(dir.join("server-outb").join("13b7000d.cut"), &content).unwrap();
    // The file of the scan of the caller: it gives the caller its node, and
    // the closure of the files of the caller sends no file.
    fs::write(dir.join("outb").join("13b70064.cut"), vec![b'c'; 10]).unwrap();
    let mark = dir.join("mark.txt");
    let script = dir.join("after-sent.pl");
    fs::write(
        &script,
        format!(
            "sub after_sent {{\n\
             \x20 @queue = ();\n\
             \x20 open(my $fh, \">>\", \"{mark}\") or die \"open: $!\";\n\
             \x20 print $fh \"after_sent \" . scalar(@queue) . \"\\n\";\n\
             \x20 close($fh);\n\
             \x20 return 0;\n\
             }}\n\
             1;\n",
            mark = mark.display()
        ),
    )
    .unwrap();
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
        dir.join("server-outb").display(),
        dir.join("in").display(),
        script.display(),
        log.display()
    );
    let config = dir.join("binkd.conf");
    fs::write(&config, text).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the port");
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        && start.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut client = client_config(&dir, "2:5047/13@fidonet", port);
    let _ = binkd_spool::queue::Queue::scan(&mut client.spool, 1_000_000_000);
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
    };
    let mut caller = Caller::new();
    let result = caller
        .call_next(
            &mut client,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(2), true, false),
            &|_| Vec::new(),
        )
        .await;
    let report = match result {
        Some(Ok(report)) => report,
        other => panic!("the call failed: {other:?}"),
    };
    assert_eq!(report.received.len(), 1, "{:?}", report.received);
    assert_eq!(fs::read(&report.received[0].path).unwrap(), content);
    let start = Instant::now();
    let mut text = String::new();
    while !text.contains("after_sent") && start.elapsed() < Duration::from_secs(5) {
        text = fs::read_to_string(&mark).unwrap_or_default();
        std::thread::sleep(Duration::from_millis(20));
    }
    // The C `perl_after_sent()` does not refresh the queue, and the queue of
    // the session is already empty, so the deletion has no effect.
    assert!(text.contains("after_sent 0"), "{text}");
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    let _ = child.wait();
    let _ = fs::remove_dir_all(dir);
}

/// The port of `perl_init(new_config, 0)` of `checkcfg()`: a reload loads the
/// script of the new config, calls its `config_loaded()`, and the sessions use
/// it. The old script is not called again.
#[cfg(feature = "perl")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_port_of_the_reload_of_the_script() {
    let dir = std::env::temp_dir().join(format!("binkd-perl-reload-script-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("in")).unwrap();
    fs::create_dir_all(dir.join("outb")).unwrap();
    fs::create_dir_all(dir.join("server-outb")).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mark = dir.join("mark.txt");
    let script_a = dir.join("a.pl");
    let script_b = dir.join("b.pl");
    for (path, name) in [(&script_a, "A"), (&script_b, "B")] {
        fs::write(
            path,
            format!(
                "sub mark {{\n\
                 \x20 my ($line) = @_;\n\
                 \x20 open(my $fh, \">>\", \"{}\") or die \"open: $!\";\n\
                 \x20 print $fh \"$line\\n\";\n\
                 \x20 close($fh);\n\
                 \x20 return 0;\n\
                 }}\n\
                 sub on_start {{ mark(\"start {name}\"); return 1; }}\n\
                 sub config_loaded {{ mark(\"config {name}\"); return 1; }}\n\
                 sub on_handshake {{ mark(\"handshake {name}\"); return \"\"; }}\n\
                 1;\n",
                mark.display()
            ),
        )
        .unwrap();
    }
    let log = dir.join("binkd.log");
    let config = dir.join("binkd.conf");
    let text = |script: &std::path::Path| {
        format!(
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
             rescan-delay 1\n\
             loglevel 8\n\
             conlog 8\n\
             log {}\n\
             pid-file {}\n",
            dir.join("server-outb").display(),
            dir.join("in").display(),
            script.display(),
            log.display(),
            dir.join("binkd.pid").display()
        )
    };
    fs::write(&config, text(&script_a)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rbinkd"))
        .env("BINKD_PERL_MARK", &mark)
        .arg("-C")
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the port");
    // Wait for the first script of the config.
    let start = Instant::now();
    let mut seen = String::new();
    while !seen.contains("config A") && start.elapsed() < Duration::from_secs(10) {
        seen = fs::read_to_string(&mark).unwrap_or_default();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(seen.contains("config A"), "{seen}");
    // The new config names the other script.
    std::thread::sleep(Duration::from_millis(1100));
    fs::write(&config, text(&script_b)).unwrap();
    let start = Instant::now();
    while !seen.contains("config B") && start.elapsed() < Duration::from_secs(10) {
        seen = fs::read_to_string(&mark).unwrap_or_default();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(seen.contains("config B"), "{seen}");
    // The listener restarted with the new hooks; wait for its socket.
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        && start.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    // The file of the scan of the caller: it gives the caller its node, and
    // the closure of the files of the caller sends no file.
    fs::write(dir.join("outb").join("13b70064.cut"), vec![b'c'; 10]).unwrap();
    let mut client = client_config(&dir, "2:5047/13@fidonet", port);
    let _ = binkd_spool::queue::Queue::scan(&mut client.spool, 1_000_000_000);
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
    };
    let mut caller = Caller::new();
    let result = caller
        .call_next(
            &mut client,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(10), true, false),
            &|_| Vec::new(),
        )
        .await;
    assert!(matches!(result, Some(Ok(_))), "the call failed: {result:?}");
    let start = Instant::now();
    while !seen.contains("handshake B") && start.elapsed() < Duration::from_secs(10) {
        seen = fs::read_to_string(&mark).unwrap_or_default();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(seen.contains("handshake B"), "{seen}");
    assert!(!seen.contains("handshake A"), "{seen}");
    assert!(!seen.contains("start B"), "{seen}");
    assert_eq!(
        seen.lines().filter(|line| *line == "config A").count(),
        1,
        "{seen}"
    );
    assert_eq!(
        seen.lines().filter(|line| *line == "config B").count(),
        1,
        "{seen}"
    );
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    let _ = child.wait();
    let _ = fs::remove_dir_all(dir);
}
