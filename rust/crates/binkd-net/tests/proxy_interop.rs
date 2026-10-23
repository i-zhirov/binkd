//! The interop matrix of the proxy support.
//!
//! The tests run the C binary and the caller of the port against a proxy server
//! of the test over the loopback address. They need the C binary of the proxy:
//! set `BINKD_C_PROXY` to its path. Without the variable the tests report that
//! they are skipped. The build needs the `--with-proxy` and the `--with-ntlm`
//! switches of `configure`.
//!
//! The matrix:
//!
//! | Client | Proxy | Test |
//! | --- | --- | --- |
//! | Rust | the test | HTTP CONNECT, HTTP basic, SOCKS4, SOCKS5 |
//! | C | the test | HTTP CONNECT, and the bytes match the port |

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use binkd_config::model::Config;
use binkd_ftn::addr::FtnAddr;
use binkd_ftn::domain::{DomainTable, FtnDomain};
use binkd_ftn::node::NodeSpec;
use binkd_log::{DateTime, Log};
use binkd_net::{Caller, SessionOptions};
use binkd_proto::SendFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// The number of the bytes of the packet of a test.
const PACKET_SIZE: usize = 300;

/// The address of the caller of the tests.
const CLIENT_AKA: &str = "2:5047/13@fidonet";
/// The address of the server of the tests.
const SERVER_AKA: &str = "2:5047/100@fidonet";

/// The name of the file that the caller sends, the address of the server.
const CLIENT_FILE: &str = "13b70064.cut";
/// The name of the file that the server sends, the address of the caller.
const SERVER_FILE: &str = "13b7000d.cut";

/// The credentials of the proxy of the tests, `user/password`.
const AUTH: &str = "user/password";
/// The credentials of the NTLM proxy, `user/password/host/domain`.
const NTLM_AUTH: &str = "user/password/HOST/Domain";

/// The challenge of the NTLM proxy of the tests, the bytes 24 to 32.
const CHALLENGE: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];

/// Returns the path of the C binary of the proxy, or `None` without the variable.
///
/// The build of the binary needs the `--with-proxy` and the `--with-ntlm` switches
/// of `configure`: the plain build of the corpus tests holds no proxy code.
fn c_binary() -> Option<String> {
    std::env::var("BINKD_C_PROXY")
        .ok()
        .filter(|p| Path::new(p).is_file())
}

/// Returns a port of the process.
fn free_port() -> u16 {
    use std::sync::atomic::{AtomicU16, Ordering};

    static NEXT: AtomicU16 = AtomicU16::new(0);
    let base = 21000 + (std::process::id() % 20000) as u16;
    let offset = NEXT.fetch_add(1, Ordering::Relaxed);
    base.wrapping_add(offset % 30000).max(1024)
}

/// Makes a directory for the spool of a test.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("binkd-proxy-{}-{name}", std::process::id()));
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

/// Writes the packet of a side and queues it for the node.
fn packet(config: &mut Config, name: &str, content: &[u8], now: i64) {
    let dir = PathBuf::from(&config.spool.domains.last().unwrap().path);
    std::fs::write(dir.join("outb").join(name), content).unwrap();
    let _ = binkd_spool::queue::Queue::scan(&mut config.spool, now);
}

/// Writes the config of the C side.
fn c_config(dir: &Path, own: &str, node: &str) {
    let config = format!(
        "domain fidonet outb 2\n\
         address {own}\n\
         sysname \"C side\"\n\
         sysop \"C sysop\"\n\
         location \"C location\"\n\
         nodeinfo \"C node\"\n\
         inbound in\n\
         log c.log\n\
         loglevel 10\n\
         conlog 0\n\
         node {node}\n\
         zallow *\n"
    );
    std::fs::write(dir.join("binkd.conf"), config).unwrap();
}

