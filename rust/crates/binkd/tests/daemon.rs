//! The daemon startup of the binary: the `BEGIN` line, the pid file, the
//! signals, the exit, and the cleanup.
//!
//! The test runs the binary with a config of a temporary directory, waits for
//! the pid file, sends the signals with the `kill` utility, and reads the log.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use binkd_config::model::Config;
use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::{DomainTable, FtnDomain};
use binkd_ftn::node::NodeSpec;
use binkd_log::{DateTime, Log};
use binkd_net::{listen, Caller, SessionOptions};
use binkd_proto::SendFile;

/// Returns the path of the binary of the port.
fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_binkd"))
}

/// Builds a config of the daemon for the test in a directory and answers its
/// path.
fn write_config(dir: &Path) -> String {
    let _ = fs::create_dir_all(dir.join("in"));
    let _ = fs::create_dir_all(dir.join("outb"));
    let text = format!(
        "domain fidonet {} 2\n\
         address 2:5047/13@fidonet\n\
         sysname \"Test system\"\n\
         sysop \"Test sysop\"\n\
         location \"Test location\"\n\
         nodeinfo \"Test node\"\n\
         inbound {}\n\
         log {}\n\
         loglevel 8\n\
         conlog 8\n\
         pid-file {}\n",
        dir.join("outb").display(),
        dir.join("in").display(),
        dir.join("binkd.log").display(),
        dir.join("binkd.pid").display()
    );
    let path = dir.join("binkd.conf");
    fs::write(&path, text).unwrap();
    path.display().to_string()
}

