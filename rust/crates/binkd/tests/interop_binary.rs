//! The binary-to-binary C and Rust interop sessions.
//!
//! The tests start the C and Rust binaries on the loopback address and run
//! the C server with the Rust client, the Rust server with the C client, and
//! the C server with the C client. Set `BINKD_C` to the C binary; without it
//! the tests report a skip.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

const SERVER_AKA: &str = "2:5047/100@fidonet";
const CLIENT_AKA: &str = "2:5047/13@fidonet";
const PASSWORD: &str = "secret";
const SERVER_PACKET: &str = "13b7000d.cut";
const CLIENT_PACKET: &str = "13b70064.cut";

/// Returns the path of the binary of the port.
fn rust_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_binkd"))
}

/// Returns the path of the C binary, or `None`.
fn c_binary() -> Option<PathBuf> {
    std::env::var("BINKD_C")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

/// Returns a port that the tests of this process do not use twice.
fn free_port() -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(0);
    let base = 23000 + (std::process::id() % 10000) as u16;
    let offset = NEXT.fetch_add(1, Ordering::Relaxed);
    base.wrapping_add(offset % 1000)
}

/// Makes the spool of one side.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("binkd-binary-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("outb")).unwrap();
    fs::create_dir_all(dir.join("in")).unwrap();
    dir
}

/// Writes a config and answers its path.
fn write_config(dir: &Path, text: &str) -> PathBuf {
    let path = dir.join("binkd.conf");
    fs::write(&path, text).unwrap();
    path
}

/// Writes the config of the C server.
fn write_c_server_config(dir: &Path, port: u16) -> PathBuf {
    write_config(
        dir,
        &format!(
            "domain fidonet {} 2\n\
             address {SERVER_AKA}\n\
             sysname \"C server\"\n\
             sysop \"C sysop\"\n\
             location \"C location\"\n\
             nodeinfo \"C node\"\n\
             inbound in\n\
             iport {port}\n\
             listen 127.0.0.1:{port}\n\
             node {CLIENT_AKA} 127.0.0.1 {PASSWORD}\n\
             log c-server.log\n\
             loglevel 8\n\
             conlog 0\n",
            dir.join("outb").display()
        ),
    )
}

/// Writes the config of the Rust server.
fn write_rust_server_config(dir: &Path, port: u16) -> PathBuf {
    write_config(
        dir,
        &format!(
            "domain fidonet {} 2\n\
             address {SERVER_AKA}\n\
             sysname \"Rust server\"\n\
             sysop \"Rust sysop\"\n\
             location \"Rust location\"\n\
             nodeinfo \"Rust node\"\n\
             inbound in\n\
             listen 127.0.0.1:{port}\n\
             node {CLIENT_AKA} 127.0.0.1 {PASSWORD}\n\
             log rust-server.log\n\
             loglevel 8\n\
             conlog 0\n",
            dir.join("outb").display()
        ),
    )
}

/// Writes the config of the C client.
fn write_c_client_config(dir: &Path, port: u16) -> PathBuf {
    write_config(
        dir,
        &format!(
            "domain fidonet {} 2\n\
             address {CLIENT_AKA}\n\
             sysname \"C client\"\n\
             sysop \"C sysop\"\n\
             location \"C location\"\n\
             nodeinfo \"C node\"\n\
             inbound in\n\
             node {SERVER_AKA} 127.0.0.1:{port} {PASSWORD}\n\
             zallow *\n\
             log c-client.log\n\
             loglevel 8\n\
             conlog 0\n",
            dir.join("outb").display()
        ),
    )
}

/// Writes the config of the Rust client.
fn write_rust_client_config(dir: &Path, port: u16) -> PathBuf {
    write_config(
        dir,
        &format!(
            "domain fidonet {} 2\n\
             address {CLIENT_AKA}\n\
             sysname \"Rust client\"\n\
             sysop \"Rust sysop\"\n\
             location \"Rust location\"\n\
             nodeinfo \"Rust node\"\n\
             inbound in\n\
             node {SERVER_AKA} 127.0.0.1:{port} {PASSWORD}\n\
             zallow *\n\
             log rust-client.log\n\
             loglevel 8\n\
             conlog 0\n",
            dir.join("outb").display()
        ),
    )
}

/// Writes a packet into the outbound of a side and answers its bytes.
fn packet(dir: &Path, name: &str, size: usize) -> Vec<u8> {
    let byte = if name == SERVER_PACKET { b's' } else { b'c' };
    let content = vec![byte; size];
    fs::write(dir.join("outb").join(name), &content).unwrap();
    content
}

/// One server process of the test.
struct Server {
    child: Child,
}

impl Server {
    /// Starts the C server.
    fn c(dir: &Path, port: u16) -> Server {
        let config = write_c_server_config(dir, port);
        Server::spawn(c_binary().unwrap(), dir, &config)
    }

