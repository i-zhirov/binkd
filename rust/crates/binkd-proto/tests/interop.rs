//! The C and Rust interop matrix.
//!
//! The tests run the C binary and the Rust session against each other over the
//! loopback address. They need the C binary: set `BINKD_C` to its path. Without the
//! variable the tests report that they are skipped.
//!
//! Two tests need a build of the C binary of another shape: the test of the rate
//! limit needs `BINKD_C_BWLIM`, the build of the `--with-bwlim` switch of
//! `configure`, and the test of the FREQ claim needs `BINKD_C_V10`, the build of
//! the version 1.0 of the protocol, the port of the `PRTCLVER` define of
//! `Config.h`. The tests report that they are skipped without the variables.
//!
//! The matrix of the plan:
//!
//! | Client | Server | State |
//! | --- | --- | --- |
//! | Rust | C | plain and bzip2 compression |
//! | C | Rust | plain, zlib compression, and bzip2 compression |
//! | C | C | the C test suite of the C project |
//! | Rust | Rust | the in-memory tests of `tests/session.rs` |

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use binkd_ftn::addr::FtnAddr;
use binkd_proto::{
    run, run_with, Method, Methods, Peer, Protocol, Report, Role, SendFile, SessionConfig, Srif,
    ZRule,
};
use tokio::net::TcpStream;

/// The number of the bytes of a file that the tests send.
const FILE_SIZE: usize = 5000;

/// Returns the path of the C binary, or `None` when the variable is not set.
fn c_binary() -> Option<String> {
    std::env::var("BINKD_C")
        .ok()
        .filter(|p| Path::new(p).is_file())
}

/// Returns the path of the C binary of the rate limit, or `None`.
///
/// The build of the binary needs the `--with-bwlim` switch of `configure`: the
/// plain build holds no `-bw` option and no `limit-rate` keyword.
fn c_binary_bwlim() -> Option<String> {
    std::env::var("BINKD_C_BWLIM")
        .ok()
        .filter(|p| Path::new(p).is_file())
}

/// Returns the path of the C binary of the protocol version 1.0, or `None`.
///
/// The C code takes the version that it advertises from `PRTCLVER` of `Config.h`,
/// and it answers the version of the peer from its `VER` frame: the C code holds
/// its own end of a batch back, and it announces `M_NUL FREQ`, only for a peer of
/// the version 1.0, the port of the send path and of `do_prescan()` of
/// `protocol.c`. The build of the binary of the version 1.0 needs the define of
/// the makefile:
///
/// `make CFLAGS='-O2 -DPRTCLVER=\"1.0\"'`
fn c_binary_v10() -> Option<String> {
    std::env::var("BINKD_C_V10")
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

/// Starts the C server of a spool.
///
/// The extra lines hold the compression rules of the config.
fn start_c_server(
    dir: &Path,
    port: u16,
    peer: &str,
    password: &str,
    extra: &str,
    flags: &str,
) -> Child {
    let config = format!(
        "domain fidonet outb 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"C server\"\n\
         sysop \"C sysop\"\n\
         location \"C location\"\n\
         nodeinfo \"C node\"\n\
         inbound in\n\
         iport {port}\n\
         listen 127.0.0.1:{port}\n\
         log c-server.log\n\
         loglevel 10\n\
         conlog 0\n\
         node {peer} 127.0.0.1 {password} {flags}\n\
         {extra}"
    );
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

/// Waits until the C server listens on the port.
async fn wait_for_port(port: u16) -> bool {
    for _ in 0..50 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Makes a directory for the spool of a test.
/// The settings of the spool of a test.
fn spool(dir: &Path) -> binkd_spool::config::SpoolConfig {
    // The inbound directory of the partial files.
    std::fs::create_dir_all(dir.join("in")).unwrap();
    let mut spool = binkd_spool::config::SpoolConfig::new();
    // The free-space values of the C defaults, `init_config()`.
    spool.minfree = -1;
    spool.minfree_nonsecure = -1;
    spool.inbound_nonsecure = dir.join("in").display().to_string();
    spool
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("binkd-interop-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("outb")).unwrap();
    std::fs::create_dir_all(dir.join("in")).unwrap();
    dir
}

fn address(s: &str) -> FtnAddr {
    let table = binkd_ftn::domain::DomainTable::new(Vec::new());
    FtnAddr::parse(s, &table).unwrap()
}

/// The settings of the Rust side of a test.
fn session_config(
    dir: &Path,
    aka: &str,
    password: &str,
    methods: Methods,
    crypt: bool,
) -> SessionConfig {
    SessionConfig {
        sysname: "Rust test".to_string(),
        sysop: "Rust sysop".to_string(),
        location: "Rust location".to_string(),
        nodeinfo: "Rust node".to_string(),
        addresses: vec![address(aka)],
        password: password.to_string(),
        inbound: dir.join("in"),
        spool: spool(dir),
        version: "rust-test/0.1".to_string(),
        blksize: 4096,
        zminsize: 1024,
        zlevel: 0,
        zrules: vec![ZRule {
            mask: "*".to_string(),
            allow: true,
        }],
        methods,
        crypt,
        nr: false,
        nd: false,
        domains: binkd_ftn::domain::DomainTable::new(Vec::new()),
        max_servers: 100,
        servers: 1,
        trace: false,
        rates: Vec::new(),
        protocol: Protocol::default(),
        bw_send: -100,
        bw_recv: -100,
        skip: Vec::new(),
        listed: false,
        prescan: false,
    }
}

/// The case of the plan: the Rust client sends a file to the C server.
#[tokio::test]
async fn rust_client_to_c_server() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let dir = temp_dir("rust-client");
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let packet = out.join("13b7000d.pkt");
    let content = vec![b'p'; 512];
    std::fs::write(&packet, &content).unwrap();

    let port = free_port();
    let mut server = start_c_server(&dir, port, "2:5047/13@fidonet", "secret", "", "");
    assert!(wait_for_port(port).await, "the C server did not start");

    let config = session_config(&dir, "2:5047/13@fidonet", "secret", Methods::none(), false);
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(packet),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];

    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let result = run(
        stream,
        Role::Client,
        &config,
        &files,
        &mut |_| Ok(()),
        Duration::from_secs(20),
    )
    .await;

    // Give the C server the moment to write the file.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = server.kill();
    let _ = server.wait();

    let report: Report = result.expect("the Rust session failed");
    assert_eq!(report.files_sent, 1);
    assert_eq!(report.peer_addresses[0].format(false), "2:5047/100@fidonet");

    // The C server wrote the file into its inbound directory.
    let received = dir.join("in/13b7000d.pkt");
    assert!(received.is_file(), "the C server did not write the file");
    assert_eq!(std::fs::read(&received).unwrap(), content);
}

/// The Rust client compresses a file for the C server.
///
/// The C server holds zlib and bzip2 and advertises both. The Rust client
/// prefers bzip2, as the C code does. A Rust client that holds zlib alone
/// picks zlib.
async fn rust_client_sends_compressed(name: &str, methods: Methods, log: &str) {
    let dir = temp_dir(name);
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let packet = out.join("13b7000d.pkt");
    let content = vec![b'p'; FILE_SIZE];
    std::fs::write(&packet, &content).unwrap();

    let port = free_port();
    // The C code denies a file without a matching rule.
    let mut server = start_c_server(&dir, port, "2:5047/13@fidonet", "secret", "zallow *", "");
    assert!(wait_for_port(port).await, "the C server did not start");

    let config = session_config(&dir, "2:5047/13@fidonet", "secret", methods, false);
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(packet),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];

    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let result = run(
        stream,
        Role::Client,
        &config,
        &files,
        &mut |_| Ok(()),
        Duration::from_secs(20),
    )
    .await;

    // Give the C server the moment to write the file.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = server.kill();
    let _ = server.wait();

    let report: Report = result.expect("the Rust session failed");
    assert_eq!(report.files_sent, 1);

    // The C server decompressed the file.
    let received = dir.join("in/13b7000d.pkt");
    assert!(received.is_file(), "the C server did not write the file");
    assert_eq!(std::fs::read(&received).unwrap(), content);

    // The C server logged the mode of the file.
    let server_log = std::fs::read_to_string(dir.join("c-server.log")).unwrap_or_default();
    assert!(server_log.contains(log), "{server_log}");
}

/// The Rust client compresses a file with bzip2 for the C server.
#[tokio::test]
async fn rust_client_to_c_server_bz2() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    rust_client_sends_compressed("rust-client-bz2", Methods::all(), "bzip2 mode is on").await;
}