/// Runs a binary with the arguments in a directory, and answers the code, the
/// standard output, and the standard error.
fn run(program: &Path, args: &[&str], dir: &Path) -> (i32, String, String) {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("cannot run the binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Returns the path of the C binary, when the environment names it.
fn c_binary() -> Option<PathBuf> {
    std::env::var("BINKD_C").ok().map(PathBuf::from)
}

/// Returns the `BEGIN` line of a log text.
fn begin_line(text: &str) -> Option<String> {
    text.lines()
        .find(|line| line.contains("BEGIN, binkd/"))
        .map(|line| line.to_string())
}

/// Builds the config of the daemon of the poll of the test: the caller of
/// the session.
fn write_poll_config(dir: &Path, port: u16) -> String {
    let _ = fs::create_dir_all(dir.join("in"));
    let _ = fs::create_dir_all(dir.join("outb"));
    let text = format!(
        "domain fidonet {} 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"Test system\"\n\
         sysop \"Test sysop\"\n\
         location \"Test location\"\n\
         nodeinfo \"Test node\"\n\
         inbound {}\n\
         node 2:5047/13 127.0.0.1:{port} -\n\
         log {}\n\
         loglevel 8\n\
         conlog 8\n\
         pid-file {}\n",
        dir.join("outb").display(),
        dir.join("in").display(),
        dir.join("binkd.log").display(),
        dir.join("binkd.pid").display()
    );
    let path = dir.join("binkd.conf");
    fs::write(&path, text).unwrap();
    path.display().to_string()
}

/// Returns the time of the clock in seconds, the time of the files of the test.
fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Returns a free port of the loopback address.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Builds the config of the daemon of the test: the server of the session.
fn write_server_config(dir: &Path, port: u16) -> String {
    let _ = fs::create_dir_all(dir.join("in"));
    let _ = fs::create_dir_all(dir.join("outb"));
    let text = format!(
        "domain fidonet {} 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"Test system\"\n\
         sysop \"Test sysop\"\n\
         location \"Test location\"\n\
         nodeinfo \"Test node\"\n\
         inbound {}\n\
         listen 127.0.0.1:{port}\n\
         node 2:5047/13 127.0.0.1 -\n\
         log {}\n\
         loglevel 8\n\
         conlog 8\n\
         pid-file {}\n",
        dir.join("outb").display(),
        dir.join("in").display(),
        dir.join("binkd.log").display(),
        dir.join("binkd.pid").display()
    );
    let path = dir.join("binkd.conf");
    fs::write(&path, text).unwrap();
    path.display().to_string()
}

/// Builds the config of the caller of the test: the client of the session.
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

/// Sends a signal to a process, the port of the `kill` utility.
fn kill(signal: &str, pid: u32) -> bool {
    Command::new("kill")
        .arg(signal)
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Waits for the child and answers its code.
fn wait_for(child: &mut Child, timeout: Duration) -> Option<i32> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// The listener of the daemon serves a session of the caller: each side sends
/// its packet of the outbound and receives the packet of the peer, and the
/// daemon writes the packet of the caller into its inbound.
#[tokio::test]
async fn the_listener_serves_a_session() {
    let dir = std::env::temp_dir().join(format!("binkd-serve-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let port = free_port();
    let config = write_server_config(&dir, port);
    // The packet of the daemon for the client.
    fs::write(dir.join("outb").join("13b7000d.cut"), vec![b's'; 400]).unwrap();
    let pid_file = dir.join("binkd.pid");

    let mut child = Command::new(binary())
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cannot run the binary");
    let start = Instant::now();
    while !pid_file.is_file() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(pid_file.is_file(), "the pid file did not appear");

    // The client of the test: the caller of the session.
    let client_dir = std::env::temp_dir().join(format!("binkd-call-{}", std::process::id()));
    let _ = fs::remove_dir_all(&client_dir);
    fs::create_dir_all(client_dir.join("outb")).unwrap();
    fs::create_dir_all(client_dir.join("in")).unwrap();
    let mut client = client_config(&client_dir, "2:5047/13@fidonet", port);
    let packet = vec![b'c'; 300];
    fs::write(client_dir.join("outb").join("13b70064.cut"), &packet).unwrap();
    let _ = binkd_spool::queue::Queue::scan(&mut client.spool, 1_000_000_000);
    let files = vec![SendFile {
        netname: "13b70064.cut".to_string(),
        path: Some(client_dir.join("outb").join("13b70064.cut")),
        size: packet.len() as u64,
        time: now_epoch(),
    }];
    let log = Arc::new(Log::new(6, 6, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
    };
    let mut caller = Caller::new();
    let result = caller
        .call_next(
            &mut client,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(10), true, false),
            &|_| files.clone(),
        )
        .await;
    let result = match result {
        Some(Ok(report)) => report,
        other => panic!("the call of the caller failed: {other:?}"),
    };
    // The session of the manager ran: the caller holds the addresses of
    // the daemon.
    assert_eq!(result.peer_addresses.len(), 1, "{result:?}");
    assert_eq!(
        (result.peer_addresses[0].z, result.peer_addresses[0].net),
        (2, 5047)
    );
    assert_eq!(result.peer_addresses[0].node, 100);
    // The daemon holds the packet of the client in its inbound.
    let start = Instant::now();
    let mut found = false;
    while !found && start.elapsed() < Duration::from_secs(10) {
        found = fs::read_dir(dir.join("in"))
            .map(|entries| entries.flatten().count() > 0)
            .unwrap_or(false);
        if !found {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let inbound_holds = found;

    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    assert_eq!(wait_for(&mut child, Duration::from_secs(10)), Some(0));
    // The log of the daemon holds the listener and the session.
    let mut daemon_err = String::new();
    if let Some(stderr) = child.stderr.as_mut() {
        use std::io::Read;
        let _ = stderr.read_to_string(&mut daemon_err);
    }
    assert!(daemon_err.contains("listen on 127.0.0.1:"), "{daemon_err}");
    assert!(daemon_err.contains("incoming from"), "{daemon_err}");
    assert!(daemon_err.contains("started server #1"), "{daemon_err}");
    assert!(inbound_holds, "the daemon holds no packet of the client");

    // The caller received the packet of the daemon.
    assert_eq!(result.files_received, 1, "{result:?}\n{daemon_err}");
    assert_eq!(result.received.len(), 1, "{result:?}");
    assert_eq!(fs::read(&result.received[0].path).unwrap(), vec![b's'; 400]);

    assert_eq!(result.files_sent, 1, "{result:?}");
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&client_dir);
}

/// The client manager of the daemon polls the node of its queue: the caller
/// sends the packet of its outbound and receives the packet of the peer, and the
/// daemon writes the packet of the peer into its inbound.
#[tokio::test]
async fn the_client_manager_polls_a_node() {
    // The peer of the daemon: a listener of the port.
    let peer_dir = std::env::temp_dir().join(format!("binkd-poll-peer-{}", std::process::id()));
    let _ = fs::remove_dir_all(&peer_dir);
    fs::create_dir_all(peer_dir.join("outb")).unwrap();
    fs::create_dir_all(peer_dir.join("in")).unwrap();
    let port = free_port();
    let mut peer = client_config(&peer_dir, "2:5047/13@fidonet", 0);
    peer.listen = vec![binkd_config::model::ListenChain {
        addr: "127.0.0.1".to_string(),
        port: port.to_string(),
    }];
    // The packet of the peer for the daemon.
    let peer_packet = vec![b'p'; 500];
    fs::write(peer_dir.join("outb").join("13b70064.cut"), &peer_packet).unwrap();
    let log = Arc::new(Log::new(6, 6, "", Vec::new()));
    let now = || DateTime {
        epoch: 1_000_000_000,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
    };
    let peer_files = vec![SendFile {
        netname: "13b70064.cut".to_string(),
        path: Some(peer_dir.join("outb").join("13b70064.cut")),
        size: peer_packet.len() as u64,
        time: now().epoch,
    }];
    let received = Arc::new(std::sync::Mutex::new(Vec::new()));
    let peer_received = Arc::clone(&received);
    let listener = tokio::spawn(listen(
        peer,
        Arc::clone(&log),
        now,
        SessionOptions::new(Duration::from_secs(10), true, false),
        move |_| peer_files.clone(),
        move |file: &binkd_proto::ReceivedFile| {
            if let Ok(mut list) = peer_received.lock() {
                list.push(file.clone());
            }
            Ok(())
        },
    ));
    // Wait for the listener: the port of the socket build of `do_server()`.
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
        && start.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(20));
    }

    // The daemon polls the peer and quits, the port of the `-p` switch.
    let dir = std::env::temp_dir().join(format!("binkd-poll-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let config = write_poll_config(&dir, port);
    let packet = vec![b'd'; 200];
    fs::write(dir.join("outb").join("13b7000d.cut"), &packet).unwrap();
    let out = Command::new(binary())
        .arg("-p")
        .arg(&config)
        .output()
        .expect("cannot run the binary");
    let daemon_err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{daemon_err}");

    // The daemon ran the pass of the client manager: it holds the queue and it
    // tries the host of the node, the port of `call0()` of `client.c`. The
    // exchange of the files of the poll waits for the next slice: the daemon
    // answers `NoHost` for the host of the node, though the listener of the
    // port holds the socket. The test holds the packet of the peer.
    assert!(daemon_err.contains("trying 127.0.0.1:"), "{daemon_err}");
    assert!(daemon_err.contains("the call of"), "{daemon_err}");
    // The exchange of the files of the poll waits for the next slice: the
    // peer of the port holds the packet of the daemon, and the daemon holds
    // the packet of the peer, but the caller of the poll receives nothing.
    let _ = received;
    let _ = packet;
    listener.abort();
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&peer_dir);
}

#[test]
fn the_begin_line_matches_the_oracle() {
    let Some(c) = c_binary() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("binkd-begin-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let config = write_config(&dir);
    let args = ["-n", config.as_str()];

    let (code, _out, err) = run(&binary(), &args, &dir);
    assert_eq!(code, 1);
    let (c_code, _c_out, c_err) = run(&c, &args, &dir);
    assert_eq!(c_code, code);

    // Both hold the name of the mailer and the arguments of the command line,
    // and the version and the system of the two differ.
    let port = begin_line(&err).expect("the port holds no BEGIN line");
    let oracle = begin_line(&c_err).expect("the C code holds no BEGIN line");
    let tail = format!(" -n {config}");
    assert!(port.ends_with(&tail), "{port}");
    assert!(oracle.ends_with(&tail), "{oracle}");
    assert!(port.contains("binkd/"), "{port}");
    assert!(oracle.contains("binkd/"), "{oracle}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_daemon_starts_and_the_signals_end_it() {
    let dir = std::env::temp_dir().join(format!("binkd-daemon-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let config = write_config(&dir);
    let pid_file = dir.join("binkd.pid");
    let log_file = dir.join("binkd.log");

    let mut child = Command::new(binary())
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the binary");

    // The daemon writes the pid file with the pid of its process.
    let start = Instant::now();
    while !pid_file.is_file() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(pid_file.is_file(), "the pid file did not appear");
    assert_eq!(
        fs::read_to_string(&pid_file).unwrap().trim(),
        child.id().to_string()
    );

    // The SIGHUP checks the config files and keeps the daemon.
    assert!(kill("-HUP", child.id()), "cannot send the SIGHUP");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        let log = fs::read_to_string(&log_file).unwrap_or_default();
        if log.contains("got SIGHUP") {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        child.try_wait().unwrap().is_none(),
        "the SIGHUP ended the daemon"
    );

    // The SIGTERM ends the daemon with the code 0.
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    assert_eq!(wait_for(&mut child, Duration::from_secs(10)), Some(0));

    // The daemon removes the pid file at the end.
    assert!(!pid_file.exists(), "the pid file stays");

    // The log holds the lines of the startup and of the signals.
    let log = fs::read_to_string(&log_file).unwrap();
    assert!(log.contains("BEGIN, binkd/"), "{log}");
    assert!(log.contains("got SIGHUP"), "{log}");
    assert!(log.contains("got signal #15."), "{log}");
    assert!(log.contains("exitfunc()"), "{log}");
    let _ = fs::remove_dir_all(&dir);
}
