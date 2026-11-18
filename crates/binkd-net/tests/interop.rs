//! The interop matrix of the network layer.
//!
//! The tests run the C binary and the caller and the listener of the port against
//! each other over the loopback address, the third exit criterion of the phase. They
//! need the C binary: set `BINKD_C` to its path. Without the variable the tests
//! report that they are skipped.
//!
//! The matrix of the plan:
//!
//! | Client | Server | State |
//! | --- | --- | --- |
//! | Rust | C | the caller of the port calls the C listener |
//! | C | Rust | the C caller calls the listener of the port |
//! | C | C | the C test suite of the C project |
//! | Rust | Rust | `tests/loopback.rs` |

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use binkd_config::model::{Config, ListenChain};
use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::{DomainTable, FtnDomain};
use binkd_ftn::node::NodeSpec;
use binkd_log::{DateTime, Log};
use binkd_net::{listen, Caller, SessionOptions};
use binkd_proto::{ReceivedFile, SendFile};

/// The number of the bytes of the packet of a test.
const PACKET_SIZE: usize = 5000;

/// The address of the caller of the tests.
const CLIENT_AKA: &str = "2:5047/13@fidonet";
/// The address of the server of the tests.
const SERVER_AKA: &str = "2:5047/100@fidonet";

/// The name of the file that the caller sends, the address of the server.
const CLIENT_FILE: &str = "13b70064.cut";

/// Returns the one file of an extension in an inbound directory.
///
/// The C code announces a file of the mail with a random name, the port of
/// `netname_()` of `prothlp.c`, so a test takes the file by its extension.
fn inbound_file(dir: &Path, ext: &str) -> PathBuf {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .map(|e| e.eq_ignore_ascii_case(ext))
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(found.len(), 1, "the inbound holds {found:?}");
    found.pop().unwrap()
}
/// The name of the file that the server sends, the address of the caller.
const SERVER_FILE: &str = "13b7000d.cut";

/// Returns the path of the C binary, or `None` when the variable is not set.
fn c_binary() -> Option<String> {
    std::env::var("BINKD_C")
        .ok()
        .filter(|p| Path::new(p).is_file())
}

/// Returns a port of the process.
///
/// The port is unique in the process: a port of a closed listener is free at the
/// moment, and another test of the suite takes it under the parallel load.
fn free_port() -> u16 {
    use std::sync::atomic::{AtomicU16, Ordering};

    static NEXT: AtomicU16 = AtomicU16::new(0);
    let base = 21000 + (std::process::id() % 20000) as u16;
    let offset = NEXT.fetch_add(1, Ordering::Relaxed);
    base.wrapping_add(offset % 30000).max(1024)
}

/// Makes a directory for the spool of a test.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("binkd-net-interop-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("outb")).unwrap();
    std::fs::create_dir_all(dir.join("in")).unwrap();
    dir
}

fn address(s: &str, domains: &DomainTable) -> FtnAddr {
    FtnAddr::parse(s, domains).unwrap()
}

fn time() -> DateTime {
    DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
    }
}

/// The settings of the port of a test.
fn config(dir: &Path, aka: &str) -> Config {
    let mut config = Config::new();
    config.spool.domains = DomainTable::new(vec![FtnDomain {
        name: "fidonet".to_string(),
        path: dir.display().to_string(),
        dir: "outb".to_string(),
        z: [2, -1],
        idomain: None,
        alias4: None,
    }]);
    config.spool.aka = vec![address(aka, &config.spool.domains)];
    config.inbound = dir.join("in").display().to_string();
    config.oport = "binkp".to_string();
    config
}

/// Writes the packet of a side and queues it for the node.
fn packet(config: &mut Config, name: &str, content: &[u8], now: i64) {
    let dir = PathBuf::from(&config.spool.domains.last().unwrap().path);
    std::fs::write(dir.join("outb").join(name), content).unwrap();
    let _ = binkd_spool::queue::Queue::scan(&mut config.spool, now);
}

