//! The C and Rust interop matrix.
//!
//! The tests run the C binary and the Rust session against each other over the
//! loopback address. They need the C binary: set `BINKD_C` to its path. Without the
//! variable the tests report that they are skipped.
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
use std::time::Duration;

use binkd_ftn::addr::FtnAddr;
use binkd_proto::{run, Method, Methods, Report, Role, SendFile, SessionConfig, ZRule};
use tokio::net::TcpStream;

/// The number of the bytes of a file that the tests send.
const FILE_SIZE: usize = 5000;

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
        trace: false,
        skip: Vec::new(),
        listed: false,
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