/// Starts the C server of a spool, the port of `binkd -s`.
fn start_c_server(dir: &Path, port: u16, peer: &str) -> Child {
    c_config(dir, SERVER_AKA, &format!("{peer} 127.0.0.1 secret"));
    let mut config = std::fs::read_to_string(dir.join("binkd.conf")).unwrap();
    config.push_str(&format!("iport {port}\nlisten 127.0.0.1:{port}\n"));
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

/// Runs the C client once, the port of `binkd -P <node> -p`.
fn run_c_client(dir: &Path, node: &str) {
    let client = Command::new(c_binary().unwrap())
        .args(["-P", node, "-p", "binkd.conf"])
        .current_dir(dir)
        .status()
        .expect("cannot run the C client");
    assert!(client.success(), "the C client failed");
}

/// Waits until a listener answers on the port.
async fn wait_for_port(port: u16) -> bool {
    for _ in 0..50 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// The protocol of the test proxy server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Protocol {
    /// The HTTP CONNECT proxy without auth.
    Http,
    /// The HTTP CONNECT proxy with the basic auth.
    HttpBasic,
    /// The HTTP CONNECT proxy with the NTLM auth.
    HttpNtlm,
    /// The SOCKS4 proxy.
    Socks4,
    /// The SOCKS5 proxy without auth.
    Socks5,
    /// The SOCKS5 proxy with the user and the password.
    Socks5Auth,
}

/// The proxy server of the tests, the peer of the client.
struct TestProxy {
    /// The address of the proxy server.
    addr: SocketAddr,
    /// The bytes of the requests that the server received.
    requests: Arc<Mutex<Vec<Vec<u8>>>>,
}

/// Starts a proxy server that accepts `connections` connections and relays
/// every one of them to the target.
///
/// The server reads the request of the protocol, records it, answers it, and
/// copies the bytes of the two sides after that: the C code holds such a server
/// on the far side of the tunnel.
fn start_proxy(protocol: Protocol, target: SocketAddr, connections: usize) -> TestProxy {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let records = Arc::clone(&requests);
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        for _ in 0..connections {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let records = Arc::clone(&records);
            tokio::spawn(async move {
                let _ = serve(protocol, &mut stream, &records, target).await;
            });
        }
    });
    TestProxy { addr, requests }
}

/// Serves one connection of the test proxy server.
async fn serve(
    protocol: Protocol,
    stream: &mut TcpStream,
    requests: &Arc<Mutex<Vec<Vec<u8>>>>,
    target: SocketAddr,
) -> std::io::Result<()> {
    match protocol {
        Protocol::Http | Protocol::HttpBasic => {
            let request = read_request(stream).await?;
            requests.lock().unwrap().push(request);
            stream
                .write_all(b"HTTP/1.0 200 Connection established\r\n\r\n")
                .await?;
        }
        Protocol::HttpNtlm => {
            let request = read_request(stream).await?;
            requests.lock().unwrap().push(request);
            // The answer of the challenge: the header and the Type 2 message
            // with the nonce at the offset 24, the port of `getNTLM2()`.
            let token = binkd_net::base64::encode(&type2(&CHALLENGE));
            let reply = format!(
                "HTTP/1.1 407 Proxy Authentication Required\r\n\
                 Proxy-Authenticate: NTLM {token}\r\n\
                 Content-Length: 0\r\n\r\n"
            );
            stream.write_all(reply.as_bytes()).await?;
            let request = read_request(stream).await?;
            requests.lock().unwrap().push(request);
            stream
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await?;
        }
        Protocol::Socks4 => {
            let mut request = [0u8; 9];
            stream.read_exact(&mut request).await?;
            requests.lock().unwrap().push(request.to_vec());
            stream.write_all(&[0, 90, 0, 0, 0, 0, 0, 0]).await?;
        }
        Protocol::Socks5 | Protocol::Socks5Auth => {
            // The greeting of the client: the version, the number of the
            // methods, and the methods.
            let mut greeting = vec![0u8; 3];
            stream.read_exact(&mut greeting).await?;
            let mut request = greeting;
            if protocol == Protocol::Socks5Auth {
                let mut method = [0u8; 1];
                stream.read_exact(&mut method).await?;
                request.push(method[0]);
            }
            requests.lock().unwrap().push(request);
            stream
                .write_all(&[5, if protocol == Protocol::Socks5 { 0 } else { 2 }])
                .await?;
            if protocol == Protocol::Socks5Auth {
                // The user and the password, the port of RFC 1929: the
                // version, the length of the user, the user, the length of the
                // password, and the password.
                let mut auth = vec![0u8; 1];
                stream.read_exact(&mut auth).await?;
                let mut length = [0u8; 1];
                stream.read_exact(&mut length).await?;
                let mut user = vec![0u8; length[0] as usize];
                stream.read_exact(&mut user).await?;
                auth.extend_from_slice(&length);
                auth.extend_from_slice(&user);
                stream.read_exact(&mut length).await?;
                let mut password = vec![0u8; length[0] as usize];
                stream.read_exact(&mut password).await?;
                auth.extend_from_slice(&length);
                auth.extend_from_slice(&password);
                requests.lock().unwrap().push(auth);
                stream.write_all(&[1, 0]).await?;
            }
            // The request of the connection: the header and the address.
            let mut request = vec![0u8; 4];
            stream.read_exact(&mut request).await?;
            let rest = match request[3] {
                1 => 6,
                3 => {
                    let mut len = [0u8; 1];
                    stream.read_exact(&mut len).await?;
                    len[0] as usize + 2
                }
                4 => 18,
                _ => 0,
            };
            let mut tail = vec![0u8; rest];
            stream.read_exact(&mut tail).await?;
            request.extend_from_slice(&tail);
            requests.lock().unwrap().push(request);
            stream.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        }
    }
    let mut server = TcpStream::connect(target).await?;
    tokio::io::copy_bidirectional(stream, &mut server).await?;
    Ok(())
}