/// The Rust client compresses a file with zlib for the C server.
///
/// The Rust client holds zlib alone, so it picks zlib from the two methods of
/// the C server.
#[tokio::test]
async fn rust_client_to_c_server_gz() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    rust_client_sends_compressed("rust-client-gz", Methods::of(Method::Gz), "gzip mode is on")
        .await;
}

/// The C client calls the Rust server.
///
/// The Rust server advertises the methods of the test. The C client polls the
/// node, sends the file of its outbound, and exits.
async fn c_client_polls_rust_server(name: &str, methods: Methods, crypt: bool) -> (Report, String) {
    let dir = temp_dir(name);
    let port = free_port();
    let config = format!(
        "domain fidonet outb 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"C client\"\n\
         sysop \"C sysop\"\n\
         location \"C location\"\n\
         nodeinfo \"C node\"\n\
         inbound in\n\
         log c-client.log\n\
         loglevel 10\n\
         conlog 0\n\
         node 2:5047/13@fidonet 127.0.0.1:{port} secret\n\
         zallow *\n"
    );
    std::fs::write(dir.join("binkd.conf"), config).unwrap();

    // The C client sends the file of its outbound for the node.
    let content = vec![b'c'; FILE_SIZE];
    std::fs::write(dir.join("outb/13b7000d.cut"), &content).unwrap();

    // The Rust server accepts one session.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    let server_config = session_config(&dir, "2:5047/13@fidonet", "secret", methods, crypt);
    let server = async move {
        let (stream, _) = listener.accept().await.unwrap();
        run(
            stream,
            Role::Server,
            &server_config,
            &[],
            &mut |_| Ok(()),
            Duration::from_secs(20),
        )
        .await
    };

    // The C client creates the poll and runs it, then exits.
    let c_client_dir = dir.clone();
    let c_client = tokio::task::spawn_blocking(move || {
        let client = Command::new(c_binary().unwrap())
            .args(["-P", "2:5047/13@fidonet", "-p", "binkd.conf"])
            .current_dir(&c_client_dir)
            .status()
            .expect("cannot run the C client");
        assert!(client.success(), "the C client failed");
    });

    let (server, c_client) = tokio::join!(server, c_client);
    c_client.unwrap();
    let report = server.expect("the Rust session failed");
    assert_eq!(report.peer_addresses[0].format(false), "2:5047/100@fidonet");

    // The Rust server received the file of the C client.
    assert_eq!(report.files_received, 1, "{report:?}");
    assert_eq!(report.received[0].size, FILE_SIZE as u64);
    assert_eq!(std::fs::read(&report.received[0].path).unwrap(), content);
    // The log of the C client holds the mode of the session.
    let log = std::fs::read_to_string(dir.join("c-client.log")).unwrap_or_default();
    (report, log)
}

/// The C client compresses a file with zlib for the Rust server.
///
/// The Rust server advertises zlib only, so the C client picks zlib.
#[tokio::test]
async fn c_client_to_rust_server_gz() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let (report, _) =
        c_client_polls_rust_server("c-client-gz", Methods::of(Method::Gz), false).await;
    assert_eq!(report.received[0].method, Some(Method::Gz));
}

/// The C client compresses a file with bzip2 for the Rust server.
///
/// The Rust server advertises both, so the C client picks bzip2.
#[tokio::test]
async fn c_client_to_rust_server_bz2() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let (report, _) = c_client_polls_rust_server("c-client-bz2", Methods::all(), false).await;
    assert_eq!(report.received[0].method, Some(Method::Bz2));
}

/// The case of the plan without compression: the C client calls the Rust server.
#[tokio::test]
async fn c_client_to_rust_server() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let (report, _) = c_client_polls_rust_server("c-client", Methods::none(), false).await;
    assert_eq!(report.received[0].method, None);
}