/// Writes the config of the C side of a test.
///
/// The C server holds the address `2:5047/100@fidonet`, and the C client polls
/// that address, so the C server passes its own address and the C client passes
/// `2:5047/13@fidonet`, the address of the listener of the port.
fn c_config(dir: &Path, name: &str, own: &str, node: &str) -> String {
    let config = format!(
        "domain fidonet outb 2\n\
         address {own}\n\
         sysname \"C side\"\n\
         sysop \"C sysop\"\n\
         location \"C location\"\n\
         nodeinfo \"C node\"\n\
         inbound in\n\
         log {name}.log\n\
         loglevel 10\n\
         conlog 0\n\
         node {node}\n\
         zallow *\n"
    );
    std::fs::write(dir.join("binkd.conf"), &config).unwrap();
    config
}

/// Starts the C server of a spool, the port of `binkd -s`.
fn start_c_server(dir: &Path, port: u16, peer: &str) -> Child {
    c_config(
        dir,
        "c-server",
        SERVER_AKA,
        &format!("{peer} 127.0.0.1 secret"),
    );
    // The C server holds the listen socket of the port.
    let mut config = std::fs::read_to_string(dir.join("binkd.conf")).unwrap();
    config.push_str(&format!("iport {port}\nlisten 127.0.0.1:{port}\n"));
    std::fs::write(dir.join("binkd.conf"), config).unwrap();
    Command::new(c_binary().unwrap())
        .arg("-s")
        .arg("binkd.conf")
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("cannot start the C server")
}

/// Runs the C client once, the port of `binkd -P <node> -p`.
fn run_c_client(dir: &Path, node: &str) {
    let client = Command::new(c_binary().unwrap())
        .args(["-P", node, "-p", "binkd.conf"])
        .current_dir(dir)
        .status()
        .expect("cannot run the C client");
    assert!(client.success(), "the C client failed");
}

/// Waits until a listener answers on the port.
async fn wait_for_port(port: u16) -> bool {
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// The caller of the port calls the C listener.
#[tokio::test]
async fn rust_caller_to_c_listener() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let server_dir = temp_dir("rust-caller-server");
    let client_dir = temp_dir("rust-caller-client");
    let port = free_port();
    // The C server holds the node of the caller and the packet of its outbound for
    // the caller, the address `2:5047/13`.
    let mut server = start_c_server(&server_dir, port, CLIENT_AKA);
    assert!(wait_for_port(port).await, "the C server did not start");
    let server_packet = vec![b's'; PACKET_SIZE];
    std::fs::write(server_dir.join("outb").join(SERVER_FILE), &server_packet).unwrap();

    // The port calls the node of the C server.
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = time;
    let mut client_config = config(&client_dir, CLIENT_AKA);
    let server_aka = address(SERVER_AKA, &client_config.spool.domains);
    client_config.spool.nodes.add(
        &server_aka,
        &NodeSpec {
            hosts: Some(format!("127.0.0.1:{port}")),
            pwd: Some("secret".to_string()),
            ..NodeSpec::default()
        },
        now().epoch,
    );
    let client_packet = vec![b'c'; PACKET_SIZE];
    packet(&mut client_config, CLIENT_FILE, &client_packet, now().epoch);
    let client_path = client_dir.join("outb").join(CLIENT_FILE);

    let files = |_peer: &FtnAddr| {
        vec![SendFile {
            netname: CLIENT_FILE.to_string(),
            path: Some(client_path.clone()),
            size: client_packet.len() as u64,
            time: 1_000_000_000,
        }]
    };
    let mut caller = Caller::new();
    let result = caller
        .call_next(
            &mut client_config,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(20), true, true),
            &files,
        )
        .await;

    let report = result
        .expect("the caller found no node")
        .expect("the session failed");
    // Give the C server the moment to write the packet.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = server.kill();
    let _ = server.wait();

    // The frame log of the session holds the file frame of each direction, the
    // port of the frame log of the C code at the loglevel 10.
    let has = |sent: bool, name: &str| {
        report
            .frames
            .iter()
            .any(|frame| frame.sent == sent && frame.name == name)
    };
    assert!(has(true, "FILE"), "{:?}", report.frames);
    assert!(has(false, "FILE"), "{:?}", report.frames);
    // The packet of 5000 bytes travels in the blocks of the 4096 byte setting.
    assert!(has(true, "DATA 4096"), "{:?}", report.frames);
    assert!(has(true, "DATA 904"), "{:?}", report.frames);

    // The session of the port sent the packet of its outbound and received the
    // packet of the C server.
    assert_eq!(report.files_sent, 1, "{report:?}");
    assert_eq!(report.files_received, 1, "{report:?}");
    assert_eq!(report.peer_addresses[0].format(false), SERVER_AKA);
    assert_eq!(
        std::fs::read(&report.received[0].path).unwrap(),
        server_packet
    );
    // The C server wrote the packet of the port.
    assert_eq!(
        std::fs::read(inbound_file(&server_dir.join("in"), "pkt")).unwrap(),
        client_packet
    );
}

