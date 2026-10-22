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
use binkd_proto::{
    run, Frame, Method, Methods, Recorder, Report, Role, SendFile, SessionConfig, ZRule,
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

/// Returns a port that is free at the moment.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
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

/// Returns the frames of the bytes of a direction, in the stable form.
///
/// The arguments that differ between two runs, the time, the challenge, and the
/// name of a file, are not part of the form.
async fn frames(bytes: &[u8]) -> Vec<String> {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut out = Vec::new();
    while cursor.position() < bytes.len() as u64 {
        let Ok(frame) = binkd_proto::read_frame(&mut cursor).await else {
            break;
        };
        if let Some(name) = frame_name(&frame) {
            out.push(name);
        }
    }
    out
}

/// Returns the stable name of a frame, or `None` for a frame that differs
/// between two runs.
fn frame_name(frame: &Frame) -> Option<String> {
    match frame {
        Frame::Data(data) => Some(format!("DATA {}", data.len())),
        Frame::Command(command, argument) => {
            use binkd_proto::Command;
            let name = match command {
                Command::Nul => "NUL",
                Command::Adr => "ADR",
                Command::Pwd => "PWD",
                Command::File => "FILE",
                Command::Ok => "OK",
                Command::Eob => "EOB",
                Command::Got => "GOT",
                Command::Err => "ERR",
                Command::Bsy => "BSY",
                Command::Get => "GET",
                Command::Skip => "SKIP",
            };
            if *command != Command::Nul {
                return Some(name.to_string());
            }
            let text = String::from_utf8_lossy(argument);
            let kind = text.split_whitespace().next().unwrap_or_default();
            if matches!(kind, "SYS" | "ZYZ" | "LOC" | "NDL" | "VER" | "OPT") {
                Some(format!("{name} {kind}"))
            } else {
                None
            }
        }
    }
}

/// Returns the frames that the C binary sent, in the order of its log.
///
/// The C code writes the frame of the same message at the queue and again at the
/// write, so the sequence holds each frame twice. The function is not used by the
/// test of the diff yet.
#[allow(dead_code)]
fn c_frames_sent(log: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in log.lines() {
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

/// Reports the first place where two sequences differ.
fn first_divergence(ours: &[String], theirs: &[String], what: &str) {
    for (i, (a, b)) in ours.iter().zip(theirs.iter()).enumerate() {
        if a != b {
            panic!(
                "the frames of {what} differ at {i}: the port sent {a:?}, the C binary sent {b:?}\n\
                 the port: {ours:?}\n\
                 the C binary: {theirs:?}"
            );
        }
    }
    assert_eq!(
        ours.len(),
        theirs.len(),
        "the count of the frames of {what} differs: the port {:?}, the C binary {:?}",
        ours,
        theirs
    );
}

/// The frame sequence that the port sends matches the frame sequence that the C
/// binary receives, so a divergence of the flow of the port fails the suite.
///
/// The C binary logs its frames at `loglevel 10`, and the port records the bytes
/// of its transport. Only the received direction of the C log is compared: the C code
/// writes the frame of the same message at the queue and again at the write, so its
/// own log holds every sent frame twice, and the order of the two copies needs the
/// study. The frames that the C binary sends are compared by the replay test of a
/// live session and by the interop test of each mode.
#[tokio::test]
async fn frame_sequences_match() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let dir = temp_dir("frames");
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let packet = out.join("13b7000d.pkt");
    let content = vec![b'f'; 512];
    std::fs::write(&packet, &content).unwrap();

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
    let mut recorder = Recorder::new(stream, Role::Client);
    let result = run(
        &mut recorder,
        Role::Client,
        &config,
        &files,
        &mut |_| Ok(()),
        Duration::from_secs(20),
    )
    .await;

    result.expect("the Rust session failed");
    // The C binary writes its log after the session, so wait for the last frame.
    let mut log = String::new();
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        log = std::fs::read_to_string(dir.join("c-server.log")).unwrap_or_default();
        if c_frames_received(&log).last().is_some() {
            break;
        }
    }
    let _ = server.kill();
    let _ = server.wait();

    let recording = recorder.into_recording();

    // The frames that the port sent are the frames that the C server received.
    first_divergence(
        &frames(&recording.sent).await,
        &c_frames_received(&log),
        "the C server received",
    );
}