/// The Rust client crypts the traffic for the C server.
///
/// The C server sends the challenge of CRAM-MD5, the Rust client answers with
/// the digest, and both sides crypt the session.
#[tokio::test]
async fn rust_client_to_c_server_crypt() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let dir = temp_dir("rust-client-crypt");
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let packet = out.join("13b7000d.pkt");
    let content = vec![b'p'; FILE_SIZE];
    std::fs::write(&packet, &content).unwrap();

    let port = free_port();
    let mut server = start_c_server(&dir, port, "2:5047/13@fidonet", "secret", "zallow *", "");
    assert!(wait_for_port(port).await, "the C server did not start");

    let config = session_config(&dir, "2:5047/13@fidonet", "secret", Methods::all(), true);
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(packet),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];

    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let result = run(
        stream,
        Role::Client,
        &config,
        &files,
        &mut |_| Ok(()),
        Duration::from_secs(20),
    )
    .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = server.kill();
    let _ = server.wait();

    let report: Report = result.expect("the Rust session failed");
    assert!(report.crypt, "the Rust client did not crypt");

    // The C server decrypted the traffic and wrote the file.
    let received = dir.join("in/13b7000d.pkt");
    assert!(received.is_file(), "the C server did not write the file");
    assert_eq!(std::fs::read(&received).unwrap(), content);

    // The C server logged the mode of the session.
    let log = std::fs::read_to_string(dir.join("c-server.log")).unwrap_or_default();
    assert!(log.contains("session in CRYPT mode"), "{log}");
}

/// The C client crypts the traffic for the Rust server.
///
/// The Rust server sends the challenge of CRAM-MD5, the C client answers with
/// the digest, and both sides crypt the session.
#[tokio::test]
async fn c_client_to_rust_server_crypt() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let (report, log) = c_client_polls_rust_server("c-client-crypt", Methods::none(), true).await;
    assert!(report.crypt, "the Rust server did not crypt");
    // The C client logged the mode of the session.
    assert!(log.contains("session in CRYPT mode"), "{log}");
}

/// The C server holds the NR mode: the Rust client announces the file with the
/// offset -1, and the C server asks for it with `M_GET`.
#[tokio::test]
async fn rust_client_to_c_server_nr() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let dir = temp_dir("rust-client-nr");
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let packet = out.join("13b7000d.pkt");
    let content = vec![b'n'; FILE_SIZE];
    std::fs::write(&packet, &content).unwrap();

    let port = free_port();
    // The `-nr` option of the node asks for the NR mode.
    let mut server = start_c_server(&dir, port, "2:5047/13@fidonet", "secret", "zallow *", "-nr");
    assert!(wait_for_port(port).await, "the C server did not start");

    let mut config = session_config(&dir, "2:5047/13@fidonet", "secret", Methods::all(), false);
    config.nr = true;
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(packet),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];

    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let result = run(
        stream,
        Role::Client,
        &config,
        &files,
        &mut |_| Ok(()),
        Duration::from_secs(20),
    )
    .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = server.kill();
    let _ = server.wait();

    let report: Report = result.expect("the Rust session failed");
    assert_eq!(report.files_sent, 1);
    let received = dir.join("in/13b7000d.pkt");
    assert!(received.is_file(), "the C server did not write the file");
    assert_eq!(std::fs::read(&received).unwrap(), content);
    // The C server saw the NR announcement of the file.
    let log = std::fs::read_to_string(dir.join("c-server.log")).unwrap_or_default();
    assert!(log.contains("remote is in NR mode"), "{log}");
}

/// The C server holds the ND mode: the Rust client asks for the ND mode, and
/// both sides wait for the acknowledgement of a file.
#[tokio::test]
async fn rust_client_to_c_server_nd() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let dir = temp_dir("rust-client-nd");
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let packet = out.join("13b7000d.pkt");
    let content = vec![b'd'; FILE_SIZE];
    std::fs::write(&packet, &content).unwrap();

    let port = free_port();
    // The `-nd` option of the node asks for the ND mode and implies `-nr`.
    let mut server = start_c_server(&dir, port, "2:5047/13@fidonet", "secret", "zallow *", "-nd");
    assert!(wait_for_port(port).await, "the C server did not start");

    let mut config = session_config(&dir, "2:5047/13@fidonet", "secret", Methods::all(), false);
    config.nr = true;
    config.nd = true;
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(packet),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];

    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let result = run(
        stream,
        Role::Client,
        &config,
        &files,
        &mut |_| Ok(()),
        Duration::from_secs(20),
    )
    .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = server.kill();
    let _ = server.wait();

    let report: Report = result.expect("the Rust session failed");
    assert_eq!(report.files_sent, 1);
    let received = dir.join("in/13b7000d.pkt");
    assert!(released(&received), "the C server did not write the file");
    assert_eq!(std::fs::read(&received).unwrap(), content);
    // The C server saw the ND mode of both sides.
    let log = std::fs::read_to_string(dir.join("c-server.log")).unwrap_or_default();
    assert!(log.contains("we are in ND mode"), "{log}");
}

/// Reports whether a path is a file.
fn released(path: &Path) -> bool {
    path.is_file()
}

/// Kills the C server when the value goes out of scope.
struct ServerGuard(Child);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Returns the process id of a line of the log of the C binary.
fn log_pid(line: &str) -> Option<&str> {
    let start = line.find('[')? + 1;
    let end = line[start..].find(']')? + start;
    Some(&line[start..end])
}