    /// Starts the Rust server.
    fn rust(dir: &Path, port: u16) -> Server {
        let config = write_rust_server_config(dir, port);
        Server::spawn(rust_binary(), dir, &config)
    }

    fn spawn(program: PathBuf, dir: &Path, config: &Path) -> Server {
        let child = Command::new(program)
            .arg("-s")
            .arg(config)
            .current_dir(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("cannot start the server");
        Server { child }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Runs the C client for one pass.
fn run_c_client(dir: &Path, node: &str) -> ExitStatus {
    Command::new(c_binary().unwrap())
        .args(["-P", node, "-p", "binkd.conf"])
        .current_dir(dir)
        .status()
        .expect("cannot run the C client")
}

/// Runs the Rust client for one pass.
fn run_rust_client(dir: &Path, node: &str) -> ExitStatus {
    Command::new(rust_binary())
        .args(["-P", node, "-p", "binkd.conf"])
        .current_dir(dir)
        .status()
        .expect("cannot run the Rust client")
}

/// Waits until the server listens on the port.
fn wait_for_port(port: u16) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Waits until the inbound of a side holds the bytes.
fn wait_for_content(dir: &Path, content: &[u8]) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if fs::read_dir(dir)
            .map(|entries| {
                entries.flatten().any(|entry| {
                    fs::read(entry.path())
                        .map(|b| b == content)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Asserts that the log holds the text.
fn assert_log(dir: &Path, name: &str, text: &str) {
    let log = fs::read_to_string(dir.join(name)).unwrap_or_default();
    assert!(log.contains(text), "{name}: {log}");
}

/// The Rust client calls the C server.
#[test]
fn the_rust_client_calls_the_c_server() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };
    let server_dir = temp_dir("rust-client-c-server");
    let client_dir = temp_dir("rust-client-c-client");
    let port = free_port();
    let server_packet = packet(&server_dir, SERVER_PACKET, 400);
    let client_packet = packet(&client_dir, CLIENT_PACKET, 500);
    write_rust_client_config(&client_dir, port);
    let _server = Server::c(&server_dir, port);
    assert!(wait_for_port(port), "the C server did not listen");

    let status = run_rust_client(&client_dir, SERVER_AKA);
    assert!(status.success(), "the Rust client failed: {status}");
    assert!(
        wait_for_content(&server_dir.join("in"), &client_packet),
        "the C server holds no packet of the Rust client"
    );
    assert!(
        wait_for_content(&client_dir.join("in"), &server_packet),
        "the Rust client holds no packet of the C server"
    );
    assert_log(&server_dir, "c-server.log", "BEGIN, binkd/");
    assert_log(&client_dir, "rust-client.log", "BEGIN, binkd/");
}

/// The C client calls the Rust server.
#[test]
fn the_c_client_calls_the_rust_server() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };
    let server_dir = temp_dir("c-client-rust-server");
    let client_dir = temp_dir("c-client-rust-client");
    let port = free_port();
    let server_packet = packet(&server_dir, SERVER_PACKET, 400);
    let client_packet = packet(&client_dir, CLIENT_PACKET, 500);
    write_c_client_config(&client_dir, port);
    let _server = Server::rust(&server_dir, port);
    assert!(wait_for_port(port), "the Rust server did not listen");

    let status = run_c_client(&client_dir, SERVER_AKA);
    assert!(status.success(), "the C client failed: {status}");
    assert!(
        wait_for_content(&server_dir.join("in"), &client_packet),
        "the Rust server holds no packet of the C client"
    );
    assert!(
        wait_for_content(&client_dir.join("in"), &server_packet),
        "the C client holds no packet of the Rust server"
    );
    assert_log(&server_dir, "rust-server.log", "BEGIN, binkd/");
    assert_log(&client_dir, "c-client.log", "BEGIN, binkd/");
}

/// The C client calls the C server.
#[test]
fn the_c_client_calls_the_c_server() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };
    let server_dir = temp_dir("c-client-c-server");
    let client_dir = temp_dir("c-client-c-client");
    let port = free_port();
    let server_packet = packet(&server_dir, SERVER_PACKET, 400);
    let client_packet = packet(&client_dir, CLIENT_PACKET, 500);
    write_c_client_config(&client_dir, port);
    let _server = Server::c(&server_dir, port);
    assert!(wait_for_port(port), "the C server did not listen");

    let status = run_c_client(&client_dir, SERVER_AKA);
    assert!(status.success(), "the C client failed: {status}");
    assert!(
        wait_for_content(&server_dir.join("in"), &client_packet),
        "the C server holds no packet of the C client"
    );
    assert!(
        wait_for_content(&client_dir.join("in"), &server_packet),
        "the C client holds no packet of the C server"
    );
    assert_log(&server_dir, "c-server.log", "BEGIN, binkd/");
    assert_log(&client_dir, "c-client.log", "BEGIN, binkd/");
}