/// Reads the header block of a request.
async fn read_request(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::new();
    loop {
        request.push(stream.read_u8().await?);
        if request.ends_with(b"\r\n\r\n") || request.ends_with(b"\n\n") {
            return Ok(request);
        }
    }
}

/// Returns the Type 2 message of a challenge, the peer of `getNTLM2()`.
fn type2(challenge: &[u8; 8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(32);
    message.extend_from_slice(b"NTLMSSP\0");
    message.extend_from_slice(&[2, 0, 0, 0]);
    message.extend_from_slice(&[0, 0, 0, 0]);
    message.extend_from_slice(&[0, 0, 0, 0]);
    message.extend_from_slice(&[0x01, 0x02, 0x00, 0x00]);
    message.extend_from_slice(challenge);
    message
}

/// The caller of the port calls the C listener through the proxy of the tests.
///
/// The function answers the requests that the proxy received.
async fn rust_caller_through_proxy(
    name: &str,
    protocol: Protocol,
    proxy: impl Fn(&TestProxy) -> String,
    expected: impl Fn(&TestProxy, u16) -> Vec<Vec<u8>>,
) -> Vec<Vec<u8>> {
    let server_dir = temp_dir(&format!("{name}-server"));
    let client_dir = temp_dir(&format!("{name}-client"));
    let port = free_port();
    let mut server = start_c_server(&server_dir, port, CLIENT_AKA);
    assert!(wait_for_port(port).await, "the C server did not start");
    let server_packet = vec![b's'; PACKET_SIZE];
    std::fs::write(server_dir.join("outb").join(SERVER_FILE), &server_packet).unwrap();

    let test_proxy = start_proxy(protocol, format!("127.0.0.1:{port}").parse().unwrap(), 1);
    let log_path = client_dir.join("client.log");
    let log = Arc::new(Log::new(10, 0, log_path.to_str().unwrap(), Vec::new()));
    let now = time;
    let mut client_config = config(&client_dir, CLIENT_AKA);
    let server_aka = address(SERVER_AKA, &client_config.spool.domains);
    client_config.spool.nodes.add(
        &server_aka,
        &NodeSpec {
            hosts: Some(format!("127.0.0.1:{port}")),
            pwd: Some("secret".to_string()),
            ..NodeSpec::default()
        },
        now().epoch,
    );
    let client_packet = vec![b'c'; PACKET_SIZE];
    packet(&mut client_config, CLIENT_FILE, &client_packet, now().epoch);
    // The C code reads the two directives, `proxy` and `socks`.
    let value = proxy(&test_proxy);
    match protocol {
        Protocol::Socks4 | Protocol::Socks5 | Protocol::Socks5Auth => client_config.socks = value,
        _ => client_config.proxy = value,
    }

    let mut caller = Caller::new();
    let result = caller
        .call_next(
            &mut client_config,
            &log,
            &now,
            SessionOptions::new(Duration::from_secs(20), true, true),
            &|_| {
                vec![SendFile {
                    netname: CLIENT_FILE.to_string(),
                    path: Some(client_dir.join("outb").join(CLIENT_FILE)),
                    size: client_packet.len() as u64,
                    time: 1_000_000_000,
                }]
            },
        )
        .await;
    let report = match result.expect("the caller found no node") {
        Ok(report) => report,
        Err(e) => {
            let log = std::fs::read_to_string(client_dir.join("client.log")).unwrap_or_default();
            panic!("the session failed: {e:?}\n{log}");
        }
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = server.kill();
    let _ = server.wait();

    // The session of the port sent the packet of its outbound and received the
    // packet of the C server through the tunnel.
    assert_eq!(report.files_sent, 1, "{report:?}");
    assert_eq!(report.files_received, 1, "{report:?}");
    assert_eq!(
        std::fs::read(server_dir.join("in").join(CLIENT_FILE)).unwrap(),
        client_packet
    );
    assert_eq!(
        std::fs::read(&report.received[0].path).unwrap(),
        server_packet
    );

    let requests = test_proxy.requests.lock().unwrap().clone();
    assert_eq!(
        requests,
        expected(&test_proxy, port),
        "{}",
        String::from_utf8_lossy(&requests[0])
    );
    requests
}

/// The caller of the port runs through the HTTP CONNECT proxy of the tests.
#[tokio::test]
async fn rust_caller_via_http_proxy() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C_PROXY to the path of the C binary of the proxy");
        return;
    };
    rust_caller_through_proxy(
        "rust-http",
        Protocol::Http,
        |proxy| proxy.addr.to_string(),
        |_, port| vec![format!("CONNECT 127.0.0.1:{port} HTTP/1.0\r\n\r\n").into_bytes()],
    )
    .await;
}

