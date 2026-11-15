//! A session between two binkp partners in memory.
//!
//! The test drives the two ends over a memory transport: a client and a server
//! exchange the banner, the password, and one file in each direction.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use binkd_ftn::addr::FtnAddr;
use binkd_proto::{
    run, run_with, Error, Method, Methods, Protocol, Report, Role, SendFile, SessionConfig, Srif,
    ZRule,
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
        free_space: binkd_spool::inbound::assume_enough,
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
        no_md5: false,
        md_flag: 0,
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
    assert_eq!(std::fs::read(server_dir.join("rate.pkt")).unwrap(), content);
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

/// Answers no free space for the provider test.
fn no_space(_: &Path) -> u64 {
    0
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

/// The free-space provider of the settings reaches `inb_fopen()`: the receiver
/// declines the file when the provider answers less than `minfree`.
#[tokio::test]
async fn the_free_space_provider_reaches_the_inbound() {
    let client_dir = temp_dir("client-free-space");
    let server_dir = temp_dir("server-free-space");
    let out = temp_dir("out-free-space");
    let client_out = out.join("client");
    std::fs::create_dir_all(&client_out).unwrap();
    let client_files = vec![make_file(&client_out, "13b7000d.pkt", 300)];
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client_config = config(&client_dir, "2:5047/13@fidonet", "");
    let mut server_config = config(&server_dir, "2:5047/100@fidonet", "");
    server_config.spool.minfree = 1;
    server_config.spool.minfree_nonsecure = 1;
    server_config.free_space = no_space;

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
    let client = client.unwrap();
    let server = server.unwrap();
    assert_eq!(client.files_sent, 0, "{client:?}");
    assert_eq!(server.files_received, 0, "{server:?}");
    assert!(server.received.is_empty(), "{server:?}");
}

/// A session with a `skip` mask: the receiver declines the file with `M_SKIP`,
/// and the file does not arrive.
#[tokio::test]
async fn session_with_skip_mask() {
    let (client, server) = session_with_skip(false).await;
    // The answerer declines the file with `M_SKIP`, so the count of the files
    // of the session holds the acknowledgement of the peer only, the port of
    // `GOT()`, and the declined file holds no count.
    assert_eq!(client.files_sent, 0);
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

/// The hooks of the test of the message queue: `on_handshake` queues a `M_NUL`
/// with the sink that the session supplies, the port of `msg_send2()`.
#[derive(Default)]
struct QueuingHooks {
    sink: Mutex<Option<binkd_proto::MessageSink>>,
}

impl binkd_proto::Hooks for QueuingHooks {
    fn set_message_sink(&self, sink: Option<binkd_proto::MessageSink>) {
        if let Ok(mut slot) = self.sink.lock() {
            *slot = sink;
        }
    }

    fn on_handshake(&self, _vars: &mut binkd_proto::Handshake) -> Option<String> {
        if let Ok(slot) = self.sink.lock() {
            if let Some(sink) = slot.as_ref() {
                sink(0, "OPT HOOK");
            }
        }
        None
    }
}

#[tokio::test]
async fn a_hook_message_reaches_the_peer() {
    let client_dir = temp_dir("sink-client");
    let server_dir = temp_dir("sink-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let mut server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    server_config.trace = true;
    let hooks = Arc::new(QueuingHooks::default());
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);

    let client = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            client_stream,
            Role::Client,
            &client_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
            Duration::from_secs(2),
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
            Duration::from_secs(2),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    assert!(client.is_ok(), "{client:?}");
    let server = server.unwrap();
    let names: Vec<&str> = server.frames.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"NUL OPT"), "{names:?}");
}

/// The hooks of the test of the session values.
#[derive(Default)]
struct ValueHooks {
    values: Mutex<Option<binkd_proto::SessionVars>>,
}

impl binkd_proto::Hooks for ValueHooks {
    fn after_handshake(&self, vars: &binkd_proto::SessionVars) -> Option<String> {
        if let Ok(mut slot) = self.values.lock() {
            *slot = Some(vars.clone());
        }
        None
    }
}

#[tokio::test]
async fn the_no_md5_option_disables_the_challenge() {
    let client_dir = temp_dir("no-md5-client");
    let server_dir = temp_dir("no-md5-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let mut server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    server_config.no_md5 = true;
    let hooks = Arc::new(ValueHooks::default());
    let values = Arc::clone(&hooks);
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            client_stream,
            Role::Client,
            &client_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
            Duration::from_secs(2),
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
            Duration::from_secs(2),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    assert!(client.is_ok(), "{client:?}");
    assert!(server.is_ok(), "{server:?}");
    let values = values
        .values
        .lock()
        .unwrap()
        .clone()
        .expect("the handshake hook saw no values");
    assert_eq!(values.opt.md, 0);
}

#[tokio::test]
async fn the_node_that_requires_md5_fails_without_a_challenge() {
    let client_dir = temp_dir("must-md5-client");
    let server_dir = temp_dir("must-md5-server");
    let mut client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    client_config.md_flag = binkd_ftn::node::MD_ON;
    let mut server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    server_config.no_md5 = true;
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        run(
            client_stream,
            Role::Client,
            &client_config,
            &[],
            &mut |_| Ok(()),
            Duration::from_secs(2),
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
            Duration::from_secs(2),
        )
        .await
    };
    let (client, _server) = tokio::join!(client, server);
    assert!(matches!(client, Err(Error::Md5Required)), "{client:?}");
}

#[tokio::test]
async fn the_session_values_reach_the_hook() {
    let client_dir = temp_dir("values-client");
    let server_dir = temp_dir("values-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let server_config = config_with(&server_dir, "2:5047/100@fidonet", "secret", Methods::all());
    let sysname = server_config.sysname.clone();
    let sysop = server_config.sysop.clone();
    let location = server_config.location.clone();
    let hooks = Arc::new(ValueHooks::default());
    let values = Arc::clone(&hooks);
    let info = binkd_proto::SessionInfo {
        peer_host: "peer.example".to_string(),
        peer_ip: "192.0.2.1".to_string(),
        our_ip: "198.51.100.2".to_string(),
        our_port: 24554,
        traffic_mail: 100,
        traffic_file: 200,
    };
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            client_stream,
            Role::Client,
            &client_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut srif,
            info,
            Some(hooks),
            Duration::from_secs(2),
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
            Duration::from_secs(2),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    assert!(client.is_ok(), "{client:?}");
    assert!(server.is_ok(), "{server:?}");
    let values = values
        .values
        .lock()
        .unwrap()
        .clone()
        .expect("the hook saw no values");
    assert!(values.start > 0, "{values:?}");
    assert_eq!(values.host, "peer.example");
    assert_eq!(values.ip, "192.0.2.1");
    assert_eq!(values.our_ip, "198.51.100.2");
    assert_eq!(values.our_port, 24554);
    assert_eq!(values.sysname, sysname);
    assert_eq!(values.sysop, sysop);
    assert_eq!(values.location, location);
    assert_eq!(values.opt.gz, 3);
    assert_eq!(values.opt.md, 1);
    assert_eq!(values.opt.crypt, 1);
    assert_eq!(values.opt.nd, 0);
    assert_eq!(values.opt.nr, 0);
    assert_eq!(values.traffic_mail, 100);
    assert_eq!(values.traffic_file, 200);
    assert_eq!(values.z_send, 0);
    assert_eq!(values.z_recv, 0);
}

/// The hooks of the test of the traffic of the queue.
#[derive(Default)]
struct AfterSentHooks {
    values: Mutex<Option<binkd_proto::SessionVars>>,
    before: Mutex<Option<binkd_proto::SessionVars>>,
}

impl binkd_proto::Hooks for AfterSentHooks {
    fn before_send(
        &self,
        vars: &binkd_proto::SessionVars,
        _file: &mut binkd_proto::FileVars,
    ) -> i32 {
        if let Ok(mut slot) = self.before.lock() {
            if slot.is_none() {
                *slot = Some(vars.clone());
            }
        }
        0
    }

    fn after_sent(
        &self,
        vars: &binkd_proto::SessionVars,
        _file: &mut binkd_proto::SentVars,
    ) -> i32 {
        if let Ok(mut slot) = self.values.lock() {
            *slot = Some(vars.clone());
        }
        0
    }
}

#[tokio::test]
async fn the_traffic_of_the_files_reaches_the_hook() {
    let client_dir = temp_dir("traffic-client");
    let server_dir = temp_dir("traffic-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    let packet = vec![b'p'; 100];
    let file = vec![b'f'; 200];
    let packet_path = client_dir.join("13b70064.pkt");
    let file_path = client_dir.join("13b70064.cut");
    std::fs::write(&packet_path, &packet).unwrap();
    std::fs::write(&file_path, &file).unwrap();
    let packet_file = packet_path.display().to_string();
    let file_file = file_path.display().to_string();
    let sends = vec![
        SendFile {
            netname: "13b70064.pkt".to_string(),
            path: Some(packet_path),
            size: packet.len() as u64,
            time: 1_000_000_000,
        },
        SendFile {
            netname: "13b70064.cut".to_string(),
            path: Some(file_path),
            size: file.len() as u64,
            time: 1_000_000_000,
        },
    ];
    let hooks = Arc::new(AfterSentHooks::default());
    let values = Arc::clone(&hooks);
    let before = Arc::clone(&hooks);
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            client_stream,
            Role::Client,
            &client_config,
            &mut |_| sends.clone(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
            Duration::from_secs(2),
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
            Duration::from_secs(2),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    assert!(client.is_ok(), "{client:?}");
    assert!(server.is_ok(), "{server:?}");
    let values = values
        .values
        .lock()
        .unwrap()
        .clone()
        .expect("the hook saw no values");
    assert_eq!(values.traffic_mail, 100);
    assert_eq!(values.traffic_file, 200);
    // The C code frees the queue of the session when it sent every file, so
    // the hook of a sent file holds no queue. The hook before the send holds
    // the queue, the port of `setup_queue()` of `perl_before_send()`.
    assert!(values.queue.is_empty(), "{values:?}");
    let before = before
        .before
        .lock()
        .unwrap()
        .clone()
        .expect("the send hook ran no queue");
    assert_eq!(before.queue.len(), 2);
    assert_eq!(before.queue[0].file, packet_file);
    assert_eq!(before.queue[0].size, 100);
    assert_eq!(before.queue[0].time, 1_000_000_000);
    assert_eq!(before.queue[0].sent, 1);
    assert_eq!(before.queue[0].flvr, 'f');
    assert_eq!(before.queue[0].action, '\0');
    assert_eq!(before.queue[0].type_, 'm');
    assert_eq!(
        before.queue[0].addr.format(false),
        address("2:5047/100@fidonet").format(false)
    );
    assert_eq!(before.queue[1].file, file_file);
    assert_eq!(before.queue[1].size, 200);
    assert_eq!(before.queue[1].type_, '\0');
}

/// The hooks of the test of the queue rebuild: the script changes `@queue`
/// in `before_send()`, the port of `refresh_queue()` of `perlhooks.c`.
struct RebuildHooks {
    file: PathBuf,
    size: u64,
    time: i64,
    added: AtomicBool,
    queue: Mutex<Option<Vec<binkd_proto::QueueFile>>>,
}

impl binkd_proto::Hooks for RebuildHooks {
    fn before_send(
        &self,
        vars: &binkd_proto::SessionVars,
        _file: &mut binkd_proto::FileVars,
    ) -> i32 {
        // The session writes the queue into `@queue` before the hook, so the
        // script starts from the current queue.
        let mut queue = vars.queue.clone();
        if !self.added.swap(true, Ordering::SeqCst) {
            queue.push(binkd_proto::QueueFile {
                file: self.file.display().to_string(),
                size: self.size,
                time: self.time,
                sent: 0,
                flvr: 'f',
                action: '\0',
                type_: '\0',
                addr: address("2:5047/100@fidonet"),
            });
        }
        if let Ok(mut slot) = self.queue.lock() {
            *slot = Some(queue);
        }
        0
    }

    fn refresh_queue(&self, _fallback: &FtnAddr) -> Option<Vec<binkd_proto::QueueFile>> {
        self.queue.lock().ok().and_then(|slot| slot.clone())
    }
}

#[tokio::test]
async fn the_hook_rebuilds_the_session_files() {
    let client_dir = temp_dir("rebuild-client");
    let server_dir = temp_dir("rebuild-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    let first = vec![b'f'; 100];
    let second = vec![b's'; 200];
    let first_path = client_dir.join("13b70064.pkt");
    let second_path = client_dir.join("13b70065.pkt");
    std::fs::write(&first_path, &first).unwrap();
    std::fs::write(&second_path, &second).unwrap();
    let sends = vec![SendFile {
        netname: "13b70064.pkt".to_string(),
        path: Some(first_path),
        size: first.len() as u64,
        time: 1_000_000_000,
    }];
    let hooks = Arc::new(RebuildHooks {
        file: second_path,
        size: second.len() as u64,
        time: 1_000_000_001,
        added: AtomicBool::new(false),
        queue: Mutex::new(None),
    });
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            client_stream,
            Role::Client,
            &client_config,
            &mut |_| sends.clone(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
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
    let client = client.unwrap();
    let server = server.unwrap();
    // The script added the second file to the queue, so the session sent it.
    assert_eq!(client.files_sent, 2);
    assert_eq!(server.received.len(), 2, "{:?}", server.received);
    let mut got_first = false;
    let mut got_second = false;
    for file in &server.received {
        let content = std::fs::read(&file.path).unwrap();
        if content == first {
            got_first = true;
        }
        if content == second {
            got_second = true;
        }
    }
    assert!(got_first, "{:?}", server.received);
    assert!(got_second, "{:?}", server.received);
}

/// The hooks of the test of the order of `after_recv()`: the hook sees the
/// partial file and the real name before the rename, the port of the call of
/// `perl_after_recv()` of `inb_done()`.
struct AfterRecvOrderHooks {
    tmp: Mutex<Option<String>>,
    file: Mutex<Option<String>>,
    tmp_there: AtomicBool,
    file_there: AtomicBool,
    target: String,
}

impl binkd_proto::Hooks for AfterRecvOrderHooks {
    fn after_recv(
        &self,
        _vars: &binkd_proto::SessionVars,
        file: &mut binkd_proto::ReceivedVars,
    ) -> i32 {
        if let Ok(mut slot) = self.tmp.lock() {
            *slot = Some(file.tmp.clone());
        }
        if let Ok(mut slot) = self.file.lock() {
            *slot = Some(file.file.clone());
        }
        self.tmp_there
            .store(std::path::Path::new(&file.tmp).exists(), Ordering::SeqCst);
        self.file_there
            .store(std::path::Path::new(&file.file).exists(), Ordering::SeqCst);
        file.file = self.target.clone();
        0
    }
}

#[tokio::test]
async fn the_hook_of_after_recv_runs_before_the_rename() {
    let client_dir = temp_dir("after-recv-order-client");
    let server_dir = temp_dir("after-recv-order-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    let packet = vec![b'p'; 100];
    let expected = packet.clone();
    let size = packet.len() as u64;
    let path = client_dir.join("13b70064.pkt");
    std::fs::write(&path, &packet).unwrap();
    let target = server_dir.join("hooked.pkt").display().to_string();
    let hooks = Arc::new(AfterRecvOrderHooks {
        tmp: Mutex::new(None),
        file: Mutex::new(None),
        tmp_there: AtomicBool::new(false),
        file_there: AtomicBool::new(false),
        target: target.clone(),
    });
    let values = Arc::clone(&hooks);
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        run(
            client_stream,
            Role::Client,
            &client_config,
            &[SendFile {
                netname: "13b70064.pkt".to_string(),
                path: Some(path),
                size,
                time: 1_000_000_000,
            }],
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let server = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            server_stream,
            Role::Server,
            &server_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
            Duration::from_secs(5),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    assert!(client.is_ok(), "{client:?}");
    let server = server.unwrap();
    // The hook saw the partial file, which exists, and the real name, which
    // does not exist before the rename, the C order of `inb_done()`.
    let tmp = values.tmp.lock().unwrap().clone().expect("no tmp");
    let file = values.file.lock().unwrap().clone().expect("no file");
    assert!(tmp.ends_with(".dt"), "{tmp}");
    assert!(values.tmp_there.load(Ordering::SeqCst), "{tmp}");
    assert!(!values.file_there.load(Ordering::SeqCst), "{file}");
    assert_ne!(tmp, file);
    // The hook changed the real name, and the session renamed the file there.
    assert_eq!(server.received.len(), 1, "{:?}", server.received);
    assert_eq!(server.received[0].path, std::path::PathBuf::from(&target));
    assert_eq!(std::fs::read(&target).unwrap(), expected);
    assert!(!server_dir.join("13b70064.pkt").exists());
}

/// The hooks of the test of the deletion in `after_recv()`: the first hook adds
/// an entry that the session does not send, the second hook deletes it, the
/// port of `refresh_queue()` of `perlhooks.c`.
struct AfterRecvRemoveHooks {
    received: AtomicUsize,
    queue: Mutex<Option<Vec<binkd_proto::QueueFile>>>,
    before_second: Mutex<Option<binkd_proto::SessionVars>>,
    after_session: Mutex<Option<binkd_proto::SessionVars>>,
}

impl binkd_proto::Hooks for AfterRecvRemoveHooks {
    fn before_recv(&self, vars: &binkd_proto::SessionVars, _file: &binkd_proto::FileVars) -> i32 {
        // The second file sees the queue that the first `after_recv()` left.
        if self.received.load(Ordering::SeqCst) == 1 {
            if let Ok(mut slot) = self.before_second.lock() {
                *slot = Some(vars.clone());
            }
        }
        0
    }

    fn after_recv(
        &self,
        _vars: &binkd_proto::SessionVars,
        _file: &mut binkd_proto::ReceivedVars,
    ) -> i32 {
        let n = self.received.fetch_add(1, Ordering::SeqCst);
        let queue = if n == 0 {
            // The entry is marked sent, so the session sends no file, and the
            // queue stays for the second hook.
            vec![binkd_proto::QueueFile {
                file: "/tmp/never.cut".to_string(),
                size: 10,
                time: 1,
                sent: 1,
                flvr: 'f',
                action: '\0',
                type_: '\0',
                addr: address("2:5047/100@fidonet"),
            }]
        } else {
            Vec::new()
        };
        if let Ok(mut slot) = self.queue.lock() {
            *slot = Some(queue);
        }
        0
    }

    fn refresh_queue(&self, _fallback: &FtnAddr) -> Option<Vec<binkd_proto::QueueFile>> {
        self.queue.lock().ok().and_then(|slot| slot.clone())
    }

    fn after_session(&self, vars: &binkd_proto::SessionVars, _ok: bool) {
        if let Ok(mut slot) = self.after_session.lock() {
            *slot = Some(vars.clone());
        }
    }
}

#[tokio::test]
async fn the_hook_removes_a_file_of_after_recv() {
    let client_dir = temp_dir("after-recv-client");
    let server_dir = temp_dir("after-recv-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    let first = vec![b'1'; 100];
    let second = vec![b'2'; 200];
    let first_path = client_dir.join("13b70064.pkt");
    let second_path = client_dir.join("13b70065.pkt");
    std::fs::write(&first_path, &first).unwrap();
    std::fs::write(&second_path, &second).unwrap();
    let sends = vec![
        SendFile {
            netname: "13b70064.pkt".to_string(),
            path: Some(first_path),
            size: first.len() as u64,
            time: 1_000_000_000,
        },
        SendFile {
            netname: "13b70065.pkt".to_string(),
            path: Some(second_path),
            size: second.len() as u64,
            time: 1_000_000_000,
        },
    ];
    let hooks = Arc::new(AfterRecvRemoveHooks {
        received: AtomicUsize::new(0),
        queue: Mutex::new(None),
        before_second: Mutex::new(None),
        after_session: Mutex::new(None),
    });
    let values = Arc::clone(&hooks);
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        run(
            client_stream,
            Role::Client,
            &client_config,
            &sends,
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let server = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            server_stream,
            Role::Server,
            &server_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
            Duration::from_secs(5),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    assert!(client.is_ok(), "{client:?}");
    let server = server.unwrap();
    assert_eq!(server.received.len(), 2, "{:?}", server.received);
    // The first `after_recv()` added the entry and the session refreshed the
    // queue, so the second `before_recv()` sees it.
    let before = values
        .before_second
        .lock()
        .unwrap()
        .clone()
        .expect("the second before_recv saw no values");
    assert_eq!(before.queue.len(), 1, "{before:?}");
    assert_eq!(before.queue[0].file, "/tmp/never.cut");
    // The second `after_recv()` deleted the entry and the session refreshed
    // the queue, so the end of the session holds no queue.
    let after = values
        .after_session
        .lock()
        .unwrap()
        .clone()
        .expect("the session saw no values");
    assert!(after.queue.is_empty(), "{after:?}");
}

/// The hooks of the test of the action of `after_sent()`: the script sets
/// `$action` from the name of the file, the port of `perl_after_sent()`.
struct ActionHooks;

impl binkd_proto::Hooks for ActionHooks {
    fn after_sent(
        &self,
        _vars: &binkd_proto::SessionVars,
        file: &mut binkd_proto::SentVars,
    ) -> i32 {
        file.action = if file.name.contains("delete") {
            'd'
        } else if file.name.contains("truncate") {
            't'
        } else if file.name.contains("kill") {
            's'
        } else {
            '\0'
        };
        1
    }
}

#[tokio::test]
async fn the_action_of_after_sent_changes_the_spool() {
    let client_dir = temp_dir("action-client");
    let server_dir = temp_dir("action-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    let delete = vec![b'd'; 100];
    let truncate = vec![b't'; 200];
    let kill = vec![b'k'; 300];
    let delete_path = client_dir.join("delete.pkt");
    let truncate_path = client_dir.join("truncate.pkt");
    let kill_path = client_dir.join("kill.pkt");
    std::fs::write(&delete_path, &delete).unwrap();
    std::fs::write(&truncate_path, &truncate).unwrap();
    std::fs::write(&kill_path, &kill).unwrap();
    let sends = vec![
        SendFile {
            netname: "delete.pkt".to_string(),
            path: Some(delete_path.clone()),
            size: delete.len() as u64,
            time: 1_000_000_000,
        },
        SendFile {
            netname: "truncate.pkt".to_string(),
            path: Some(truncate_path.clone()),
            size: truncate.len() as u64,
            time: 1_000_000_000,
        },
        SendFile {
            netname: "kill.pkt".to_string(),
            path: Some(kill_path.clone()),
            size: kill.len() as u64,
            time: 1_000_000_000,
        },
    ];
    let hooks = Arc::new(ActionHooks);
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            client_stream,
            Role::Client,
            &client_config,
            &mut |_| sends.clone(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
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
    let client = client.unwrap();
    let server = server.unwrap();
    assert_eq!(client.files_sent, 3);
    assert_eq!(server.received.len(), 3, "{:?}", server.received);
    // `d` removes the file at once.
    assert!(
        !delete_path.exists(),
        "{} still exists",
        delete_path.display()
    );
    // `t` truncates the file.
    assert_eq!(std::fs::metadata(&truncate_path).unwrap().len(), 0);
    // `s` removes the file at the end of a successful session.
    assert!(!kill_path.exists(), "{} still exists", kill_path.display());
}

/// The hooks of the test of the answerer side of `on_handshake()`: the hook
/// runs after the `ADR` of the caller, so it sees the caller's address.
#[derive(Default)]
struct AnswererHandshakeHooks {
    he: Mutex<Option<Vec<FtnAddr>>>,
    traffic: Mutex<Option<(u64, u64)>>,
}

impl binkd_proto::Hooks for AnswererHandshakeHooks {
    fn on_handshake(&self, vars: &mut binkd_proto::Handshake) -> Option<String> {
        if let Ok(mut slot) = self.he.lock() {
            *slot = Some(vars.he.clone());
        }
        if let Ok(mut slot) = self.traffic.lock() {
            *slot = Some((vars.traffic_mail, vars.traffic_file));
        }
        vars.me = Some(vec![address("2:5047/200@fidonet")]);
        vars.password = Some("hook-secret".to_string());
        None
    }
}

#[tokio::test]
async fn the_answerer_runs_on_handshake_after_the_adr() {
    let client_dir = temp_dir("answerer-hook-client");
    let server_dir = temp_dir("answerer-hook-server");
    // The client sends the password that the hook expects. The server config
    // holds another password, so the hook must set the one the check uses.
    let client_config = config(&client_dir, "2:5047/13@fidonet", "hook-secret");
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    let hooks = Arc::new(AnswererHandshakeHooks::default());
    let values = Arc::clone(&hooks);
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
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
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            server_stream,
            Role::Server,
            &server_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
            Duration::from_secs(5),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    let client = client.unwrap();
    let _ = server.unwrap();
    // The hook ran after the `ADR` of the caller, so it saw the caller's
    // address, the C order of `ADR()`.
    let he = values
        .he
        .lock()
        .unwrap()
        .clone()
        .expect("the hook saw no addresses");
    assert_eq!(he.len(), 1, "{he:?}");
    assert_eq!(he[0].format(false), "2:5047/13@fidonet");
    // The hook replaced our address and the password, and the session used
    // both.
    assert!(
        client
            .peer_addresses
            .iter()
            .any(|fa| fa.format(false) == "2:5047/200@fidonet"),
        "{:?}",
        client.peer_addresses
    );
    // The answerer scanned the queue for the caller before the hook, the port
    // of `q_scan_addrs()` of `perl_on_handshake()`.
    let traffic = values
        .traffic
        .lock()
        .unwrap()
        .expect("the hook saw no traffic");
    assert_eq!(traffic, (0, 0));
}

/// The hooks of the test of the two parts of `msg_send2()`: the C code splits
/// some messages and the hook reads `$s1` and `$s2`.
#[derive(Default)]
struct SendSplitHooks {
    calls: Mutex<Vec<(binkd_proto::Command, String, String)>>,
}

impl binkd_proto::Hooks for SendSplitHooks {
    fn on_send(
        &self,
        _vars: &binkd_proto::SessionVars,
        command: binkd_proto::Command,
        s1: &str,
        s2: &str,
    ) {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push((command, s1.to_string(), s2.to_string()));
        }
    }
}

#[tokio::test]
async fn the_hook_sees_the_two_parts_of_the_message() {
    let client_dir = temp_dir("send-split-client");
    let server_dir = temp_dir("send-split-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let sysname = client_config.sysname.clone();
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    let hooks = Arc::new(SendSplitHooks::default());
    let values = Arc::clone(&hooks);
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            client_stream,
            Role::Client,
            &client_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
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
    let calls = values.calls.lock().unwrap();
    assert!(
        calls
            .iter()
            .any(|(command, s1, s2)| *command == binkd_proto::Command::Nul
                && s1 == "SYS "
                && s2 == &sysname),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|(command, s1, s2)| *command == binkd_proto::Command::Nul
                && s1 == "OPT"
                && s2.starts_with(" EXTCMD")),
        "{calls:?}"
    );
}

/// The hooks of the test of the queue of `before_recv()`: the C code does not
/// refresh the queue after the hook, so a change of `@queue` has no effect.
struct BeforeRecvHooks {
    pending: AtomicBool,
    refreshed: AtomicBool,
}

impl binkd_proto::Hooks for BeforeRecvHooks {
    fn before_recv(&self, _vars: &binkd_proto::SessionVars, _file: &binkd_proto::FileVars) -> i32 {
        self.pending.store(true, Ordering::SeqCst);
        0
    }

    fn after_recv(
        &self,
        _vars: &binkd_proto::SessionVars,
        _file: &mut binkd_proto::ReceivedVars,
    ) -> i32 {
        self.pending.store(false, Ordering::SeqCst);
        0
    }

    fn refresh_queue(&self, _fallback: &FtnAddr) -> Option<Vec<binkd_proto::QueueFile>> {
        // The C `perl_before_recv()` does not call `refresh_queue()`, so a
        // refresh while the hook runs is a difference of the port.
        if self.pending.load(Ordering::SeqCst) {
            self.refreshed.store(true, Ordering::SeqCst);
        }
        Some(Vec::new())
    }
}

#[tokio::test]
async fn the_session_does_not_refresh_the_queue_of_before_recv() {
    let client_dir = temp_dir("before-recv-client");
    let server_dir = temp_dir("before-recv-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    let content = vec![b'r'; 100];
    let path = client_dir.join("13b70064.cut");
    std::fs::write(&path, &content).unwrap();
    let sends = vec![SendFile {
        netname: "13b70064.cut".to_string(),
        path: Some(path),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];
    let hooks = Arc::new(BeforeRecvHooks {
        pending: AtomicBool::new(false),
        refreshed: AtomicBool::new(false),
    });
    let values = Arc::clone(&hooks);
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        run(
            client_stream,
            Role::Client,
            &client_config,
            &sends,
            &mut |_| Ok(()),
            Duration::from_secs(5),
        )
        .await
    };
    let server = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            server_stream,
            Role::Server,
            &server_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
            Duration::from_secs(5),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    assert!(client.is_ok(), "{client:?}");
    let server = server.unwrap();
    // The session refreshed the queue only after the hook, the port of the C
    // code, so the change of the hook had no effect.
    assert!(
        !values.refreshed.load(Ordering::SeqCst),
        "the session refreshed the queue of before_recv"
    );
    assert_eq!(server.received.len(), 1, "{:?}", server.received);
    assert_eq!(std::fs::read(&server.received[0].path).unwrap(), content);
}

/// The hooks of the test of the queue of `after_sent()`: the C code does not
/// refresh the queue after the hook, so a change of `@queue` has no effect.
struct AfterSentQueueHooks {
    pending: AtomicBool,
    refreshed: AtomicBool,
    sent: AtomicBool,
}

impl binkd_proto::Hooks for AfterSentQueueHooks {
    fn after_sent(
        &self,
        _vars: &binkd_proto::SessionVars,
        _file: &mut binkd_proto::SentVars,
    ) -> i32 {
        self.pending.store(true, Ordering::SeqCst);
        self.sent.store(true, Ordering::SeqCst);
        0
    }

    fn refresh_queue(&self, _fallback: &FtnAddr) -> Option<Vec<binkd_proto::QueueFile>> {
        // The C `perl_after_sent()` does not call `refresh_queue()`, so a
        // refresh after the hook is a difference of the port.
        if self.pending.load(Ordering::SeqCst) {
            self.refreshed.store(true, Ordering::SeqCst);
        }
        Some(Vec::new())
    }
}

#[tokio::test]
async fn the_session_does_not_refresh_the_queue_of_after_sent() {
    let client_dir = temp_dir("after-sent-client");
    let server_dir = temp_dir("after-sent-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    let content = vec![b's'; 100];
    let path = client_dir.join("13b70064.pkt");
    std::fs::write(&path, &content).unwrap();
    let sends = vec![SendFile {
        netname: "13b70064.pkt".to_string(),
        path: Some(path),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];
    let hooks = Arc::new(AfterSentQueueHooks {
        pending: AtomicBool::new(false),
        refreshed: AtomicBool::new(false),
        sent: AtomicBool::new(false),
    });
    let values = Arc::clone(&hooks);
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            client_stream,
            Role::Client,
            &client_config,
            &mut |_| sends.clone(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
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
    let client = client.unwrap();
    let _ = server.unwrap();
    assert!(values.sent.load(Ordering::SeqCst));
    assert!(
        !values.refreshed.load(Ordering::SeqCst),
        "the session refreshed the queue of after_sent"
    );
    assert_eq!(client.files_sent, 1);
}

/// The hooks of the test of the removal from the queue: the script deletes
/// the other entry of `@queue` in `before_send()`, the port of
/// `refresh_queue()` of `perlhooks.c`.
struct RemoveHooks {
    removed: AtomicBool,
    queue: Mutex<Option<Vec<binkd_proto::QueueFile>>>,
}

impl binkd_proto::Hooks for RemoveHooks {
    fn before_send(
        &self,
        vars: &binkd_proto::SessionVars,
        file: &mut binkd_proto::FileVars,
    ) -> i32 {
        let mut queue = vars.queue.clone();
        if !self.removed.swap(true, Ordering::SeqCst) {
            // The script keeps the file in transfer and deletes every other
            // entry of the queue.
            queue.retain(|entry| entry.file == file.path);
        }
        if let Ok(mut slot) = self.queue.lock() {
            *slot = Some(queue);
        }
        0
    }

    fn refresh_queue(&self, _fallback: &FtnAddr) -> Option<Vec<binkd_proto::QueueFile>> {
        self.queue.lock().ok().and_then(|slot| slot.clone())
    }
}

#[tokio::test]
async fn the_hook_removes_a_file_from_the_session() {
    let client_dir = temp_dir("remove-client");
    let server_dir = temp_dir("remove-server");
    let client_config = config(&client_dir, "2:5047/13@fidonet", "secret");
    let server_config = config(&server_dir, "2:5047/100@fidonet", "secret");
    let keep = vec![b'k'; 100];
    let removed = vec![b'x'; 200];
    let keep_path = client_dir.join("13b70064.pkt");
    let removed_path = client_dir.join("13b70065.pkt");
    std::fs::write(&keep_path, &keep).unwrap();
    std::fs::write(&removed_path, &removed).unwrap();
    let sends = vec![
        SendFile {
            netname: "13b70064.pkt".to_string(),
            path: Some(keep_path),
            size: keep.len() as u64,
            time: 1_000_000_000,
        },
        SendFile {
            netname: "13b70065.pkt".to_string(),
            path: Some(removed_path),
            size: removed.len() as u64,
            time: 1_000_000_000,
        },
    ];
    let hooks = Arc::new(RemoveHooks {
        removed: AtomicBool::new(false),
        queue: Mutex::new(None),
    });
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            client_stream,
            Role::Client,
            &client_config,
            &mut |_| sends.clone(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(hooks),
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
    let client = client.unwrap();
    let server = server.unwrap();
    // The script deleted the second entry, so the session sent one file only.
    assert_eq!(client.files_sent, 1);
    assert_eq!(server.received.len(), 1, "{:?}", server.received);
    let received = std::fs::read(&server.received[0].path).unwrap();
    assert_eq!(received, keep);
    assert_ne!(received, removed);
}

/// The hooks of the test of the compression counters.
#[derive(Default)]
struct CompressionHooks {
    sent: Mutex<Option<binkd_proto::SessionVars>>,
    received: Mutex<Option<binkd_proto::SessionVars>>,
}

impl binkd_proto::Hooks for CompressionHooks {
    fn after_sent(
        &self,
        vars: &binkd_proto::SessionVars,
        _file: &mut binkd_proto::SentVars,
    ) -> i32 {
        if let Ok(mut slot) = self.sent.lock() {
            *slot = Some(vars.clone());
        }
        0
    }

    fn after_recv(
        &self,
        vars: &binkd_proto::SessionVars,
        _file: &mut binkd_proto::ReceivedVars,
    ) -> i32 {
        if let Ok(mut slot) = self.received.lock() {
            *slot = Some(vars.clone());
        }
        0
    }
}

#[tokio::test]
async fn the_compression_counters_reach_the_hooks() {
    let client_dir = temp_dir("compression-client");
    let server_dir = temp_dir("compression-server");
    let client_config = config_with(&client_dir, "2:5047/13@fidonet", "secret", Methods::all());
    let server_config = config_with(&server_dir, "2:5047/100@fidonet", "secret", Methods::all());
    let content = vec![b'z'; 5000];
    let path = client_dir.join("13b70064.pkt");
    std::fs::write(&path, &content).unwrap();
    let sends = vec![SendFile {
        netname: "13b70064.pkt".to_string(),
        path: Some(path),
        size: content.len() as u64,
        time: 1_000_000_000,
    }];
    let client_hooks = Arc::new(CompressionHooks::default());
    let server_hooks = Arc::new(CompressionHooks::default());
    let client_values = Arc::clone(&client_hooks);
    let server_values = Arc::clone(&server_hooks);
    let (client_stream, server_stream) = tokio::io::duplex(1 << 20);
    let client = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            client_stream,
            Role::Client,
            &client_config,
            &mut |_| sends.clone(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(client_hooks),
            Duration::from_secs(2),
        )
        .await
    };
    let server = async move {
        let mut srif = binkd_proto::NoSrif;
        binkd_proto::run_with_hooks(
            server_stream,
            Role::Server,
            &server_config,
            &mut |_| Vec::new(),
            &mut |_| Ok(()),
            &mut srif,
            binkd_proto::SessionInfo::default(),
            Some(server_hooks),
            Duration::from_secs(2),
        )
        .await
    };
    let (client, server) = tokio::join!(client, server);
    assert!(client.is_ok(), "{client:?}");
    assert!(server.is_ok(), "{server:?}");
    let sent = client_values
        .sent
        .lock()
        .unwrap()
        .clone()
        .expect("the send hook saw no values");
    let received = server_values
        .received
        .lock()
        .unwrap()
        .clone()
        .expect("the recv hook saw no values");
    assert!(sent.z_send > 0, "{sent:?}");
    assert_eq!(sent.z_send, received.z_recv);
}
