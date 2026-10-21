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

/// Returns a port that is free at the moment.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Starts the C server of a spool.
///
/// The extra lines hold the compression rules of the config.
fn start_c_server(dir: &Path, port: u16, peer: &str, password: &str, extra: &str) -> Child {
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
         loglevel 6\n\
         conlog 0\n\
         node {peer} 127.0.0.1 {password}\n\
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
fn session_config(dir: &Path, aka: &str, password: &str, methods: Methods) -> SessionConfig {
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
    let mut server = start_c_server(&dir, port, "2:5047/13@fidonet", "secret", "");
    assert!(wait_for_port(port).await, "the C server did not start");

    let config = session_config(&dir, "2:5047/13@fidonet", "secret", Methods::none());
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: packet,
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
    let mut server = start_c_server(&dir, port, "2:5047/13@fidonet", "secret", "zallow *");
    assert!(wait_for_port(port).await, "the C server did not start");

    let config = session_config(&dir, "2:5047/13@fidonet", "secret", methods);
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: packet,
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
async fn c_client_polls_rust_server(name: &str, methods: Methods) -> Report {
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
         loglevel 6\n\
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
    let server_config = session_config(&dir, "2:5047/13@fidonet", "secret", methods);
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
    report
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

    let report = c_client_polls_rust_server("c-client-gz", Methods::of(Method::Gz)).await;
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

    let report = c_client_polls_rust_server("c-client-bz2", Methods::all()).await;
    assert_eq!(report.received[0].method, Some(Method::Bz2));
}

/// The case of the plan without compression: the C client calls the Rust server.
#[tokio::test]
async fn c_client_to_rust_server() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let report = c_client_polls_rust_server("c-client", Methods::none()).await;
    assert_eq!(report.received[0].method, None);
}