/// The caller of the port runs through the proxy of the tests with the basic
/// auth.
#[tokio::test]
async fn rust_caller_via_http_proxy_auth() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C_PROXY to the path of the C binary of the proxy");
        return;
    };
    rust_caller_through_proxy(
        "rust-http-auth",
        Protocol::HttpBasic,
        |proxy| format!("{}/{AUTH}", proxy.addr),
        |_, port| {
            vec![format!(
                "CONNECT 127.0.0.1:{port} HTTP/1.0\r\n\
                     Proxy-Authorization: basic dXNlcjpwYXNzd29yZA==\r\n\r\n"
            )
            .into_bytes()]
        },
    )
    .await;
}

/// The caller of the port runs through the proxy of the tests with the NTLM
/// auth.
///
/// The expected requests hold the Type 1 and the Type 3 messages of the port
/// itself, so the test compares the two sides of the port: the messages of the
/// handshake and the messages of the C code, which the proxy of the C interop
/// test holds.
#[tokio::test]
async fn rust_caller_via_http_proxy_ntlm() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C_PROXY to the path of the C binary of the proxy");
        return;
    };
    rust_caller_through_proxy(
        "rust-http-ntlm",
        Protocol::HttpNtlm,
        |proxy| format!("{}/{NTLM_AUTH}", proxy.addr),
        |_, port| {
            let type1 = binkd_net::ntlm::get_ntlm1(NTLM_AUTH).unwrap();
            let type3 = binkd_net::ntlm::get_ntlm2(
                NTLM_AUTH,
                &binkd_net::base64::encode(&type2(&CHALLENGE)),
            )
            .unwrap();
            vec![
                format!(
                    "CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\
                     Connection: keep-alive\r\n\
                     Proxy-Authorization: NTLM {type1}\r\n\r\n"
                )
                .into_bytes(),
                format!(
                    "CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\
                     Proxy-Authorization: NTLM {type3}\r\n\r\n"
                )
                .into_bytes(),
            ]
        },
    )
    .await;
}

/// The caller of the port runs through the SOCKS4 proxy of the tests.
#[tokio::test]
async fn rust_caller_via_socks4_proxy() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C_PROXY to the path of the C binary of the proxy");
        return;
    };
    rust_caller_through_proxy(
        "rust-socks4",
        Protocol::Socks4,
        |proxy| proxy.addr.to_string(),
        |_, port| {
            let mut request = vec![4, 1];
            request.extend_from_slice(&port.to_be_bytes());
            request.extend_from_slice(&[127, 0, 0, 1, 0]);
            vec![request]
        },
    )
    .await;
}

/// The caller of the port runs through the SOCKS5 proxy of the tests.
#[tokio::test]
async fn rust_caller_via_socks5_proxy() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C_PROXY to the path of the C binary of the proxy");
        return;
    };
    rust_caller_through_proxy(
        "rust-socks5",
        Protocol::Socks5,
        |proxy| format!("{}/", proxy.addr),
        |_, port| {
            let mut request = vec![5, 1, 0, 1, 127, 0, 0, 1];
            request.extend_from_slice(&port.to_be_bytes());
            vec![vec![5, 1, 0], request]
        },
    )
    .await;
}

/// The caller of the port runs through the SOCKS5 proxy of the tests with the
/// user and the password.
#[tokio::test]
async fn rust_caller_via_socks5_auth_proxy() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C_PROXY to the path of the C binary of the proxy");
        return;
    };
    rust_caller_through_proxy(
        "rust-socks5-auth",
        Protocol::Socks5Auth,
        |proxy| format!("{}/{AUTH}", proxy.addr),
        |_, port| {
            let mut request = vec![5, 1, 0, 1, 127, 0, 0, 1];
            request.extend_from_slice(&port.to_be_bytes());
            vec![
                vec![5, 2, 0, 2],
                b"\x01\x04user\x08password".to_vec(),
                request,
            ]
        },
    )
    .await;
}