/// Returns the frames that the C binary sent, in the order of its log.
///
/// The server of the C code forks a process for the session after it queues the
/// banner, so the parent and the child both hold the queue and both write the
/// frames of the banner to the log. The parser keeps the frames of the process
/// that reads the session, the port of `recv_block()`.
fn c_frames_sent(log: &str) -> Vec<String> {
    let session = log
        .lines()
        .find(|line| line.contains("recvd hdr:"))
        .and_then(log_pid);
    let mut out = Vec::new();
    for line in log.lines() {
        if session.is_some() && log_pid(line) != session {
            continue;
        }
        if let Some(rest) = line.split("send message ").nth(1) {
            let mut words = rest.split_whitespace();
            let name = words.next().unwrap_or_default();
            let kind = words.next().unwrap_or_default();
            if name == "NUL" {
                if matches!(kind, "SYS" | "ZYZ" | "LOC" | "NDL" | "VER" | "OPT") {
                    out.push(format!("NUL {kind}"));
                }
            } else if !name.is_empty() {
                out.push(name.to_string());
            }
        } else if let Some(rest) = line.split("next block to send: ").nth(1) {
            let size = rest.split_whitespace().next().unwrap_or_default();
            out.push(format!("DATA {size}"));
        }
    }
    out
}

/// Returns the frames that the C binary received, in the order of its log.
fn c_frames_received(log: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in log.lines() {
        if let Some(rest) = line.split("rcvd msg ").nth(1) {
            let mut words = rest.split_whitespace();
            let name = words.next().unwrap_or_default();
            let kind = words.next().unwrap_or_default();
            if name == "NUL" {
                if matches!(kind, "SYS" | "ZYZ" | "LOC" | "NDL" | "VER" | "OPT") {
                    out.push(format!("NUL {kind}"));
                }
            } else if !name.is_empty() {
                out.push(name.to_string());
            }
        } else if let Some(rest) = line.split("recvd hdr: ").nth(1) {
            if rest.contains("(data)") {
                let size = rest.split_whitespace().next().unwrap_or_default();
                out.push(format!("DATA {size}"));
            }
        }
    }
    out
}

/// The frame sequence that the port sends matches the frame sequence that the C
/// binary receives, and the other way around, so a divergence of the flow of the
/// port fails the suite.
///
/// The C binary logs its frames at `loglevel 10`, and the port records the bytes
/// of its transport. The parser keeps the frames of the process of the session,
/// because the server of the C code forks and both processes write the banner.
async fn frames_match(
    name: &str,
    methods: Methods,
    crypt: bool,
    nr: bool,
    nd: bool,
    extra: &str,
    flags: &str,
) {
    // The C binary closes the transport at the end of the session, and the
    // socket of the peer may answer with a reset before the frames of the last
    // batch are read. The scenario runs again for that race.
    for attempt in 0..3 {
        match frames_match_once(name, methods, crypt, nr, nd, extra, flags).await {
            Ok(()) => return,
            Err(e) if attempt == 2 => panic!("the frames do not match: {e}"),
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
}

/// Runs the scenario of the frames once.
async fn frames_match_once(
    name: &str,
    methods: Methods,
    crypt: bool,
    nr: bool,
    nd: bool,
    extra: &str,
    flags: &str,
) -> Result<(), String> {
    let dir = temp_dir(name);
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let packet = out.join("13b7000d.pkt");
    let content = vec![b'f'; 512];
    std::fs::write(&packet, &content).unwrap();

    let port = free_port();
    let server = ServerGuard(start_c_server(
        &dir,
        port,
        "2:5047/13@fidonet",
        "secret",
        extra,
        flags,
    ));
    assert!(wait_for_port(port).await, "the C server did not start");

    let mut config = session_config(&dir, "2:5047/13@fidonet", "secret", methods, crypt);
    config.nr = nr;
    config.nd = nd;
    // The port logs the frames of the session, before the encryption.
    config.trace = true;
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(packet),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];

    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let result = run(
        stream,
        Role::Client,
        &config,
        &files,
        &mut |_| Ok(()),
        Duration::from_secs(60),
    )
    .await;

    let report = result.map_err(|e| format!("the Rust session failed: {e:?}"))?;
    // The C binary writes its log after the session and from two processes, so
    // wait until the log stops growing before the frames are compared.
    let mut log = String::new();
    let mut settled = 0;
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let next = std::fs::read_to_string(dir.join("c-server.log")).unwrap_or_default();
        if next == log
            && c_frames_received(&next).last().is_some()
            && c_frames_sent(&next).last().is_some()
        {
            settled += 1;
            if settled == 3 {
                break;
            }
        } else {
            settled = 0;
        }
        log = next;
    }
    drop(server);

    let port_sent: Vec<String> = report
        .frames
        .iter()
        .filter(|frame| frame.sent)
        .map(|frame| frame.name.clone())
        .collect();
    let port_received: Vec<String> = report
        .frames
        .iter()
        .filter(|frame| !frame.sent)
        .map(|frame| frame.name.clone())
        .collect();

    // The frames that the port sent are the frames that the C server received.
    compare(
        &port_sent,
        &c_frames_received(&log),
        "the C server received",
    )?;
    // The frames that the C server sent are the frames that the port received.
    compare(&port_received, &c_frames_sent(&log), "the C server sent")?;
    Ok(())
}

/// Compares two sequences of frames and reports the first difference.
fn compare(ours: &[String], theirs: &[String], what: &str) -> Result<(), String> {
    for (i, (a, b)) in ours.iter().zip(theirs.iter()).enumerate() {
        if a != b {
            return Err(format!(
                "the frames of {what} differ at {i}: the port {a:?}, the C binary {b:?}"
            ));
        }
    }
    if ours.len() != theirs.len() {
        return Err(format!(
            "the count of the frames of {what} differs: the port {}, the C binary {}",
            ours.len(),
            theirs.len()
        ));
    }
    Ok(())
}

/// The frames of the plain session match.
#[tokio::test]
async fn frame_sequences_match() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };
    let _ = frames_match(
        "frames",
        Methods::all(),
        false,
        false,
        false,
        "zallow *",
        "",
    )
    .await;
}

/// The frames of a session with zlib compression match.
#[tokio::test]
async fn frame_sequences_match_gz() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };
    let _ = frames_match(
        "frames-gz",
        Methods::of(Method::Gz),
        false,
        false,
        false,
        "zallow *",
        "",
    )
    .await;
}

/// The frames of a session with bzip2 compression match.
#[tokio::test]
async fn frame_sequences_match_bz2() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };
    let _ = frames_match(
        "frames-bz2",
        Methods::all(),
        false,
        false,
        false,
        "zallow *",
        "",
    )
    .await;
}

