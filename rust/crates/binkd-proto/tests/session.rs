//! A session between two binkp partners in memory.
//!
//! The test drives the two ends over a memory transport: a client and a server
//! exchange the banner, the password, and one file in each direction.

use std::path::{Path, PathBuf};
use std::time::Duration;

use binkd_ftn::addr::FtnAddr;
use binkd_proto::{run, Method, Methods, Report, Role, SendFile, SessionConfig, ZRule};

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
    config_with(dir, aka, password, Methods::none())
}

/// The settings of a session with the compression methods of a side.
fn config_with(dir: &Path, aka: &str, password: &str, methods: Methods) -> SessionConfig {
    SessionConfig {
        sysname: format!("test {aka}"),
        sysop: "Test sysop".to_string(),
        location: "Test location".to_string(),
        nodeinfo: "Test node".to_string(),
        addresses: vec![address(aka)],
        password: password.to_string(),
        inbound: dir.to_path_buf(),
        version: "test/1.0".to_string(),
        blksize: 4096,
        zminsize: 1024,
        zlevel: 0,
        zrules: vec![ZRule {
            mask: "*".to_string(),
            allow: true,
        }],
        methods,
        crypt: true,
        nr: false,
        nd: false,
        domains: binkd_ftn::domain::DomainTable::new(Vec::new()),
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
        path: Some(path),
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

/// A session that compresses a file in each direction.
async fn session_with_compression(name: &str, methods: Methods) -> (Report, Report) {
    let client_dir = temp_dir(&format!("client-{name}"));
    let server_dir = temp_dir(&format!("server-{name}"));
    let out = temp_dir(&format!("out-{name}"));
    let client_out = out.join("client");
    let server_out = out.join("server");
    std::fs::create_dir_all(&client_out).unwrap();
    std::fs::create_dir_all(&server_out).unwrap();

    let client_files = vec![make_file(&client_out, "13b7000d.pkt", 5000)];
    let server_files = vec![make_file(&server_out, "13b70064.pkt", 5000)];
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client_config = config_with(&client_dir, "2:5047/13@fidonet", "", methods);
    let server_config = config_with(&server_dir, "2:5047/100@fidonet", "", methods);

    let client = async move {
        run(
            client_stream,
            Role::Client,
            &client_config,
            &client_files,
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let server = async move {
        run(
            server_stream,
            Role::Server,
            &server_config,
            &server_files,
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    (client.unwrap(), server.unwrap())
}

/// Both sides compress the files of 5000 bytes, above `zminsize`.
#[tokio::test]
async fn session_with_gz_compression() {
    let (client, server) = session_with_compression("gz", Methods::of(Method::Gz)).await;
    assert_eq!(client.files_sent, 1);
    assert_eq!(client.files_received, 1);
    assert_eq!(server.files_sent, 1);
    assert_eq!(server.files_received, 1);
    assert_eq!(server.received[0].method, Some(Method::Gz));
    assert_eq!(server.received[0].size, 5000);
}

/// bzip2 is preferred over zlib when both sides hold both.
#[tokio::test]
async fn session_with_bz2_compression() {
    let (_, server) = session_with_compression("bz2", Methods::all()).await;
    assert_eq!(server.received[0].method, Some(Method::Bz2));
    assert_eq!(server.received[0].size, 5000);
}

/// A file below `zminsize` stays plain.
#[tokio::test]
async fn session_without_compression_of_a_small_file() {
    let client_dir = temp_dir("client-small");
    let server_dir = temp_dir("server-small");
    let out = temp_dir("out-small");
    let client_out = out.join("client");
    std::fs::create_dir_all(&client_out).unwrap();

    let client_files = vec![make_file(&client_out, "13b7000d.pkt", 100)];
    let (client, server) =
        session(&client_dir, &server_dir, "", "", client_files, Vec::new()).await;
    let client = client.unwrap();
    let server = server.unwrap();
    assert_eq!(client.files_sent, 1);
    assert_eq!(server.received[0].method, None);
}

/// A rule that denies the name keeps the file plain.
#[tokio::test]
async fn session_without_compression_of_a_denied_file() {
    let client_dir = temp_dir("client-deny");
    let server_dir = temp_dir("server-deny");
    let out = temp_dir("out-deny");
    let client_out = out.join("client");
    std::fs::create_dir_all(&client_out).unwrap();

    let client_files = vec![make_file(&client_out, "13b7000d.pkt", 5000)];
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let mut client_config = config(&client_dir, "2:5047/13@fidonet", "");
    client_config.methods = Methods::all();
    client_config.zrules = vec![ZRule {
        mask: "*.pkt".to_string(),
        allow: false,
    }];
    let server_config = config(&server_dir, "2:5047/100@fidonet", "");
    let client = async move {
        run(
            client_stream,
            Role::Client,
            &client_config,
            &client_files,
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let server = async move {
        run(
            server_stream,
            Role::Server,
            &server_config,
            &[],
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    assert!(client.is_ok(), "{client:?}");
    let server = server.unwrap();
    assert_eq!(server.received[0].method, None);
    assert_eq!(server.received[0].size, 5000);
}

/// Both sides with a password crypt the traffic of the session.
#[tokio::test]
async fn session_with_crypt() {
    let client_dir = temp_dir("client-crypt");
    let server_dir = temp_dir("server-crypt");
    let out = temp_dir("out-crypt");
    let client_out = out.join("client");
    std::fs::create_dir_all(&client_out).unwrap();

    let client_files = vec![make_file(&client_out, "13b7000d.pkt", 300)];
    let (client, server) = session(
        &client_dir,
        &server_dir,
        "secret",
        "secret",
        client_files,
        Vec::new(),
    )
    .await;
    let client = client.unwrap();
    let server = server.unwrap();
    // The answerer sent the challenge, so both sides crypted the session.
    assert!(client.crypt, "the caller did not crypt");
    assert!(server.crypt, "the answerer did not crypt");
    assert_eq!(client.files_sent, 1);
    assert_eq!(server.files_received, 1);
    assert_eq!(
        std::fs::read(server_dir.join("13b7000d.pkt")).unwrap(),
        vec![b'x'; 300]
    );
}

/// Without a password the session stays plain, even with `crypt` set.
#[tokio::test]
async fn session_without_crypt_without_a_password() {
    let client_dir = temp_dir("client-nocrypt");
    let server_dir = temp_dir("server-nocrypt");
    let (client, server) = session(&client_dir, &server_dir, "", "", Vec::new(), Vec::new()).await;
    let client = client.unwrap();
    let server = server.unwrap();
    assert!(!client.crypt);
    assert!(!server.crypt);
}

/// The settings carry the NR and the ND modes of the node.
///
/// The negotiation of the modes arrives in the next step; the test pins that the
/// session holds the fields and runs with them.
#[tokio::test]
async fn session_with_nr_nd() {
    let client_dir = temp_dir("client-nr");
    let server_dir = temp_dir("server-nr");
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let mut client_config = config(&client_dir, "2:5047/13@fidonet", "");
    client_config.nr = true;
    client_config.nd = true;
    let mut server_config = config(&server_dir, "2:5047/100@fidonet", "");
    server_config.nr = true;
    server_config.nd = true;
    let client = async move {
        run(
            client_stream,
            Role::Client,
            &client_config,
            &[],
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let server = async move {
        run(
            server_stream,
            Role::Server,
            &server_config,
            &[],
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    assert!(client.is_ok(), "{client:?}");
    assert!(server.is_ok(), "{server:?}");
}

/// A session with the NR and the ND modes of the two nodes.
async fn session_with_modes(name: &str, nr: bool, nd: bool) -> (Report, Report) {
    let client_dir = temp_dir(&format!("client-{name}"));
    let server_dir = temp_dir(&format!("server-{name}"));
    let out = temp_dir(&format!("out-{name}"));
    let client_out = out.join("client");
    std::fs::create_dir_all(&client_out).unwrap();
    let client_files = vec![make_file(&client_out, "13b7000d.pkt", 300)];
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let mut client_config = config(&client_dir, "2:5047/13@fidonet", "");
    client_config.nr = nr;
    client_config.nd = nd;
    let mut server_config = config(&server_dir, "2:5047/100@fidonet", "");
    server_config.nr = nr;
    server_config.nd = nd;
    let client = async move {
        run(
            client_stream,
            Role::Client,
            &client_config,
            &client_files,
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let server = async move {
        run(
            server_stream,
            Role::Server,
            &server_config,
            &[],
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    (client.unwrap(), server.unwrap())
}

/// The NR mode: the caller announces the file with the offset -1, the
/// answerer asks for it with `M_GET`, and the caller sends the file.
#[tokio::test]
async fn session_with_nr_mode() {
    let (client, server) = session_with_modes("nr", true, false).await;
    assert_eq!(client.files_sent, 1);
    assert_eq!(server.files_received, 1);
    assert_eq!(server.received[0].size, 300);
}

/// The ND mode: the caller waits for the acknowledgement of the file.
#[tokio::test]
async fn session_with_nd_mode() {
    let (client, server) = session_with_modes("nd", true, true).await;
    assert_eq!(client.files_sent, 1);
    assert_eq!(server.files_received, 1);
    assert_eq!(server.received[0].size, 300);
}

/// The ND mode defers the rename of a received file to the next file or to
/// the end of the batch. Two files of the caller arrive with the right
/// contents, and the receiver runs the callback of each of them.
#[tokio::test]
async fn session_with_nd_mode_two_files() {
    let client_dir = temp_dir("client-nd2");
    let server_dir = temp_dir("server-nd2");
    let out = temp_dir("out-nd2");
    let client_out = out.join("client");
    std::fs::create_dir_all(&client_out).unwrap();
    let client_files = vec![
        make_file(&client_out, "13b7000d.pkt", 300),
        make_file(&client_out, "13b7000e.pkt", 400),
    ];
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let mut client_config = config(&client_dir, "2:5047/13@fidonet", "");
    client_config.nr = true;
    client_config.nd = true;
    let mut server_config = config(&server_dir, "2:5047/100@fidonet", "");
    server_config.nr = true;
    server_config.nd = true;
    let client = async move {
        run(
            client_stream,
            Role::Client,
            &client_config,
            &client_files,
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let server = async move {
        let mut names = Vec::new();
        run(
            server_stream,
            Role::Server,
            &server_config,
            &[],
            &mut |file| {
                names.push(file.netname.clone());
                Ok(())
            },
            Duration::from_secs(5),
        )
        .await
        .map(|report| (report, names))
    };
    let (client, server) = tokio::join!(client, server);
    let client = client.unwrap();
    let (server, names) = server.unwrap();
    assert_eq!(client.files_sent, 2);
    assert_eq!(server.files_received, 2);
    assert_eq!(names, vec!["13b7000d.pkt", "13b7000e.pkt"]);
    assert_eq!(server.received[0].size, 300);
    assert_eq!(server.received[1].size, 400);
    assert_eq!(
        std::fs::read(server_dir.join("13b7000e.pkt")).unwrap(),
        vec![b'x'; 400]
    );
}

/// The ND mode: the caller saves the answer of the peer from `M_GOT` as the
/// link status file of the address of the peer.
#[tokio::test]
async fn session_with_nd_status() {
    use binkd_ftn::domain::{DomainTable, FtnDomain};

    let client_dir = temp_dir("client-nd-status");
    let server_dir = temp_dir("server-nd-status");
    let out = temp_dir("out-nd-status");
    let client_out = out.join("client");
    std::fs::create_dir_all(&client_out).unwrap();
    std::fs::create_dir_all(out.join("outb")).unwrap();
    let client_files = vec![make_file(&client_out, "13b7000d.pkt", 300)];
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let mut client_config = config(&client_dir, "2:5047/13@fidonet", "");
    client_config.nr = true;
    client_config.nd = true;
    // The link status file lands in the outbound directory of the domain.
    client_config.domains = DomainTable::new(vec![FtnDomain {
        name: "fidonet".to_string(),
        path: out.display().to_string(),
        dir: "outb".to_string(),
        z: [2, -1],
        idomain: None,
        alias4: None,
    }]);
    let mut server_config = config(&server_dir, "2:5047/100@fidonet", "");
    server_config.nr = true;
    server_config.nd = true;
    let client = async move {
        run(
            client_stream,
            Role::Client,
            &client_config,
            &client_files,
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let server = async move {
        run(
            server_stream,
            Role::Server,
            &server_config,
            &[],
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    assert!(client.is_ok(), "{client:?}");
    let server = server.unwrap();
    assert_eq!(server.files_received, 1);
    // The status holds the name, the size, and the time of the file.
    let status = std::fs::read_to_string(out.join("outb/13b70064.stc")).unwrap();
    assert_eq!(status, "13b7000d.pkt 300 1000000000");
}
