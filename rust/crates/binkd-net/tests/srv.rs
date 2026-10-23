//! The SRV lookup of the port against a server of the test.
//!
//! The C code asks the resolver library of the system, so the C test suite
//! cannot reach it. The port reads the settings of the resolver itself, so the
//! test supplies its own server and exercises the query, the answer, and the
//! fallback.

use std::time::Duration;

use binkd_net::dns::{encode_name, Resolver, CLASS_IN, TYPE_SRV};
use binkd_net::srv::{srv_getaddrinfo_with, Family, Hints, SockType};
use tokio::net::UdpSocket;

/// Returns the length of the question of a query, after the header.
fn question_len(request: &[u8]) -> usize {
    let mut at = 12;
    loop {
        let length = request[at] as usize;
        at += 1;
        if length == 0 {
            break;
        }
        at += length;
    }
    at + 4 - 12
}

/// Returns the answer of a query: the header, the question of the query, and the
/// records of the test.
fn answer(request: &[u8], records: &[Vec<u8>]) -> Vec<u8> {
    let mut reply = request[..12].to_vec();
    // The flags hold the answer and the recursion, the port of `RES_RECURSE`.
    reply[2] = 0x81;
    reply[3] = 0x80;
    reply[6] = (records.len() >> 8) as u8;
    reply[7] = records.len() as u8;
    let question = question_len(request);
    reply.extend_from_slice(&request[12..12 + question]);
    for record in records {
        reply.extend_from_slice(record);
    }
    reply
}

/// Returns an SRV record that points at the name of the question.
fn srv_record(port: u16, target: &str) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0u16.to_be_bytes());
    data.extend_from_slice(&0u16.to_be_bytes());
    data.extend_from_slice(&port.to_be_bytes());
    encode_name(&mut data, target);
    let mut record = vec![0xc0, 0x0c];
    record.extend_from_slice(&TYPE_SRV.to_be_bytes());
    record.extend_from_slice(&CLASS_IN.to_be_bytes());
    record.extend_from_slice(&300u32.to_be_bytes());
    record.extend_from_slice(&(data.len() as u16).to_be_bytes());
    record.extend_from_slice(&data);
    record
}

/// Starts a server of the test that answers one query with the records.
async fn start(records: Vec<Vec<u8>>) -> Resolver {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server = socket.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 512];
        let Ok((len, peer)) = socket.recv_from(&mut buf).await else {
            return;
        };
        let reply = answer(&buf[..len], &records);
        let _ = socket.send_to(&reply, peer).await;
    });
    Resolver {
        nameservers: vec![server.ip()],
        port: server.port(),
        search: Vec::new(),
        ndots: 1,
        timeout: Duration::from_secs(5),
        attempts: 1,
    }
}

fn hints() -> Hints {
    Hints {
        family: Family::V4,
        socktype: SockType::Stream,
        ..Hints::default()
    }
}

/// The lookup of a name answers the target of the record.
#[tokio::test]
async fn the_target_of_a_record() {
    let resolver = start(vec![srv_record(24555, "localhost")]).await;
    let addrs = srv_getaddrinfo_with("example.org", "binkp", &hints(), &resolver)
        .await
        .unwrap();
    assert_eq!(addrs.len(), 1, "{addrs:?}");
    assert_eq!(addrs[0].port(), 24555);
    assert!(addrs[0].ip().is_loopback(), "{addrs:?}");
}

/// The port of the record wins over the service of the query.
#[tokio::test]
async fn the_port_of_a_record() {
    let resolver = start(vec![srv_record(2000, "localhost")]).await;
    let addrs = srv_getaddrinfo_with("example.org", "binkp", &hints(), &resolver)
        .await
        .unwrap();
    assert_eq!(addrs[0].port(), 2000);
}

/// Every record of the answer adds its own target, in the order of the answer.
#[tokio::test]
async fn the_order_of_the_answer() {
    let resolver = start(vec![
        srv_record(2001, "localhost"),
        srv_record(2002, "127.0.0.1"),
    ])
    .await;
    let addrs = srv_getaddrinfo_with("example.org", "binkp", &hints(), &resolver)
        .await
        .unwrap();
    assert_eq!(addrs.len(), 2, "{addrs:?}");
    assert_eq!(addrs[0].port(), 2001);
    assert_eq!(addrs[1].port(), 2002);
}

/// A record of another type asks for no target.
#[tokio::test]
async fn the_record_of_another_type() {
    let mut record = srv_record(2001, "localhost");
    // The type of the record becomes the type of an address.
    record[2] = 0;
    record[3] = 1;
    let resolver = start(vec![record]).await;
    let addrs = srv_getaddrinfo_with("localhost", "binkp", &hints(), &resolver)
        .await
        .unwrap();
    // The answer holds no SRV record, so the plain resolution answers.
    assert!(addrs.iter().any(|addr| addr.port() == 24554), "{addrs:?}");
}

/// An empty answer falls back to the plain resolution of the C code.
#[tokio::test]
async fn the_fallback_of_an_empty_answer() {
    let resolver = start(Vec::new()).await;
    let addrs = srv_getaddrinfo_with("localhost", "binkp", &hints(), &resolver)
        .await
        .unwrap();
    // The system resolves `localhost`, and the port of the service is 24554.
    assert!(
        addrs.iter().any(|addr| addr.ip().is_loopback()),
        "{addrs:?}"
    );
    assert!(addrs.iter().all(|addr| addr.port() == 24554), "{addrs:?}");
}

/// The guards of the C code answer a plain resolution without a query.
#[tokio::test]
async fn the_guards_of_the_plain_resolution() {
    // The server of the test holds no record: a query would hold no answer.
    let resolver = start(Vec::new()).await;
    // A numeric service asks for no SRV lookup.
    let addrs = srv_getaddrinfo_with("localhost", "24554", &hints(), &resolver)
        .await
        .unwrap();
    assert!(addrs.iter().all(|addr| addr.port() == 24554), "{addrs:?}");
    // An address literal asks for no SRV lookup.
    let addrs = srv_getaddrinfo_with("127.0.0.1", "binkp", &hints(), &resolver)
        .await
        .unwrap();
    assert!(
        addrs.iter().any(|addr| addr.ip().is_loopback()),
        "{addrs:?}"
    );
}