/// The frames of a session with the crypt mode match.
#[tokio::test]
async fn frame_sequences_match_crypt() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };
    let _ = frames_match(
        "frames-crypt",
        Methods::all(),
        true,
        false,
        false,
        "zallow *",
        "",
    )
    .await;
}

/// The frames of a session with the NR mode match.
#[tokio::test]
async fn frame_sequences_match_nr() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };
    let _ = frames_match(
        "frames-nr",
        Methods::all(),
        false,
        true,
        false,
        "zallow *",
        "-nr",
    )
    .await;
}

/// The frames of a session with the ND mode match.
#[tokio::test]
async fn frame_sequences_match_nd() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };
    let _ = frames_match(
        "frames-nd",
        Methods::all(),
        false,
        true,
        true,
        "zallow *",
        "-nd",
    )
    .await;
}

/// A perturbed frame of the crypted session fails the diff, so the test of the
/// frames catches a divergence and does not pass on two equal sequences.
#[tokio::test]
async fn frame_sequences_match_catches_a_perturbation() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };
    let dir = temp_dir("frames-perturbed");
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let packet = out.join("13b7000d.pkt");
    std::fs::write(&packet, vec![b'f'; 512]).unwrap();
    let port = free_port();
    let server = ServerGuard(start_c_server(
        &dir,
        port,
        "2:5047/13@fidonet",
        "secret",
        "zallow *",
        "",
    ));
    assert!(wait_for_port(port).await, "the C server did not start");
    let mut config = session_config(&dir, "2:5047/13@fidonet", "secret", Methods::all(), true);
    config.trace = true;
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(packet),
        size: 512,
        time: 1_000_000_000,
    }];
    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let report = run(
        stream,
        Role::Client,
        &config,
        &files,
        &mut |_| Ok(()),
        Duration::from_secs(60),
    )
    .await
    .expect("the Rust session failed");
    tokio::time::sleep(Duration::from_millis(500)).await;
    drop(server);
    let log = std::fs::read_to_string(dir.join("c-server.log")).unwrap_or_default();
    let port_sent: Vec<String> = report
        .frames
        .iter()
        .filter(|frame| frame.sent)
        .map(|frame| frame.name.clone())
        .collect();
    let port_received: Vec<String> = report
        .frames
        .iter()
        .filter(|frame| !frame.sent)
        .map(|frame| frame.name.clone())
        .collect();
    let c_sent = c_frames_sent(&log);
    let c_received = c_frames_received(&log);

    // The sequences match before the perturbation.
    compare(&port_sent, &c_received, "the C server received").unwrap();
    compare(&port_received, &c_sent, "the C server sent").unwrap();

    // One frame of each direction of the port is perturbed, and the diff of
    // that direction must fail.
    assert!(
        perturbed_diff_caught(&port_sent, &c_received),
        "the diff did not catch a perturbed sent frame"
    );
    assert!(
        perturbed_diff_caught(&port_received, &c_sent),
        "the diff did not catch a perturbed received frame"
    );
}

/// Perturbs one frame of a sequence and reports whether the diff catches it.
fn perturbed_diff_caught(frames: &[String], other: &[String]) -> bool {
    let mut perturbed = frames.to_vec();
    let index = perturbed.len() / 2;
    let original = perturbed[index].clone();
    perturbed[index] = if original == "OK" {
        "EOB".to_string()
    } else {
        "OK".to_string()
    };
    assert_ne!(perturbed, frames);
    compare(&perturbed, other, "the perturbed sequence").is_err()
}

/// The Rust client resumes a partial file that the C server holds: the C server
/// asks for the file from the position of the partial, and the Rust client sends
/// the rest of the file.
///
/// The other direction is not part of the matrix: the C client names a mail
/// packet with a random name, so a test cannot hold the partial that the Rust
/// server would find. The receiver of the port is covered by the test
/// `session_with_a_partial_file` of the session suite.
#[tokio::test]
async fn rust_client_to_c_server_resumed() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let dir = temp_dir("rust-client-resumed");
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let content: Vec<u8> = (0..FILE_SIZE as u32).map(|i| (i % 251) as u8).collect();
    let packet = out.join("13b7000d.pkt");
    std::fs::write(&packet, &content).unwrap();

    // The C server holds the first part of the file already: the partial file
    // of the inbound directory, as `inb_fopen()` writes it.
    let partial = dir.join("in/000043f8");
    std::fs::write(
        partial.with_extension("hr"),
        "13b7000d.pkt 5000 1000000000 2:5047/13@fidonet\n",
    )
    .unwrap();
    std::fs::write(partial.with_extension("dt"), &content[..100]).unwrap();

    let port = free_port();
    let mut server = start_c_server(&dir, port, "2:5047/13@fidonet", "secret", "zallow *", "");
    assert!(wait_for_port(port).await, "the C server did not start");

    let config = session_config(&dir, "2:5047/13@fidonet", "secret", Methods::all(), false);
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(packet),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];

    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let result = run(
        stream,
        Role::Client,
        &config,
        &files,
        &mut |_| Ok(()),
        Duration::from_secs(20),
    )
    .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = server.kill();
    let _ = server.wait();
    result.expect("the Rust session failed");

    // The C server wrote the whole file, not only the rest.
    let received = dir.join("in/13b7000d.pkt");
    assert!(received.is_file(), "the C server did not write the file");
    assert_eq!(std::fs::read(&received).unwrap(), content);
    // The C server asked for the file from the position of the partial.
    let log = std::fs::read_to_string(dir.join("c-server.log")).unwrap_or_default();
    assert!(log.contains("13b7000d.pkt"), "{log}");
}

