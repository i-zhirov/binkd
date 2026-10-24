//! The SRIF events of a session, the port of the `flag` and `exec` directives
//! with the spool, the external program, and the listener and the caller of the
//! network layer.
//!
//! The test runs the caller and the listener of the port against each other. The
//! listener holds an `exec` entry with the `*S` macro, so the SRIF exchange runs
//! the external program of the test, the port of an external FREQ processor, and the
//! caller receives the file that the program answered.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use binkd_config::model::{Config, EvtFlag};
use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::{DomainTable, FtnDomain};
use binkd_ftn::node::NodeSpec;
use binkd_log::{DateTime, Log};
use binkd_net::{listen, Caller, SessionOptions};
use binkd_proto::SendFile;

/// Makes a directory for the spool of a test.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("binkd-srif-net-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("outb")).unwrap();
    std::fs::create_dir_all(dir.join("in")).unwrap();
    dir
}

fn address(s: &str, domains: &DomainTable) -> FtnAddr {
    FtnAddr::parse(s, domains).unwrap()
}

fn time() -> DateTime {
    DateTime {
        epoch: 1_000_000_000,
        day: 9,
        month: 9,
        hour: 1,
        minute: 46,
        second: 40,
    }
}

/// The settings of the port of a test.
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

/// The listener runs the external program of an `exec` entry and the caller
/// receives the file of the exchange.
#[tokio::test]
async fn the_exchange_of_a_session() {
    let server_dir = temp_dir("server");
    let client_dir = temp_dir("client");
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };

    // The caller holds a request file for the listener. The name of the file
    // holds the address of the listener, the port of the scan of `q_scan()`.
    //
    // A request file holds the flavour of the hold, and the C code calls the
    // node of such a flavour only for a poll. The caller holds a mail packet
    // for the listener as well, so the scan of the spool answers a flavour that
    // the caller may call.
    let request = vec![b'q'; 100];
    let request_path = client_dir.join("outb").join("13b70064.req");
    std::fs::write(&request_path, &request).unwrap();
    std::fs::write(client_dir.join("outb").join("13b70064.cut"), b"the mail").unwrap();

    // The answer of the exchange is the file of the caller, and the external
    // FREQ processor of the listener answers it.
    let answer = server_dir.join("outb").join("13b7000d.pkt");
    std::fs::write(&answer, b"the answer").unwrap();
    let script = server_dir.join("freq.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '=%s\\n' '{}' > \"${{1%.srf}}.rsp\"\n",
            answer.display()
        ),
    )
    .unwrap();

    // The listener holds an `exec` entry of the SRIF interface.
    let mut server_config = config(&server_dir, "2:5047/100@fidonet");
    server_config.evt_flags.push(EvtFlag {
        path: None,
        // The C code runs the program of the entry with the shell, so the
        // program needs the right of execution. The test runs the script with
        // the shell of the port, which reads it as a command.
        command: Some(format!("sh {} *S", script.display())),
        pattern: "*.req".to_string(),
        imm: true,
    });
    // The spool of the answerer needs the node of the caller, the port of
    // `q_add_file()` for a file that the caller sends.
    let caller_aka = address("2:5047/13@fidonet", &server_config.spool.domains);
    server_config.spool.nodes.add(
        &caller_aka,
        &NodeSpec {
            hosts: Some("127.0.0.1".to_string()),
            pwd: Some("secret".to_string()),
            ..NodeSpec::default()
        },
        time().epoch,
    );
    server_config.listen = vec![binkd_config::model::ListenChain {
        addr: "127.0.0.1".to_string(),
        port: port.to_string(),
    }];

    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = time;
    let server_log = Arc::clone(&log);
    let server = tokio::spawn(listen(
        server_config,
        server_log,
        now,
        SessionOptions::new(Duration::from_secs(20), true, true),
        |_| Vec::new(),
        |_| Ok(()),
    ));
    // The probe of the port is a session that fails, and it proves that the
    // listener answers on the port.
    assert!(
        {
            let mut started = false;
            for _ in 0..50 {
                if tokio::net::TcpStream::connect(("127.0.0.1", port))
                    .await
                    .is_ok()
                {
                    started = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            started
        },
        "the listener did not start"
    );

    // The caller calls the listener and sends the request file.
    let mut client_config = config(&client_dir, "2:5047/13@fidonet");
    let server_aka = address("2:5047/100@fidonet", &client_config.spool.domains);
    client_config.spool.nodes.add(
        &server_aka,
        &NodeSpec {
            hosts: Some(format!("127.0.0.1:{port}")),
            pwd: Some("secret".to_string()),
            ..NodeSpec::default()
        },
        now().epoch,
    );
    let mut caller = Caller::new();
    let result = caller
        .call_next(
            &mut client_config,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(20), true, true),
            &|_| {
                // The caller sends the request file of the exchange.
                vec![SendFile {
                    netname: "13b70064.req".to_string(),
                    path: Some(request_path.clone()),
                    size: request.len() as u64,
                    time: 1_000_000_000,
                }]
            },
        )
        .await;
    let report = result
        .expect("the caller found no node")
        .expect("the session failed");
    tokio::time::sleep(Duration::from_millis(500)).await;
    server.abort();

    // The external program ran: the listener wrote the request file for it, the
    // port of `srif_fill()`, and the C code deletes the file of the request
    // after the exchange, the port of `evt_run()`.
    assert_eq!(request.len(), 100);
    assert!(!server_dir.join("in").join("13b70064.req").is_file());
    // The listener deleted the two files of the exchange as well.
    assert!(!server_dir.join("outb").join("13b7000d.srf").is_file());
    assert!(!server_dir.join("outb").join("13b7000d.rsp").is_file());
    // The caller received the file of the exchange, the port of the answer of
    // the external program.
    assert_eq!(report.files_sent, 1, "{report:?}");
    assert_eq!(report.files_received, 1, "{report:?}");
    assert_eq!(report.received[0].netname, "13b7000d.pkt");
    assert_eq!(
        std::fs::read(&report.received[0].path).unwrap(),
        b"the answer"
    );
}
