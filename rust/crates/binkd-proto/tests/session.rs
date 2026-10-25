//! A session between two binkp partners in memory.
//!
//! The test drives the two ends over a memory transport: a client and a server
//! exchange the banner, the password, and one file in each direction.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use binkd_ftn::addr::FtnAddr;
use binkd_proto::{
    run, run_with, Method, Methods, Protocol, Report, Role, SendFile, SessionConfig, Srif, ZRule,
};

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
        spool: spool(dir),
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

/// The rate limit of a file holds the transfer, the port of `setup_rate_limit()`.
///
/// The client holds an absolute send limit of 4096 bytes per second, so the file of
/// 4096 bytes needs about one second. The port of the C code meters the bytes of the
/// two directions and waits while a direction is over the limit.
#[tokio::test]
async fn the_rate_limit_of_a_file() {
    let client_dir = temp_dir("rate-client");
    let server_dir = temp_dir("rate-server");
    let content = vec![b'r'; 4096];
    let path = client_dir.join("rate.pkt");
    std::fs::write(&path, &content).unwrap();
    let file = SendFile {
        netname: "13b70064.pkt".to_string(),
        path: Some(path),
        size: content.len() as u64,
        time: 1_000_000_000,
    };

    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let mut client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    // The C code reads the `-bw` of the `defnode` when the peer holds no node of
    // its own, the port of the `ADR()` of `protocol.c`.
    client_config.spool.nodes.add(
        &address("0:0/0.0@defnode"),
        &binkd_ftn::node::NodeSpec {
            bw_send: Some(4096),
            ..binkd_ftn::node::NodeSpec::default()
        },
        0,
    );
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");

    let started = std::time::Instant::now();
    let client = async move {
        run(
            client_stream,
            Role::Client,
            &client_config,
            &[file],
            &mut |_| Ok(()),
            Duration::from_secs(20),
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
            Duration::from_secs(20),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    let elapsed = started.elapsed();
    let report = client.expect("the client failed");
    let _ = server.expect("the server failed");
    assert_eq!(report.files_sent, 1);
    // The file arrived in full, and the limit held the transfer.
    assert_eq!(
        std::fs::read(server_dir.join("13b70064.pkt")).unwrap(),
        content
    );
    assert!(elapsed.as_millis() >= 500, "{elapsed:?}");
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

/// The port starts the next batch when the batch held more than the two `M_EOB`
/// frames, the port of the end of the batch of `protocol()`.
///
/// The peer speaks the protocol by hand: it answers the login, takes the end of the
/// batch of the caller, and sends a file in the second batch.
#[tokio::test]
async fn session_with_multi_batch() {
    use binkd_proto::{read_frame, write_frame, Command, Frame};

    let client_dir = temp_dir("client-batch");
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client_config = config(&client_dir, "2:5047/13@fidonet", "");
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
        let mut stream = server_stream;
        // The banner and the login.
        for line in [
            "SYS batch",
            "ZYZ test",
            "LOC test",
            "NDL test",
            "VER test binkp/1.1",
            "OPT EXTCMD",
        ] {
            write_frame(&mut stream, &Frame::text(Command::Nul, line))
                .await
                .unwrap();
        }
        write_frame(
            &mut stream,
            &Frame::text(Command::Adr, "2:5047/100@fidonet"),
        )
        .await
        .unwrap();
        write_frame(&mut stream, &Frame::text(Command::Ok, "non-secure"))
            .await
            .unwrap();
        // The first batch: the end of the batch of the caller.
        loop {
            if let Ok(Frame::Command(Command::Eob, _)) = read_frame(&mut stream).await {
                break;
            }
        }
        // The second batch: a file of the peer.
        write_frame(
            &mut stream,
            &Frame::text(Command::File, "13b70064.pkt 5 1000000000 0"),
        )
        .await
        .unwrap();
        write_frame(&mut stream, &Frame::Data(b"hello".to_vec()))
            .await
            .unwrap();
        write_frame(&mut stream, &Frame::text(Command::Eob, ""))
            .await
            .unwrap();
        // The caller answers with its end of the second batch.
        loop {
            if let Ok(Frame::Command(Command::Eob, _)) = read_frame(&mut stream).await {
                break;
            }
        }
        write_frame(&mut stream, &Frame::text(Command::Eob, ""))
            .await
            .unwrap();
        // The caller closes the session.
        let _ = read_frame(&mut stream).await;
    };
    let (client, _server) = tokio::join!(client, server);
    let client = client.unwrap();
    assert_eq!(client.files_received, 1);
    assert_eq!(client.received[0].netname, "13b70064.pkt");
    assert_eq!(client.received[0].size, 5);
}

/// A session with a `skip` mask: the receiver declines the file with `M_SKIP`,
/// and the file does not arrive.
#[tokio::test]
async fn session_with_skip_mask() {
    let (client, server) = session_with_skip(false).await;
    assert_eq!(client.files_sent, 1);
    assert_eq!(server.files_received, 0);
    assert!(server.received.is_empty());
}

/// A session with a destructive `skip` mask: the receiver answers with `M_GOT`,
/// and the file does not arrive.
#[tokio::test]
async fn session_with_a_destructive_skip_mask() {
    let (_, server) = session_with_skip(true).await;
    assert_eq!(server.files_received, 0);
    assert!(server.received.is_empty());
}

/// Runs a session where the answerer holds a `skip` mask.
async fn session_with_skip(destr: bool) -> (Report, Report) {
    use binkd_proto::{addrtype, SkipRule};

    let name = if destr { "destructive" } else { "plain" };
    let client_dir = temp_dir(&format!("client-skip-{name}"));
    let server_dir = temp_dir(&format!("server-skip-{name}"));
    let out = temp_dir(&format!("out-skip-{name}"));
    let client_out = out.join("client");
    std::fs::create_dir_all(&client_out).unwrap();
    let client_files = vec![make_file(&client_out, "13b7000d.pkt", 300)];
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client_config = config(&client_dir, "2:5047/13@fidonet", "");
    let mut server_config = config(&server_dir, "2:5047/100@fidonet", "");
    server_config.skip = vec![SkipRule {
        mask: "*.pkt".to_string(),
        atype: addrtype::A_ALL,
        // A size of 0 matches a file of any size, as `skip <mask>` does.
        size: 0,
        destr,
    }];
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

/// The receiver resumes a partial file: the port asks for the file from the
/// position of the partial, and the file arrives complete.
#[tokio::test]
async fn session_with_a_partial_file() {
    use binkd_spool::inbound::{FreeSpace, Inbound, TFile};

    let client_dir = temp_dir("client-partial");
    let server_dir = temp_dir("server-partial");
    let out = temp_dir("out-partial");
    let client_out = out.join("client");
    std::fs::create_dir_all(&client_out).unwrap();
    let content: Vec<u8> = (0..300u32).map(|i| (i % 251) as u8).collect();
    let path = client_out.join("13b7000d.pkt");
    std::fs::write(&path, &content).unwrap();
    let client_files = vec![SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(path),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client_config = config(&client_dir, "2:5047/13@fidonet", "");
    let mut server_config = config(&server_dir, "2:5047/100@fidonet", "");
    server_config.trace = true;

    // The partial file of a transfer that a session left behind.
    let tfile = TFile {
        netname: "13b7000d.pkt".to_string(),
        size: content.len() as u64,
        time: 1_000_000_000,
        start: 0,
    };
    let spool = spool(&server_dir);
    // The inbound of the session, as the settings hold it.
    let inbound = Inbound {
        dir: server_dir.display().to_string(),
        secure: true,
        fa: vec![address("2:5047/100@fidonet")],
        nfa: 1,
        skip_all_flag: false,
    };
    let mut rnd = || 7u32;
    let mut partial = inbound
        .inb_fopen(&tfile, &spool, FreeSpace::default(), 0, &mut rnd)
        .expect("the partial file");
    std::io::Write::write_all(&mut partial, &content[..100]).unwrap();
    drop(partial);

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
    // The file holds every byte of the sender, not only the rest.
    let received = std::fs::read(server_dir.join("13b7000d.pkt")).unwrap();
    assert_eq!(received.len(), content.len());
    assert_eq!(received, content);
    // The port asked for the file from the position of the partial.
    let names: Vec<&str> = server.frames.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"GET"), "{names:?}");
    assert!(names.contains(&"GOT"), "{names:?}");
}

/// The SRIF events of the tests: it records the files and the end of the session.
#[derive(Default)]
struct Recorder {
    /// The names of the received files.
    files: Arc<Mutex<Vec<String>>>,
    /// The response file that an exchange answers, once.
    answer: Arc<Mutex<Option<std::path::PathBuf>>>,
    /// The claims of the peer.
    claims: Arc<Mutex<usize>>,
    /// The number of the ends of the session.
    finished: Arc<Mutex<usize>>,
}

impl Srif for Recorder {
    fn claim(&mut self) {
        if let Ok(mut claims) = self.claims.lock() {
            *claims += 1;
        }
    }

    fn end_of_batch(&mut self) {
        if let Ok(mut claims) = self.claims.lock() {
            *claims = 0;
        }
    }

    fn on_file(&mut self, filename: &str, _peer: &binkd_proto::Peer) -> Vec<String> {
        if let Ok(mut files) = self.files.lock() {
            files.push(filename.to_string());
        }
        Vec::new()
    }

    fn claimed(&self) -> bool {
        false
    }

    fn flush(&mut self, _peer: &binkd_proto::Peer) -> Vec<String> {
        // The port of `process_rcvdlist()`: the exchange answers the files of
        // the response file, and the session sends them.
        self.answer
            .lock()
            .ok()
            .and_then(|mut answer| answer.take())
            .map(|path| vec![path.display().to_string()])
            .unwrap_or_default()
    }

    fn finish(&mut self) {
        if let Ok(mut finished) = self.finished.lock() {
            *finished += 1;
        }
    }
}

/// The session calls the SRIF events of the C code: the received file, the end
/// of the batch, and the end of the session.
#[tokio::test]
async fn the_srif_events_of_a_session() {
    let client_dir = temp_dir("srif-client");
    let server_dir = temp_dir("srif-server");
    let content = vec![b's'; 100];
    let path = client_dir.join("out").join("13b7000d.pkt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, &content).unwrap();
    let file = SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(path),
        size: content.len() as u64,
        time: 1_000_000_000,
    };
    // The answerer holds the file of the exchange, the port of an external FREQ
    // processor.
    let answer = server_dir.join("13b70064.pkt");
    std::fs::write(&answer, b"the answer").unwrap();

    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    let mut server_srif = Recorder {
        answer: Arc::new(Mutex::new(Some(answer.clone()))),
        ..Recorder::default()
    };
    let files = Arc::clone(&server_srif.files);
    let finished = Arc::clone(&server_srif.finished);

    let client = async move {
        let mut received = Vec::new();
        run(
            client_stream,
            Role::Client,
            &client_config,
            &[file],
            &mut |file| {
                received.push(file.clone());
                Ok(())
            },
            Duration::from_secs(5),
        )
        .await
        .map(|report| (report, received))
    };
    let server = async move {
        run_with(
            server_stream,
            Role::Server,
            &server_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut server_srif,
            Duration::from_secs(5),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    let (report, received) = client.expect("the client failed");
    let _ = server.expect("the server failed");

    // The answerer saw the file of the caller, the port of `inb_done()`.
    let names = files.lock().unwrap().clone();
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(names[0].ends_with("13b7000d.pkt"), "{names:?}");
    // The caller received the file of the exchange, the port of the queue of
    // the session, and the answerer ran the end of the session.
    assert_eq!(report.files_received, 1, "{report:?}");
    assert_eq!(received.len(), 1, "{received:?}");
    assert_eq!(received[0].netname, "13b70064.pkt");
    assert_eq!(*finished.lock().unwrap(), 1);
}