/// The rate limit of the C code holds the transfer of the port.
///
/// The C server holds the node of the port with `-bw 4096`, so the C code limits
/// the receiving of the file, and the port holds the node of the C server with the
/// same limit, so the port limits the sending. The file of 4096 bytes needs about
/// one second. The C code of the meter is the oracle of the port.
#[tokio::test]
async fn rust_client_to_c_server_rate_limited() {
    let Some(_) = c_binary_bwlim() else {
        eprintln!("skip: set BINKD_C_BWLIM to the path of the C binary of the rate limit");
        return;
    };

    let dir = temp_dir("rust-client-rate");
    let port = free_port();
    let mut server = start_c_server_bwlim(
        &dir,
        port,
        "2:5047/13@fidonet",
        "secret",
        "zallow *",
        "-bw 4096",
    );
    assert!(wait_for_port(port).await, "the C server did not start");

    let content = vec![b'p'; FILE_SIZE];
    let mut config = session_config(&dir, "2:5047/13@fidonet", "secret", Methods::none(), false);
    // The C code reads the `-bw` of the node of the peer, the port of `ADR()`.
    config.spool.nodes.add(
        &address("2:5047/100@fidonet"),
        &binkd_ftn::node::NodeSpec {
            bw_send: Some(4096),
            bw_recv: Some(4096),
            ..binkd_ftn::node::NodeSpec::default()
        },
        0,
    );
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let packet = out.join("13b7000d.pkt");
    std::fs::write(&packet, &content).unwrap();
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(packet),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];

    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let started = std::time::Instant::now();
    let result = run(
        stream,
        Role::Client,
        &config,
        &files,
        &mut |_| Ok(()),
        Duration::from_secs(20),
    )
    .await;
    let elapsed = started.elapsed();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = server.kill();
    let _ = server.wait();

    let report = result.expect("the Rust session failed");
    assert_eq!(report.files_sent, 1);
    // The C server wrote the file in full, and the limit of the two sides held
    // the transfer of 4096 bytes for about two seconds.
    assert_eq!(
        std::fs::read(dir.join("in").join("13b7000d.pkt")).unwrap(),
        content
    );
    assert!(elapsed.as_millis() >= 500, "{elapsed:?}");
}

/// Starts the C server of the rate limit of a spool.
fn start_c_server_bwlim(
    dir: &Path,
    port: u16,
    peer: &str,
    password: &str,
    extra: &str,
    flags: &str,
) -> Child {
    let config = format!(
        "domain fidonet outb 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"C server\"\n\
         sysop \"C sysop\"\n\
         location \"C location\"\n\
         nodeinfo \"C node\"\n\
         inbound in\n\
         iport {port}\n\
         listen 127.0.0.1:{port}\n\
         log c-server.log\n\
         loglevel 10\n\
         conlog 0\n\
         node {peer} 127.0.0.1 {password} {flags}\n\
         {extra}"
    );
    std::fs::write(dir.join("binkd.conf"), config).unwrap();
    Command::new(c_binary_bwlim().unwrap())
        .arg("-s")
        .arg("binkd.conf")
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("cannot start the C server")
}