/// The C caller calls the listener of the port.
#[tokio::test]
async fn c_caller_to_rust_listener() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let server_dir = temp_dir("c-caller-server");
    let client_dir = temp_dir("c-caller-client");
    let port = free_port();
    // The listener of the port holds the node of the C caller and the packet of its
    // outbound for the caller, the address `2:5047/100`.
    let mut server_config = config(&server_dir, SERVER_AKA);
    server_config.listen = vec![ListenChain {
        addr: "127.0.0.1".to_string(),
        port: port.to_string(),
    }];
    let client_aka = address(CLIENT_AKA, &server_config.spool.domains);
    server_config.spool.nodes.add(
        &client_aka,
        &NodeSpec {
            hosts: Some("127.0.0.1".to_string()),
            pwd: Some("secret".to_string()),
            ..NodeSpec::default()
        },
        time().epoch,
    );
    let server_packet = vec![b's'; PACKET_SIZE];
    let server_path = server_dir.join("outb").join(SERVER_FILE);
    let server_size = server_packet.len() as u64;
    std::fs::write(&server_path, &server_packet).unwrap();

    // The C client holds the node of the listener and the packet of its outbound
    // for the listener.
    let client_packet = vec![b'c'; PACKET_SIZE];
    c_config(
        &client_dir,
        "c-client",
        CLIENT_AKA,
        &format!("{SERVER_AKA} 127.0.0.1:{port} secret"),
    );
    std::fs::write(client_dir.join("outb").join(CLIENT_FILE), &client_packet).unwrap();

    // The listener of the port runs until the session ends.
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = time;
    let received = Arc::new(Mutex::new(Vec::<ReceivedFile>::new()));
    let server_received = Arc::clone(&received);
    let files = move |_peer: &[FtnAddr]| {
        vec![SendFile {
            netname: SERVER_FILE.to_string(),
            path: Some(server_path.clone()),
            size: server_size,
            time: 1_000_000_000,
        }]
    };
    let receive = move |file: &ReceivedFile| {
        if let Ok(mut received) = server_received.lock() {
            received.push(file.clone());
        }
        Ok(())
    };
    let server = tokio::spawn(listen(
        server_config,
        log,
        now,
        SessionOptions::new(Duration::from_secs(20), true, true),
        files,
        receive,
    ));
    // The probe of the port is a session that fails, and it proves that the
    // listener answers on the port.
    assert!(wait_for_port(port).await, "the listener did not start");

    // The C client polls the node and exits.
    let c_client_dir = client_dir.clone();
    let c_client = tokio::task::spawn_blocking(move || run_c_client(&c_client_dir, SERVER_AKA));
    c_client.await.unwrap();
    // Give the session the moment to finish.
    tokio::time::sleep(Duration::from_millis(500)).await;
    server.abort();

    // The session of the port received the packet of the C caller and sent the
    // packet of its outbound.
    let received = received.lock().unwrap();
    assert_eq!(received.len(), 1, "{received:?}");
    assert_eq!(received[0].size, PACKET_SIZE as u64);
    assert_eq!(std::fs::read(&received[0].path).unwrap(), client_packet);
    // The C client wrote the packet of the port.
    assert_eq!(
        std::fs::read(inbound_file(&client_dir.join("in"), "pkt")).unwrap(),
        server_packet
    );
    // The log of the C client at the loglevel 10 holds the file frame of each
    // direction, the oracle of the frame log of the session of the port.
    // The MinGW C log uses `\` separators; compare as `/`.
    let log = std::fs::read_to_string(client_dir.join("c-client.log"))
        .unwrap_or_default()
        .replace('\\', "/");
    assert!(log.contains("sending ./outb/13b70064.cut as "), "{log}");
    assert!(log.contains("rcvd msg FILE "), "{log}");
    assert!(log.contains(".pkt 5000"), "{log}");
}