/// The C caller calls the listener of the port through the proxy of the tests.
///
/// The test compares the request of the C client with the request of the port:
/// both sides run the same scenario, the C client with its own code and the port
/// with `h_connect()`, so the bytes must match.
#[tokio::test]
async fn c_caller_via_rust_proxy() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C_PROXY to the path of the C binary of the proxy");
        return;
    };

    let server_dir = temp_dir("c-caller-server");
    let client_dir = temp_dir("c-caller-client");
    let port = free_port();
    let server_packet = vec![b's'; PACKET_SIZE];
    std::fs::write(server_dir.join("outb").join(SERVER_FILE), &server_packet).unwrap();
    let client_packet = vec![b'c'; PACKET_SIZE];
    std::fs::write(client_dir.join("outb").join(CLIENT_FILE), &client_packet).unwrap();

    // The listener of the port accepts one session.
    let mut server_config = config(&server_dir, SERVER_AKA);
    server_config.listen = vec![binkd_config::model::ListenChain {
        addr: "127.0.0.1".to_string(),
        port: port.to_string(),
    }];
    let client_aka = address(CLIENT_AKA, &server_config.spool.domains);
    server_config.spool.nodes.add(
        &client_aka,
        &NodeSpec {
            hosts: Some("127.0.0.1".to_string()),
            pwd: Some("secret".to_string()),
            ..NodeSpec::default()
        },
        time().epoch,
    );
    let server_path = server_dir.join("outb").join(SERVER_FILE);
    let server_size = server_packet.len() as u64;
    let received = Arc::new(Mutex::new(Vec::new()));
    let server_received = Arc::clone(&received);
    let log = Arc::new(Log::new(0, 0, "", Vec::new()));
    let now = time;
    let server = tokio::spawn(binkd_net::listen(
        server_config,
        log,
        now,
        SessionOptions::new(Duration::from_secs(20), true, true),
        move |_| {
            vec![SendFile {
                netname: SERVER_FILE.to_string(),
                path: Some(server_path.clone()),
                size: server_size,
                time: 1_000_000_000,
            }]
        },
        move |file: &binkd_proto::ReceivedFile| {
            if let Ok(mut received) = server_received.lock() {
                received.push(file.clone());
            }
            Ok(())
        },
    ));

    // The C client calls the listener through the proxy of the tests.
    let test_proxy = start_proxy(
        Protocol::Http,
        format!("127.0.0.1:{port}").parse().unwrap(),
        2,
    );
    c_config(
        &client_dir,
        CLIENT_AKA,
        &format!("{SERVER_AKA} 127.0.0.1:{port} secret"),
    );
    let mut config = std::fs::read_to_string(client_dir.join("binkd.conf")).unwrap();
    config.push_str(&format!("proxy {}\n", test_proxy.addr));
    std::fs::write(client_dir.join("binkd.conf"), config).unwrap();
    let c_client_dir = client_dir.clone();
    let c_client = tokio::task::spawn_blocking(move || run_c_client(&c_client_dir, SERVER_AKA));
    c_client.await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    server.abort();

    // The two sides exchanged the packets through the tunnel.
    let files = received.lock().unwrap().clone();
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(std::fs::read(&files[0].path).unwrap(), client_packet);
    assert_eq!(
        std::fs::read(client_dir.join("in").join(SERVER_FILE)).unwrap(),
        server_packet
    );

    // The request of the C client holds the tunnel to the listener.
    let requests = test_proxy.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1, "{requests:?}");
    let c_request = String::from_utf8_lossy(&requests[0]).to_string();
    assert_eq!(
        c_request,
        format!("CONNECT 127.0.0.1:{port} HTTP/1.0\r\n\r\n")
    );

    // The request of the port is the request of the C code, byte for byte.
    // The port runs the same scenario against the same proxy: it connects,
    // `h_connect()` reads the answer, and the proxy records the request.
    let mut stream = TcpStream::connect(test_proxy.addr).await.unwrap();
    let mut proxy_config = Config::new();
    proxy_config.proxy = test_proxy.addr.to_string();
    let port_log = Arc::new(Log::new(0, 0, "", Vec::new()));
    binkd_net::proxy::h_connect(
        &mut stream,
        "127.0.0.1",
        &port.to_string(),
        &proxy_config,
        Duration::from_secs(5),
        &port_log,
        &now,
    )
    .await
    .unwrap();
    let requests = test_proxy.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 2, "{requests:?}");
    let port_request = String::from_utf8_lossy(&requests[1]).to_string();
    assert_eq!(port_request, c_request);
}
