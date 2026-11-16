//! The loopback test of the network layer.
//!
//! Two instances of the port exchange a packet in each direction, the first
//! exit criterion of the phase. The caller calls the node of the listener, and
//! each side sends the packet of its outbound.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use binkd_config::model::Config;
use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::{DomainTable, FtnDomain};
use binkd_ftn::node::NodeSpec;
use binkd_log::{DateTime, Log};
use binkd_net::{bind, Caller};
use binkd_proto::SendFile;

/// Returns the one file of an extension in an inbound directory.
fn inbound_file(dir: &Path, ext: &str) -> PathBuf {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir.join("in"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .map(|e| e.eq_ignore_ascii_case(ext))
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(found.len(), 1, "the inbound holds {found:?}");
    found.pop().unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("binkd-loopback-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("outb")).unwrap();
    std::fs::create_dir_all(dir.join("in")).unwrap();
    dir
}

fn address(s: &str, domains: &DomainTable) -> FtnAddr {
    FtnAddr::parse(s, domains).unwrap()
}

/// Answers no free space for the provider test.
fn no_space(_: &Path) -> u64 {
    0
}

fn time() -> DateTime {
    DateTime {
        epoch: 1_000_000_000,
        year: 2001,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
        offset_minutes: 0,
    }
}

/// The settings of one side of the test.
fn config(dir: &Path, aka: &str) -> Config {
    let mut config = Config::new();
    config.spool.domains = DomainTable::new(vec![FtnDomain {
        name: "fidonet".to_string(),
        path: dir.display().to_string(),
        dir: "outb".to_string(),
        z: [2, -1],
        idomain: None,
        alias4: None,
    }]);
    config.spool.aka = vec![address(aka, &config.spool.domains)];
    config.inbound = dir.join("in").display().to_string();
    config.oport = "binkp".to_string();
    config
}

/// Writes the packet of a side and queues it for the node.
///
/// The scan of the queue reads the outbound of the domain and gives the
/// flavours to the node of the receiver, the port of `q_scan()`.
fn packet(config: &mut Config, name: &str, content: &[u8], _node: &FtnAddr, now: i64) {
    let dir = PathBuf::from(&config.spool.domains.last().unwrap().path);
    let path = dir.join("outb").join(name);
    std::fs::write(&path, content).unwrap();
    let _queue = binkd_spool::queue::Queue::scan(&mut config.spool, now);
}

/// The free-space provider of the options reaches the listener session: the
/// server declines the file when the provider answers less than `minfree`.
#[tokio::test]
async fn the_free_space_provider_of_the_options_reaches_the_session() {
    use binkd_net::SessionOptions;
    use std::sync::Mutex;

    let server_dir = temp_dir("server-free-space");
    let client_dir = temp_dir("client-free-space");
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = time;
    let mut client_config = config(&client_dir, "2:5047/13@fidonet");
    let mut server_config = config(&server_dir, "2:5047/100@fidonet");
    server_config.spool.minfree = 1;
    server_config.spool.minfree_nonsecure = 1;

    let server_aka = address("2:5047/100@fidonet", &server_config.spool.domains);
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    server_config.listen = vec![binkd_config::model::ListenChain {
        addr: "127.0.0.1".to_string(),
        port: port.to_string(),
    }];
    client_config.spool.nodes.add(
        &server_aka,
        &NodeSpec {
            hosts: Some(format!("127.0.0.1:{port}")),
            ..NodeSpec::default()
        },
        now().epoch,
    );
    let content = vec![b'f'; 300];
    packet(
        &mut client_config,
        "13b70064.cut",
        &content,
        &server_aka,
        now().epoch,
    );
    let client_file = SendFile {
        netname: "13b70064.cut".to_string(),
        path: Some(client_dir.join("outb/13b70064.cut")),
        size: content.len() as u64,
        time: 1_000_000_000,
    };

    let received = Arc::new(Mutex::new(Vec::new()));
    let server_received = Arc::clone(&received);
    let server_log = Arc::clone(&log);
    let server = tokio::spawn(binkd_net::listen(
        server_config,
        server_log,
        now,
        SessionOptions::new(Duration::from_secs(5), true, true).with_free_space(no_space),
        |_| Vec::new(),
        move |file| {
            if let Ok(mut list) = server_received.lock() {
                list.push(file.clone());
            }
            Ok(())
        },
    ));
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(5)
        && tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_err()
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut caller = Caller::new();
    let result = caller
        .call_next(
            &mut client_config,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(5), true, true),
            &|_| vec![client_file.clone()],
        )
        .await;
    let report = result
        .expect("the caller failed")
        .expect("the session failed");
    assert_eq!(report.files_sent, 0, "{report:?}");
    assert!(received.lock().unwrap().is_empty());
    server.abort();
}

#[tokio::test]
async fn two_instances_exchange_a_packet() {
    let server_dir = temp_dir("server");
    let client_dir = temp_dir("client");
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = time;
    let mut client_config = config(&client_dir, "2:5047/13@fidonet");
    let mut server_config = config(&server_dir, "2:5047/100@fidonet");

    let server_aka = address("2:5047/100@fidonet", &server_config.spool.domains);
    let client_aka = address("2:5047/13@fidonet", &client_config.spool.domains);

    // The listener of the server.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    server_config.listen = vec![binkd_config::model::ListenChain {
        addr: "127.0.0.1".to_string(),
        port: port.to_string(),
    }];
    // The node of the caller, with the host of the listener.
    client_config.spool.nodes.add(
        &server_aka,
        &NodeSpec {
            hosts: Some(format!("127.0.0.1:{port}")),
            ..NodeSpec::default()
        },
        now().epoch,
    );

    // The packet of each side.
    let client_packet = vec![b'c'; 300];
    let server_packet = vec![b's'; 400];
    packet(
        &mut client_config,
        "13b70064.cut",
        &client_packet,
        &server_aka,
        now().epoch,
    );
    packet(
        &mut server_config,
        "13b7000d.cut",
        &server_packet,
        &client_aka,
        now().epoch,
    );

    let server_files = vec![SendFile {
        netname: "13b7000d.cut".to_string(),
        path: Some(server_dir.join("outb/13b7000d.cut")),
        size: server_packet.len() as u64,
        time: 1_000_000_000,
    }];
    let client_files = vec![SendFile {
        netname: "13b70064.cut".to_string(),
        path: Some(client_dir.join("outb/13b70064.cut")),
        size: client_packet.len() as u64,
        time: 1_000_000_000,
    }];

    // The listener of the server runs until the session ends.
    let server_log = Arc::clone(&log);
    let server_files = server_files.clone();
    let server = async move {
        let listeners = bind(&server_config, &server_log, &now).await.unwrap();
        let listener = listeners.into_iter().next().unwrap();
        let (stream, _) = listener.accept().await.unwrap();
        let settings = binkd_net::session_config(&server_config, None, true, true, &time());
        let mut received: Vec<binkd_proto::ReceivedFile> = Vec::new();
        let result = binkd_proto::run(
            stream,
            binkd_proto::Role::Server,
            &settings,
            &server_files,
            &mut |file| {
                received.push(file.clone());
                Ok(())
            },
            Duration::from_secs(5),
        )
        .await;
        assert!(result.is_ok(), "{result:?}");
        received
    };

    let client = async {
        let mut caller = Caller::new();
        let result = caller
            .call_next(
                &mut client_config,
                &log,
                &now,
                binkd_net::SessionOptions::new(Duration::from_secs(5), true, true),
                &|_| client_files.clone(),
            )
            .await;
        result
    };

    let (server_received, client_result) = tokio::join!(server, client);
    let report = client_result
        .expect("the caller failed")
        .expect("the session failed");
    assert_eq!(report.files_sent, 1, "{report:?}");
    assert_eq!(report.files_received, 1, "{report:?}");

    // Each side wrote the packet of the other. The C code announces a file
    // of the mail with a random name, the port of `netname_()` of `prothlp.c`,
    // so a test takes the file of an inbound by its extension.
    assert_eq!(
        std::fs::read(inbound_file(&server_dir, "pkt")).unwrap(),
        client_packet
    );
    assert_eq!(
        std::fs::read(inbound_file(&client_dir, "pkt")).unwrap(),
        server_packet
    );
    assert_eq!(server_received.len(), 1);
}
