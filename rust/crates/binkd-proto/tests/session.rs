//! A session between two binkp partners in memory.
//!
//! The test drives the two ends over a memory transport: a client and a server
//! exchange the banner, the password, and one file in each direction.

use std::path::{Path, PathBuf};
use std::time::Duration;

use binkd_ftn::addr::FtnAddr;
use binkd_proto::{run, Report, Role, SendFile, SessionConfig};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("binkd-proto-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn address(s: &str) -> FtnAddr {
    let table = binkd_ftn::domain::DomainTable::new(Vec::new());
    FtnAddr::parse(s, &table).unwrap()
}

fn config(dir: &Path, aka: &str, password: &str) -> SessionConfig {
    SessionConfig {
        sysname: format!("test {aka}"),
        sysop: "Test sysop".to_string(),
        location: "Test location".to_string(),
        nodeinfo: "Test node".to_string(),
        addresses: vec![address(aka)],
        password: password.to_string(),
        inbound: dir.to_path_buf(),
        version: "test/1.0".to_string(),
    }
}

/// Runs the two ends of a session.
async fn session(
    client_dir: &Path,
    server_dir: &Path,
    client_password: &str,
    server_password: &str,
    client_files: Vec<SendFile>,
    server_files: Vec<SendFile>,
) -> (
    Result<Report, binkd_proto::Error>,
    Result<Report, binkd_proto::Error>,
) {
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client_config = config(client_dir, "2:5047/13@fidonet", client_password);
    let server_config = config(server_dir, "2:5047/100@fidonet", server_password);

    let client = async move {
        let mut received = Vec::new();
        run(
            client_stream,
            Role::Client,
            &client_config,
            &client_files,
            &mut |file| {
                received.push(file.clone());
                Ok(())
            },
            Duration::from_secs(5),
        )
        .await
    };
    let server = async move {
        let mut received = Vec::new();
        run(
            server_stream,
            Role::Server,
            &server_config,
            &server_files,
            &mut |file| {
                received.push(file.clone());
                Ok(())
            },
            Duration::from_secs(5),
        )
        .await
    };
    tokio::join!(client, server)
}

/// Makes a file of a size in a directory.
fn make_file(dir: &Path, name: &str, size: usize) -> SendFile {
    let path = dir.join(name);
    std::fs::write(&path, vec![b'x'; size]).unwrap();
    SendFile {
        netname: name.to_string(),
        path,
        size: size as u64,
        time: 1_000_000_000,
    }
}

#[tokio::test]
async fn session_without_password() {
    let client_dir = temp_dir("client");
    let server_dir = temp_dir("server");
    let (client, server) = session(&client_dir, &server_dir, "", "", Vec::new(), Vec::new()).await;
    let client = client.unwrap();
    let server = server.unwrap();
    assert_eq!(client.peer_addresses[0].format(false), "2:5047/100@fidonet");
    assert_eq!(server.peer_addresses[0].format(false), "2:5047/13@fidonet");
    assert_eq!(client.files_sent + server.files_sent, 0);
}

#[tokio::test]
async fn session_with_password() {
    let client_dir = temp_dir("client-pwd");
    let server_dir = temp_dir("server-pwd");
    let (client, server) = session(
        &client_dir,
        &server_dir,
        "secret",
        "secret",
        Vec::new(),
        Vec::new(),
    )
    .await;
    assert!(client.is_ok(), "{client:?}");
    assert!(server.is_ok(), "{server:?}");
}

#[tokio::test]
async fn session_with_a_bad_password() {
    let client_dir = temp_dir("client-bad");
    let server_dir = temp_dir("server-bad");
    let (client, server) = session(
        &client_dir,
        &server_dir,
        "one",
        "other",
        Vec::new(),
        Vec::new(),
    )
    .await;
    // Both sides see the password of the other and reject it.
    assert!(
        matches!(client, Err(binkd_proto::Error::BadPassword)),
        "{client:?}"
    );
    assert!(matches!(server, Err(binkd_proto::Error::BadPassword)));
}

#[tokio::test]
async fn session_with_files() {
    let client_dir = temp_dir("client-files");
    let server_dir = temp_dir("server-files");
    let out = temp_dir("out-files");
    let client_out = out.join("client");
    let server_out = out.join("server");
    std::fs::create_dir_all(&client_out).unwrap();
    std::fs::create_dir_all(&server_out).unwrap();

    let client_files = vec![make_file(&client_out, "13b7000d.pkt", 100)];
    let server_files = vec![make_file(&server_out, "13b70064.pkt", 200)];
    let (client, server) =
        session(&client_dir, &server_dir, "", "", client_files, server_files).await;
    let client = client.unwrap();
    let server = server.unwrap();

    assert_eq!(client.files_sent, 1);
    assert_eq!(client.files_received, 1);
    assert_eq!(server.files_sent, 1);
    assert_eq!(server.files_received, 1);
    assert_eq!(client.bytes_sent, 100);
    assert_eq!(client.bytes_received, 200);

    // The server received the file of the client and the other way around.
    let received = std::fs::read(client_dir.join("13b70064.pkt")).unwrap();
    assert_eq!(received, vec![b'x'; 200]);
    let received = std::fs::read(server_dir.join("13b7000d.pkt")).unwrap();
    assert_eq!(received, vec![b'x'; 100]);
}
