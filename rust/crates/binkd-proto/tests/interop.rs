//! The C and Rust interop matrix.
//!
//! The test runs the C binary and the Rust session against each other over the
//! loopback address. It needs the C binary: set `BINKD_C` to its path. Without the
//! variable the test reports that it is skipped.
//!
//! The matrix of the plan is:
//!
//! | Client | Server | State |
//! | --- | --- | --- |
//! | Rust | C | done |
//! | C | Rust | the next step |
//! | C | C | the C test suite of the C project |
//! | Rust | Rust | the in-memory test of `tests/session.rs` |

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use binkd_ftn::addr::FtnAddr;
use binkd_proto::{run, Report, Role, SendFile, SessionConfig};
use tokio::net::TcpStream;

/// Returns the directory of the C binary, or `None` when the variable is not set.
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
fn start_c_server(dir: &Path, port: u16, peer: &str, password: &str) -> Child {
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
         node {peer} 127.0.0.1 {password}\n"
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
    std::fs::write(&packet, vec![b'p'; 512]).unwrap();

    let port = free_port();
    let mut server = start_c_server(&dir, port, "2:5047/13@fidonet", "secret");
    assert!(wait_for_port(port).await, "the C server did not start");

    let config = SessionConfig {
        sysname: "Rust client".to_string(),
        sysop: "Rust sysop".to_string(),
        location: "Rust location".to_string(),
        nodeinfo: "Rust node".to_string(),
        addresses: vec![address("2:5047/13@fidonet")],
        password: "secret".to_string(),
        inbound: dir.join("in"),
        version: "rust-test/0.1".to_string(),
    };
    let files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: packet,
        size: 512,
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
    assert_eq!(std::fs::read(&received).unwrap(), vec![b'p'; 512]);
}

/// The other direction of the matrix: the C client calls the Rust server.
///
/// The C binary creates the poll file with `-P <node>` and runs the clients with `-p`
/// in one call: `-P` alone only adds the poll and starts the daemon, which never
/// exits. The poll file lands in the outbound directory of the domain.
#[tokio::test]
async fn c_client_to_rust_server() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let dir = temp_dir("c-client");
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
         node 2:5047/13@fidonet 127.0.0.1:{port} secret\n"
    );
    std::fs::write(dir.join("binkd.conf"), config).unwrap();

    // The Rust server accepts one session.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    let server_dir = dir.join("rust");
    std::fs::create_dir_all(server_dir.join("in")).unwrap();
    let server_config = SessionConfig {
        sysname: "Rust server".to_string(),
        sysop: "Rust sysop".to_string(),
        location: "Rust location".to_string(),
        nodeinfo: "Rust node".to_string(),
        addresses: vec![address("2:5047/13@fidonet")],
        password: "secret".to_string(),
        inbound: server_dir.join("in"),
        version: "rust-test/0.1".to_string(),
    };
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
}