/// The C client of the version 1.0 drives the delayed SRIF path with its claim.
///
/// The C code announces `M_NUL FREQ` for every request file of the queue when
/// the peer holds the protocol version 1.0, the port of `do_prescan()` of
/// `protocol.c`. The C code sends the claim only for a session of the version
/// 1.0, and it holds its own end of a batch back for such a session only, the
/// port of the send path of `protocol.c`, so the test runs the C client of the
/// version 1.0, the port of `PRTCLVER` of `Config.h`.
///
/// The C client polls the listener, sends the request file of its outbound, and
/// receives the file that the external program of the port answers, the port of an
/// external FREQ processor. The port holds its own end of the batch back until
/// the C client answers the claim with its own end, so the exchange of the
/// request file runs before the end of the batch of the port, the port of
/// `delay_EOB` of `protocol.c`.
#[tokio::test]
async fn c_client_claims_freq_to_rust_server() {
    let Some(_) = c_binary_v10() else {
        eprintln!("skip: set BINKD_C_V10 to the path of the C binary of the version 1.0");
        return;
    };

    let dir = temp_dir("c-client-srif");
    // The port holds a spool of its own: the C client and the port are two
    // nodes, and the scan of the port reads the outbound of the port only.
    let rust_dir = dir.join("rust");
    std::fs::create_dir_all(rust_dir.join("outb")).unwrap();
    std::fs::create_dir_all(rust_dir.join("in")).unwrap();
    let port = free_port();
    let config = format!(
        "domain fidonet outb 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"C client\"\n\
         sysop \"C sysop\"\n\
         location \"C location\"\n\
         nodeinfo \"C node\"\n\
         inbound in\n\
         log c-client.log\n\
         loglevel 10\n\
         conlog 0\n\
         node 2:5047/13@fidonet 127.0.0.1:{port} secret\n\
         prescan\n\
         zallow *\n"
    );
    std::fs::write(dir.join("binkd.conf"), config).unwrap();
    // The C client holds a request file for the node that it polls.
    std::fs::write(dir.join("outb/13b7000d.req"), vec![b'r'; 100]).unwrap();

    // The external program of the port answers the file of the exchange.
    let answer = rust_dir.join("outb/13b70064.pkt");
    let mut srif = Erp {
        answer: answer.clone(),
        requests: Arc::new(Mutex::new(Vec::new())),
        claims: 0,
    };

    // The listener of the port advertises the version 1.0, the port of the
    // answer of a peer of the C code, so the C client announces the claim
    // and holds its own end of the batch back, the port of `delay_EOB`.
    let mut server_config = session_config(
        &rust_dir,
        "2:5047/13@fidonet",
        "secret",
        Methods::none(),
        false,
    );
    server_config.protocol = Protocol::V10;
    // The port logs the frames of the session, before the encryption.
    server_config.trace = true;
    // The port scans its own outbound for the files of the C client and
    // announces the queue, the port of `do_prescan()` of `protocol.c`, so the
    // spool holds a request file for the address of the C client.
    let mut spool = spool(&rust_dir);
    spool.domains.push(binkd_ftn::domain::FtnDomain {
        name: "fidonet".to_string(),
        path: rust_dir.display().to_string(),
        dir: "outb".to_string(),
        z: [2, 0],
        idomain: None,
        alias4: None,
    });
    std::fs::write(rust_dir.join("outb/13b70064.req"), vec![b'q'; 100]).unwrap();
    server_config.spool = spool;
    server_config.prescan = true;
    // The spool of the answerer needs the node of the caller, the port of
    // `q_add_file()`.
    server_config.spool.nodes.add(
        &address("2:5047/100@fidonet"),
        &binkd_ftn::node::NodeSpec {
            hosts: Some("127.0.0.1".to_string()),
            pwd: Some("secret".to_string()),
            ..binkd_ftn::node::NodeSpec::default()
        },
        0,
    );
    let requests = Arc::clone(&srif.requests);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    let server = async move {
        let (stream, _) = listener.accept().await.unwrap();
        run_with(
            stream,
            Role::Server,
            &server_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut srif,
            Duration::from_secs(20),
        )
        .await
    };

    // The C client creates the poll and runs it, then exits.
    let c_client_dir = dir.clone();
    let c_client = tokio::task::spawn_blocking(move || {
        let client = Command::new(c_binary_v10().unwrap())
            .args(["-P", "2:5047/13@fidonet", "-p", "binkd.conf"])
            .current_dir(&c_client_dir)
            .status()
            .expect("cannot run the C client");
        assert!(client.success(), "the C client failed");
    });

    let (server, c_client) = tokio::join!(server, c_client);
    c_client.unwrap();
    let report = server.expect("the Rust session failed");

    // The C client sent the claim and the request file, and the external
    // program of the port answered the file of the exchange.
    // The C client names a request file at random when it sends it, the port
    // of `netname_()` of `protocol.c`.
    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(requests[0].ends_with(".req"), "{requests:?}");
    // The C client received the file of the exchange into its inbound, and it
    // answered the claim with its own end of the batch.
    let inbound = dir.join("in").join("13b70064.pkt");
    assert!(inbound.is_file(), "the C client did not write the file");
    assert_eq!(std::fs::read(&inbound).unwrap(), b"the answer");
    // The log of the C client holds the whole exchange of the claims of the
    // version 1.0, the port of the messages of `do_prescan()` of `protocol.c`.
    let log = std::fs::read_to_string(dir.join("c-client.log")).unwrap_or_default();
    // The C client claims the FREQ of its own request file.
    assert!(log.contains("send message NUL FREQ"), "{log}");
    // The port announces the queue of its spool: the number of the nodes and
    // the size of the files, the port of `q_get_sizes()`.
    assert!(log.contains("rcvd msg NUL TRF 0 100"), "{log}");
    // The port claims the FREQ of the request file of its queue.
    assert!(log.contains("rcvd msg NUL FREQ"), "{log}");
    // The claim of the port makes the count of the C client nonzero, the port
    // of `state->delay_EOB++` of the `M_NUL FREQ` handler.
    assert!(log.contains("Remote claims to have a FREQ for us"), "{log}");
    // The C client received the request file of the port and the file of the
    // exchange, and it sent the request file of its own outbound.
    assert!(
        log.contains("done (to 2:5047/13@fidonet, OK, S/R: 1/2"),
        "{log}"
    );
    // The port announces the request file of its queue with a random name,
    // the port of `netname_()` of `prothlp.c`, so the C client receives it
    // under that name and writes it into its inbound.
    let received = log
        .lines()
        .find_map(|line| line.split("rcvd msg FILE ").nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .map(|name| name.to_string());
    let Some(received) = received else {
        panic!("the C client received no file of the port: {log}");
    };
    assert!(received.ends_with(".req"), "{received}");
    assert_ne!(received, "13b70064.req", "{received}");
    assert!(
        dir.join("in").join(&received).is_file(),
        "the C client did not write {received}"
    );
    // The port held its own end of the batch back until the C client answered
    // the claim with its own end, the port of `delay_EOB` of `protocol.c`, so
    // the end of the batch of the port left after the end of the C client.
    let ours = report
        .frames
        .iter()
        .position(|frame| frame.sent && frame.name == "EOB");
    let theirs = report
        .frames
        .iter()
        .position(|frame| !frame.sent && frame.name == "EOB");
    let (Some(ours), Some(theirs)) = (ours, theirs) else {
        panic!("the frames of the session hold no end of a batch: {report:?}");
    };
    assert!(
        ours > theirs,
        "the port answered the end of the batch at once: {report:?}"
    );
}

/// The C client skips the request file of the port, and the session holds the
/// queue back, the port of `SKIP()` and of `OK_SEND_FILES()` of `protocol.c`.
///
/// The `skip` directive of the C client declines the file of the port, so the C
/// client answers `M_SKIP` and the port sets `state->r_skipped_flag`, the port of
/// `SKIP()`. The C code sends the queue of the session only while the flag is
/// clear, the port of `OK_SEND_FILES()`.
///
/// The listener advertises the version 1.1: the C client claims the FREQ of the
/// version 1.0 only for a peer of that version, the port of `do_prescan()`, and a
/// claim of the peer holds the end of the batch of the port back, the port of
/// `delay_EOB`. A claim of both sides would hold the end of the batch of both
/// sides, because the C code answers the claim of a peer for a file that it
/// declined only, the port of the `delay_EOB--` of `inbound.c`, and the file of
/// the port is the one that the C client declined.
#[tokio::test]
async fn c_client_skips_the_request_of_rust_server() {
    let Some(_) = c_binary_v10() else {
        eprintln!("skip: set BINKD_C_V10 to the path of the C binary of the version 1.0");
        return;
    };

    let dir = temp_dir("c-client-skip");
    let rust_dir = dir.join("rust");
    std::fs::create_dir_all(rust_dir.join("outb")).unwrap();
    std::fs::create_dir_all(rust_dir.join("in")).unwrap();
    let port = free_port();
    // The C client declines every request file of the peer, the port of the
    // `skip` directive, and it holds its own end of the batch back, the port of
    // `delay_EOB`, so it answers the frame of the peer no more and the test
    // holds the session of the C client until the timeout of the config. The
    // test holds a short timeout of the C client, the port of the test harness,
    // so the session of the C client ends the test in a moment and not in the
    // 20 seconds of the run of the two binaries of the oracle. The timeout of
    // the C code is the silence of the select of its loop, the port of
    // `tv.tv_sec = config->nettimeout` of `protocol.c`, and not the budget
    // of the session: every frame of the peer starts the wait of the select
    // again, so a slow machine holds the margin of the timeout. The exchange
    // of the session holds 30 ms, the port of the run of the two binaries of
    // the oracle, and the margin is 66 times the exchange. The C code takes
    // the size of a `skip` entry before the mask, and the size of the entry
    // of the test is `0`: the C code reads the size `-` as a mask then, the
    // port of `read_skip()` of `readcfg.c`.
    let config = format!(
        "domain fidonet outb 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"C client\"\n\
         sysop \"C sysop\"\n\
         location \"C location\"\n\
         nodeinfo \"C node\"\n\
         inbound in\n\
         log c-client.log\n\
         loglevel 10\n\
         conlog 0\n\
         timeout 2\n\
         node 2:5047/13@fidonet 127.0.0.1:{port} secret\n\
         prescan\n\
         skip 0 *.req\n\
         zallow *\n"
    );
    std::fs::write(dir.join("binkd.conf"), config).unwrap();
    std::fs::write(dir.join("outb/13b7000d.req"), vec![b'r'; 100]).unwrap();

    // The external program of the port answers the file of the exchange.
    let answer = rust_dir.join("outb/13b70064.pkt");
    let mut srif = Erp {
        answer: answer.clone(),
        requests: Arc::new(Mutex::new(Vec::new())),
        claims: 0,
    };

    let mut server_config = session_config(
        &rust_dir,
        "2:5047/13@fidonet",
        "secret",
        Methods::none(),
        false,
    );
    // The listener advertises the version 1.1, so the C client holds no claim
    // and it answers the end of the batch of the port at once.
    server_config.trace = true;
    let mut spool = spool(&rust_dir);
    spool.domains.push(binkd_ftn::domain::FtnDomain {
        name: "fidonet".to_string(),
        path: rust_dir.display().to_string(),
        dir: "outb".to_string(),
        z: [2, 0],
        idomain: None,
        alias4: None,
    });
    std::fs::write(rust_dir.join("outb/13b70064.req"), vec![b'q'; 100]).unwrap();
    server_config.spool = spool;
    server_config.prescan = true;
    server_config.spool.nodes.add(
        &address("2:5047/100@fidonet"),
        &binkd_ftn::node::NodeSpec {
            hosts: Some("127.0.0.1".to_string()),
            pwd: Some("secret".to_string()),
            ..binkd_ftn::node::NodeSpec::default()
        },
        0,
    );

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    let server = async move {
        let (stream, _) = listener.accept().await.unwrap();
        run_with(
            stream,
            Role::Server,
            &server_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut srif,
            Duration::from_secs(20),
        )
        .await
    };

    let c_client_dir = dir.clone();
    let c_client = tokio::task::spawn_blocking(move || {
        let client = Command::new(c_binary_v10().unwrap())
            .args(["-P", "2:5047/13@fidonet", "-p", "binkd.conf"])
            .current_dir(&c_client_dir)
            .status()
            .expect("cannot run the C client");
        assert!(client.success(), "the C client failed");
    });

    let (server, c_client) = tokio::join!(server, c_client);
    c_client.unwrap();
    let report = server.expect("the Rust session failed");

    // The peer declined the file of the port, so the session holds the queue
    // back, the port of `state->r_skipped_flag` of `protocol.c`.
    assert!(report.skipped, "{report:?}");
    // The port announced the request file of its queue and the file of the
    // exchange, and the peer declined the request file, so the count of the
    // files of the port holds the file that the peer acknowledged only, the
    // port of the count of `GOT()` of `protocol.c`.
    assert_eq!(report.files_sent, 1, "{report:?}");
    assert_eq!(report.received.len(), 1, "{report:?}");
    assert!(report.received[0].netname.ends_with(".req"), "{report:?}");
    let log = std::fs::read_to_string(dir.join("c-client.log")).unwrap_or_default();
    assert!(log.contains("rcvd msg FILE"), "{log}");
    assert!(log.contains("skipping"), "{log}");
    // The C client ends the session with the status of a failure, and the
    // status is NOT a bug of the port: the same C client ends the session
    // with the status of a failure against the C server of the C project too,
    // the port of the run of the two binaries of the oracle. The claim of the
    // port holds the count of the claims of the version 1.0, the port of
    // `delay_EOB` of `protocol.c`, and the request file that the C client
    // declined is the one that would answer the claim, so the C client holds
    // its own end of the batch back and its own timeout ends the session.
    //
    // The run of the two binaries of the oracle holds the proof, the port of
    // the log of the two sides:
    //
    //   the C client: done (to 2:5047/13@fidonet, failed, S/R: 1/0)
    //   the C server: done (from 2:5047/100@fidonet, OK, S/R: 0/1)
    //
    // The port holds the status of the C server, the port of the session of
    // the C code, so the session of the port ends with the status of a
    // success and the C client ends the session of the test with the status
    // of a failure.
    assert!(
        log.contains("done (to 2:5047/13@fidonet, failed, S/R: 1/1"),
        "{log}"
    );
}

/// The external FREQ processor of the test, the port of the program of an
/// `exec` entry with the `*S` macro.
struct Erp {
    /// The response file that the program answers.
    answer: std::path::PathBuf,
    /// The names of the request files that the program read.
    requests: Arc<Mutex<Vec<String>>>,
    /// The FREQ claims of the peer that the program did not answer yet, the
    /// port of `delay_EOB` of `protocol.c`.
    claims: usize,
}

impl Srif for Erp {
    fn claim(&mut self) {
        self.claims += 1;
    }

    fn end_of_batch(&mut self) {
        self.claims = 0;
    }

    fn on_file(&mut self, filename: &str, _peer: &Peer) -> Vec<String> {
        // The program of the entry runs for the request file of the peer, and
        // it answers the file of the exchange. The program writes the file of
        // the answer, so the prescan of the port does not find it.
        if filename.ends_with(".req") {
            self.requests.lock().unwrap().push(filename.to_string());
            std::fs::write(&self.answer, b"the answer").unwrap();
            return vec![self.answer.display().to_string()];
        }
        Vec::new()
    }

    fn flush(&mut self, _peer: &Peer) -> Vec<String> {
        Vec::new()
    }

    fn claimed(&self) -> bool {
        self.claims > 0
    }

    fn finish(&mut self) {}
}
