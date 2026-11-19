//! The library engine of the port, the port of the two managers of `main()`.
//!
//! The test starts the server engine and the client engine in one process, so
//! it holds the library form of both roles without the binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rbinkd::{cli::Options, Config, Engine, FtnAddr};
use rbinkd_log::Log;

/// Returns a free port of the process.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Makes the spool of one side.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("binkd-engine-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("outb")).unwrap();
    fs::create_dir_all(dir.join("in")).unwrap();
    dir
}

/// Returns a path for a config line, with the separators of the config format.
fn path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Writes a config and parses it.
fn config(dir: &Path, aka: &str, port: u16, node: Option<&str>) -> Config {
    let outbound = dir.join("outb");
    let inbound = dir.join("in");
    let listen = if node.is_none() {
        format!("listen 127.0.0.1:{port}\n")
    } else {
        String::new()
    };
    let node = node
        .map(|peer| format!("node {peer} 127.0.0.1:{port} secret\n"))
        .unwrap_or_default();
    let text = format!(
        "domain fidonet {} 2\n\
         address {aka}\n\
         sysname \"Engine test\"\n\
         sysop \"Test sysop\"\n\
         location \"Test location\"\n\
         nodeinfo \"Test node\"\n\
         inbound {}\n\
         log {}\n\
         loglevel 6\n\
         conlog 6\n\
         timeout 2s\n\
         {listen}{node}",
        path_text(&outbound),
        path_text(&inbound),
        path_text(&dir.join("binkd.log"))
    );
    let path = dir.join("binkd.conf");
    fs::write(&path, text).unwrap();
    rbinkd_config::parse(&path.display().to_string()).unwrap()
}

/// Returns the files of a directory.
fn files(dir: &Path) -> Vec<Vec<u8>> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_file())
                .filter_map(|entry| fs::read(entry.path()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Waits until the listener of the server accepts a connection.
async fn wait_for_port(port: u16) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the engine server did not bind the port {port}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_engine_serves_and_polls() {
    let server_dir = temp_dir("server");
    let client_dir = temp_dir("client");
    let port = free_port();
    let server_packet = vec![b's'; 400];
    let client_packet = vec![b'c'; 500];
    fs::write(server_dir.join("outb/13b7000d.cut"), &server_packet).unwrap();
    fs::write(client_dir.join("outb/13b70064.cut"), &client_packet).unwrap();

    let server_config = config(&server_dir, "2:5047/100@fidonet", port, None);
    let client_config = config(
        &client_dir,
        "2:5047/13@fidonet",
        port,
        Some("2:5047/100@fidonet"),
    );
    let table = client_config.spool.domains.clone();
    let server_addr = FtnAddr::parse("2:5047/100@fidonet", &table).unwrap();

    let server_log = Arc::new(Log::new(6, 6, "", Vec::new()));
    let client_log = Arc::new(Log::new(6, 6, "", Vec::new()));
    let server = Engine::new(server_config, Options::default(), server_log);
    let mut client = Engine::new(client_config, Options::default(), client_log);
    let server_task = tokio::spawn(server.serve());

    wait_for_port(port).await;
    let report = client
        .poll(&server_addr)
        .await
        .expect("the engine named no node")
        .expect("the engine call failed");
    assert_eq!(report.files_sent, 1, "{report:?}");
    assert_eq!(report.files_received, 1, "{report:?}");

    // The server holds the client packet in its inbound, and the client holds
    // the server packet in its inbound.
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        let server_files = files(&server_dir.join("in"));
        let client_files = files(&client_dir.join("in"));
        if server_files.iter().any(|file| file == &client_packet)
            && client_files.iter().any(|file| file == &server_packet)
        {
            server_task.abort();
            let _ = fs::remove_dir_all(&server_dir);
            let _ = fs::remove_dir_all(&client_dir);
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    server_task.abort();
    let _ = fs::remove_dir_all(&server_dir);
    let _ = fs::remove_dir_all(&client_dir);
    panic!("the engine did not exchange the packets");
}
