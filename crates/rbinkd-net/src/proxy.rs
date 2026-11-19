//! The proxy support of an outgoing call, the port of `https.c`.
//!
//! The C code connects the socket to the proxy server in `call0()` of
//! `client.c` and then calls `h_connect()`, which runs the handshake of the
//! proxy and hands the socket back for the binkp session. The port does the
//! same, and the module holds both parts: `proxy_address()` resolves the proxy
//! server, the port of the resolution of `call0()`, and `h_connect()` runs the
//! handshake.
//!
//! The C code resolves the SOCKS4 destination with the hints of `AF_INET`, so the
//! port holds the IPv4 addresses only.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use rbinkd_config::model::Config;
use rbinkd_log::{DateTime, Log, CONNECTION, DEBUG3, ERROR, INFO, PROTOCOL, START};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::base64;
use crate::ntlm;
use crate::srv::{Family, Hints};

/// The proxy of a call failed, the port of the error of `h_connect()`.
#[derive(Debug)]
pub enum ProxyError {
    /// The transport failed.
    Io(std::io::Error),
    /// The peer rejected the request.
    Rejected(String),
    /// The peer did not answer in time.
    Timeout,
}

/// Returns the address of the proxy server of a call, the port of the resolution
/// of `call0()` of `client.c`.
///
/// The C code resolves the host of the `proxy` or the `socks` directive with
/// the service name of the directive, `squid` or `socks`, and falls back to the
/// numeric default, 3128 or 1080. The port does the same.
pub async fn proxy_address(
    proxy: &str,
    socks: &str,
    family: Family,
    log: &Arc<Log>,
    now: &(impl Fn() -> DateTime + Sync),
) -> Result<SocketAddr, ProxyError> {
    let http = !proxy.is_empty();
    let entry = if http { proxy } else { socks };
    let (host, sport) = split_proxy(entry, http);
    // The C code resolves the host of the directive with the hints of the node,
    // the port of `srv_getaddrinfo()`.
    let hints = Hints {
        family,
        ..Hints::default()
    };
    // The C code falls back to the numeric default of the two directives when
    // the name of the service is unknown.
    if let Ok(addrs) = crate::srv::srv_getaddrinfo(&host, &sport, &hints).await {
        if let Some(addr) = addrs.first() {
            return Ok(*addr);
        }
    }
    let default = if http { 3128 } else { 1080 };
    log.message(
        CONNECTION,
        &format!("Port {sport} not found, try default {default}"),
        &now(),
    );
    // The C code reads the first address of the answer.
    let first = match crate::srv::srv_getaddrinfo(&host, &default.to_string(), &hints).await {
        Ok(addrs) => addrs.first().copied().ok_or_else(|| {
            log.message(
                ERROR,
                &format!(
                    "{} host {host} not found",
                    if http { "Proxy" } else { "Socks" }
                ),
                &now(),
            );
            ProxyError::Rejected(format!("{host} not found"))
        }),
        Err(e) => {
            log.message(
                ERROR,
                &format!(
                    "{} host {host} not found",
                    if http { "Proxy" } else { "Socks" }
                ),
                &now(),
            );
            Err(ProxyError::Io(e))
        }
    };
    first
}

/// Splits the host and the port of a proxy directive, the port of the part of
/// `call0()` of `client.c`.
pub fn split_proxy(entry: &str, http: bool) -> (String, String) {
    match entry.split_once(':') {
        Some((host, rest)) => {
            let sport = rest.split_once('/').map(|(port, _)| port).unwrap_or(rest);
            (host.to_string(), sport.to_string())
        }
        None => {
            let host = entry.split_once('/').map(|(host, _)| host).unwrap_or(entry);
            let sport = if http { "squid" } else { "socks" };
            (host.to_string(), sport.to_string())
        }
    }
}

/// The state of a handshake of a proxy, the port of the state of
/// `h_connect()` of `https.c`.
struct Handshake<'a> {
    /// The config, for the `oport` directive and the proxy directives.
    config: &'a Config,
    /// The time that a read waits for the peer, the port of `nettimeout`.
    timeout: Duration,
    log: &'a Arc<Log>,
    now: &'a (dyn Fn() -> DateTime + Sync),
}

/// Runs the handshake of the proxy, the port of `h_connect()` of `https.c`.
///
/// The socket is already connected to the proxy server, and the handshake leaves
/// it as the tunnel to the destination host.
pub async fn h_connect(
    stream: &mut TcpStream,
    host: &str,
    port: &str,
    config: &Config,
    timeout: Duration,
    log: &Arc<Log>,
    now: &(impl Fn() -> DateTime + Sync),
) -> Result<(), ProxyError> {
    let state = Handshake {
        config,
        timeout,
        log,
        now,
    };
    if !config.proxy.is_empty() {
        state
            .http(stream, host, port, &config.proxy, &config.socks)
            .await?;
    }
    if !config.socks.is_empty() {
        state.socks(stream, host, port, &config.socks).await?;
    }
    Ok(())
}

impl Handshake<'_> {
    /// Runs the handshake of an HTTP CONNECT proxy.
    async fn http(
        &self,
        stream: &mut TcpStream,
        host: &str,
        port: &str,
        proxy: &str,
        socks: &str,
    ) -> Result<(), ProxyError> {
        // The host and the auth part of the directive.
        let (proxy_host, auth) = match proxy.split_once('/') {
            Some((host, auth)) => (host, Some(auth)),
            None => (proxy, None),
        };
        self.log.message(
            INFO,
            &format!("connected to proxy {proxy_host}"),
            &(self.now)(),
        );
        // More than one slash in the auth part asks for NTLM. Otherwise the
        // first slash becomes the colon of the credential.
        let mut ntlm_mode = false;
        let mut ntlm_auth = String::new();
        let mut basic = String::new();
        if let Some(auth) = auth {
            if auth.matches('/').count() > 1 {
                ntlm_mode = true;
                ntlm_auth = auth.to_string();
            } else {
                let credential = auth.replacen('/', ":", 1);
                basic = base64::encode(credential.as_bytes());
            }
        }
        // A `socks` directive makes the HTTP proxy the tunnel to the socks
        // server.
        let target = if !socks.is_empty() {
            let socks_host = socks.split('/').next().unwrap_or_default();
            if socks_host.contains(':') {
                socks_host.to_string()
            } else {
                format!("{socks_host}:1080")
            }
        } else {
            format!("{host}:{port}")
        };
        let mut request = format!(
            "CONNECT {target} HTTP/1.{}\r\n",
            if ntlm_mode { 1 } else { 0 }
        );
        if auth.is_some() {
            if ntlm_mode {
                request.push_str("Connection: keep-alive\r\n");
                request.push_str("Proxy-Authorization: NTLM ");
                // The C code ignores the error of `getNTLM1()` and sends the
                // header without a token.
                if let Ok(token) = ntlm::get_ntlm1(&ntlm_auth) {
                    request.push_str(&token);
                }
                request.push_str("\r\n");
            } else {
                request.push_str(&format!("Proxy-Authorization: basic {basic}\r\n"));
            }
        }
        request.push_str("\r\n");
        if let Err(e) = stream.write_all(request.as_bytes()).await {
            self.log
                .message(START, &format!("Send to proxy error: {e}"), &(self.now)());
            return Err(ProxyError::Io(e));
        }
        self.log.message(
            DEBUG3,
            &format!("sent proxy request: {request}"),
            &(self.now)(),
        );

        // The answer: the C code reads the bytes one by one until the end of
        // the header block, the first empty line, or the end of the buffer.
        let mut buf: Vec<u8> = Vec::new();
        loop {
            let byte = match read_byte(stream, self.timeout).await {
                Ok(Some(byte)) => byte,
                Ok(None) => {
                    self.log.message(START, "proxy timeout...", &(self.now)());
                    return Err(ProxyError::Timeout);
                }
                Err(e) => {
                    self.log
                        .message(START, &format!("proxy error: {e}"), &(self.now)());
                    return Err(ProxyError::Io(e));
                }
            };
            buf.push(byte);
            if buf.len() + 1 < 1024
                && find(&buf, b"\r\n\r\n").is_none()
                && find(&buf, b"\n\n").is_none()
            {
                continue;
            }
            // The challenge of NTLM: the C code builds the Type 3 message,
            // sends it on the same socket, and reads the answer again.
            if ntlm_mode {
                if let Some(at) = find(&buf, b"uthenticate: NTLM ") {
                    let token = buf[at + 18..].to_vec();
                    // The C code drops the body of the answer, when the
                    // answer states its length.
                    if let Some(at) = find(&buf, b"Content-Length: ") {
                        let length: usize = String::from_utf8_lossy(&buf[at + 16..])
                            .split_whitespace()
                            .next()
                            .and_then(|number| number.parse().ok())
                            .unwrap_or(0);
                        for _ in 0..length {
                            if read_byte(stream, self.timeout)
                                .await
                                .ok()
                                .flatten()
                                .is_none()
                            {
                                break;
                            }
                        }
                    }
                    let request =
                        format!("CONNECT {host}:{port} HTTP/1.1\r\nProxy-Authorization: NTLM ");
                    match ntlm::get_ntlm2(&ntlm_auth, &String::from_utf8_lossy(&token)) {
                        Ok(ntlm3) => {
                            let request = format!("{request}{ntlm3}\r\n\r\n");
                            if let Err(e) = stream.write_all(request.as_bytes()).await {
                                self.log.message(
                                    START,
                                    &format!("Send to proxy error: {e}"),
                                    &(self.now)(),
                                );
                                return Err(ProxyError::Io(e));
                            }
                            buf.clear();
                            continue;
                        }
                        Err(code) => {
                            self.log.message(
                                CONNECTION,
                                &format!(
                                    "Invalid username/password/host/domain string ({ntlm_auth}) {code}"
                                ),
                                &(self.now)(),
                            );
                            // The C code falls through to the status line of
                            // the answer with the request in the buffer.
                            self.log.message(
                                CONNECTION,
                                &format!("Connection rejected by proxy ({request})"),
                                &(self.now)(),
                            );
                            return Err(ProxyError::Rejected(request));
                        }
                    }
                }
            }
            // The first line of the answer holds the status.
            let end = find(&buf, b"\n").unwrap_or(buf.len());
            let mut line = &buf[..end];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            if find(line, b" 200 ").is_some() {
                return Ok(());
            }
            let line = String::from_utf8_lossy(line).to_string();
            self.log.message(
                CONNECTION,
                &format!("Connection rejected by proxy ({line})"),
                &(self.now)(),
            );
            return Err(ProxyError::Rejected(line));
        }
    }

    /// Runs the handshake of a SOCKS proxy.
    ///
    /// The C code reads the user and the password of the directive from the
    /// first slash, and the slash tells the version: no slash asks for SOCKS4
    /// and a slash asks for SOCKS5.
    async fn socks(
        &self,
        stream: &mut TcpStream,
        host: &str,
        port: &str,
        socks: &str,
    ) -> Result<(), ProxyError> {
        let (socks_host, auth) = match socks.split_once('/') {
            Some((host, auth)) => (host, Some(auth)),
            None => (socks, None),
        };
        self.log.message(
            INFO,
            &format!(
                "connected to socks{} {socks_host}",
                if auth.is_some() { '5' } else { '4' }
            ),
            &(self.now)(),
        );
        if let Some(auth) = auth {
            return self.socks5(stream, host, port, auth).await;
        }
        self.socks4(stream, host, port).await
    }

    /// Runs the handshake of a SOCKS4 proxy.
    ///
    /// The C code resolves the destination with the hints of `AF_INET`, so the
    /// port holds the IPv4 addresses only.
    async fn socks4(
        &self,
        stream: &mut TcpStream,
        host: &str,
        port: &str,
    ) -> Result<(), ProxyError> {
        // The C code resolves the destination with the hints of IPv4, the
        // port of `srv_getaddrinfo()`.
        let hints = Hints {
            family: Family::V4,
            ..Hints::default()
        };
        let addrs: Vec<SocketAddrV4> = match crate::srv::srv_getaddrinfo(host, port, &hints).await {
            Ok(addrs) => addrs.iter().filter_map(|addr| v4(*addr)).collect(),
            Err(e) => {
                self.log.message(
                    CONNECTION,
                    &format!("getaddrinfo failed: {e}"),
                    &(self.now)(),
                );
                return Err(ProxyError::Io(e));
            }
        };
        let mut last = None;
        for addr in addrs {
            let portnum = addr.port();
            let trying = if port == self.config.oport {
                format!("trying {addr}...")
            } else {
                format!("trying {addr}:{portnum}...")
            };
            self.log.message(START, &trying, &(self.now)());
            let mut request = vec![4, 1];
            request.extend_from_slice(&portnum.to_be_bytes());
            request.extend_from_slice(&addr.ip().octets());
            request.push(0);
            if let Err(e) = stream.write_all(&request).await {
                self.log
                    .message(CONNECTION, &format!("socks error: {e}"), &(self.now)());
                return Err(ProxyError::Io(e));
            }
            // The answer of eight bytes: the version, the code, the port, and
            // the address of the server.
            let mut reply = [0u8; 8];
            self.read(stream, &mut reply).await?;
            if reply[0] != 0 {
                self.log
                    .message(CONNECTION, "Bad reply from socks server", &(self.now)());
                return Err(ProxyError::Rejected("bad reply".to_string()));
            }
            if reply[1] != 90 {
                let code = reply[1];
                self.log.message(
                    CONNECTION,
                    &format!("connection rejected by socks4 server ({code})"),
                    &(self.now)(),
                );
                // The C code tries the next address on the same connection.
                // The port does the same, and it answers an error for the last
                // address: the C code answers a success there.
                last = Some(code);
                continue;
            }
            return Ok(());
        }
        Err(ProxyError::Rejected(format!(
            "connection rejected by socks4 server ({})",
            last.unwrap_or(0)
        )))
    }

    /// Runs the handshake of a SOCKS5 proxy.
    async fn socks5(
        &self,
        stream: &mut TcpStream,
        host: &str,
        port: &str,
        auth: &str,
    ) -> Result<(), ProxyError> {
        let Some(portnum) = crate::port::find_port(port) else {
            self.log.message(
                CONNECTION,
                &format!("getaddrinfo failed: service {port} not found"),
                &(self.now)(),
            );
            return Err(ProxyError::Rejected(format!("service {port} not found")));
        };
        // The greeting: the version, the number of the methods, and the
        // methods. The C code holds no method for an empty user, the user and
        // the password method for a user and a password, and the GSSAPI method
        // for a user alone.
        let (user, password) = match auth.split_once('/') {
            Some((user, password)) => (user, Some(password)),
            None => (auth, None),
        };
        let greeting = if user.is_empty() {
            vec![5, 1, 0]
        } else {
            vec![5, 2, 0, if password.is_some() { 2 } else { 1 }]
        };
        if let Err(e) = stream.write_all(&greeting).await {
            self.log
                .message(CONNECTION, &format!("socks error: {e}"), &(self.now)());
            return Err(ProxyError::Io(e));
        }
        // The answer of the method: the version and the method.
        let mut reply = [0u8; 2];
        if stream.read_exact(&mut reply).await.is_err() || !(0..=2).contains(&reply[1]) {
            self.log.message(
                ERROR,
                "Auth. method not supported by socks5 server",
                &(self.now)(),
            );
            return Err(ProxyError::Rejected("auth method".to_string()));
        }
        self.log.message(
            PROTOCOL,
            &format!("Socks5, Auth={}", reply[1]),
            &(self.now)(),
        );
        if reply[1] == 2 {
            // The user and the password, the port of RFC 1929.
            let user = user.as_bytes();
            let password = password.unwrap_or_default().as_bytes();
            let mut request = vec![1, user.len() as u8];
            request.extend_from_slice(user);
            request.push(password.len() as u8);
            request.extend_from_slice(password);
            if let Err(e) = stream.write_all(&request).await {
                self.log
                    .message(CONNECTION, &format!("socks error: {e}"), &(self.now)());
                return Err(ProxyError::Io(e));
            }
            let mut reply = [0u8; 2];
            if stream.read_exact(&mut reply).await.is_err() || reply[1] != 0 {
                self.log.message(
                    ERROR,
                    &format!(
                        "Authentication failed (socks5 returns {:02X}{:02X})",
                        reply[0], reply[1]
                    ),
                    &(self.now)(),
                );
                return Err(ProxyError::Rejected("authentication".to_string()));
            }
        }
        // The request: the version, the command, the reserved byte, the
        // address, and the port. The C code sends a dotted quad as an IPv4
        // address and every other host as a name.
        let mut request = vec![5, 1, 0];
        if let Ok(ip) = host.parse::<Ipv4Addr>() {
            request.push(1);
            request.extend_from_slice(&ip.octets());
        } else {
            request.push(3);
            request.push(host.len() as u8);
            request.extend_from_slice(host.as_bytes());
        }
        request.extend_from_slice(&portnum.to_be_bytes());
        if let Err(e) = stream.write_all(&request).await {
            self.log
                .message(CONNECTION, &format!("socks error: {e}"), &(self.now)());
            return Err(ProxyError::Io(e));
        }
        // The answer: the version, the reply, the reserved byte, the address
        // type, the address, and the port.
        let mut reply = [0u8; 4];
        self.read(stream, &mut reply).await?;
        if reply[0] != 5 {
            self.log
                .message(CONNECTION, "Bad reply from socks server", &(self.now)());
            return Err(ProxyError::Rejected("bad reply".to_string()));
        }
        // The C code reads the address of the answer by its type.
        let rest = match reply[3] {
            1 => 6,
            3 => {
                let mut len = [0u8; 1];
                if stream.read_exact(&mut len).await.is_err() {
                    return Err(ProxyError::Rejected("bad reply".to_string()));
                }
                len[0] as usize + 2
            }
            4 => 18,
            _ => 0,
        };
        if rest > 0 {
            let mut tail = vec![0u8; rest];
            self.read(stream, &mut tail).await?;
        }
        if reply[1] == 0 {
            return Ok(());
        }
        let message = match reply[1] {
            1 => "general SOCKS5 server failure".to_string(),
            2 => "connection not allowed by ruleset (socks5)".to_string(),
            3 => "Network unreachable (socks5)".to_string(),
            4 => "Host unreachable (socks5)".to_string(),
            5 => "Connection refused (socks5)".to_string(),
            6 => "TTL expired (socks5)".to_string(),
            7 => "Command not supported by socks5".to_string(),
            8 => "Address type not supported".to_string(),
            code => format!("Unknown reply (0x{code:02X}) from socks5 server"),
        };
        self.log.message(CONNECTION, &message, &(self.now)());
        Err(ProxyError::Rejected(message))
    }

    /// Reads the bytes of an answer with the timeout of every byte, the port
    /// of the `select()` and the `recv()` loops of `https.c`.
    async fn read(&self, stream: &mut TcpStream, buf: &mut [u8]) -> Result<(), ProxyError> {
        for byte in buf.iter_mut() {
            match read_byte(stream, self.timeout).await {
                Ok(Some(b)) => *byte = b,
                Ok(None) => {
                    self.log.message(START, "socks timeout...", &(self.now)());
                    return Err(ProxyError::Timeout);
                }
                Err(e) => {
                    self.log
                        .message(CONNECTION, &format!("socks error: {e}"), &(self.now)());
                    self.log.message(
                        CONNECTION,
                        "connection closed by socks server...",
                        &(self.now)(),
                    );
                    return Err(ProxyError::Io(e));
                }
            }
        }
        Ok(())
    }
}

/// Reads one byte of a peer, the port of the `select()` and the `recv()` of a
/// byte of `https.c`.
///
/// The answer `None` is the timeout of the wait, and the error is a peer that
/// closed the transport or a transport that failed.
async fn read_byte(
    stream: &mut TcpStream,
    timeout: Duration,
) -> Result<Option<u8>, std::io::Error> {
    if timeout.is_zero() {
        return stream.read_u8().await.map(Some);
    }
    match tokio::time::timeout(timeout, stream.read_u8()).await {
        Ok(Ok(byte)) => Ok(Some(byte)),
        Ok(Err(e)) => Err(e),
        Err(_) => Ok(None),
    }
}

/// Returns the IPv4 address of an address, or `None`.
fn v4(addr: SocketAddr) -> Option<SocketAddrV4> {
    match addr {
        SocketAddr::V4(addr) => Some(addr),
        SocketAddr::V6(_) => None,
    }
}

/// Returns the first position of a needle in the bytes, the port of `strstr()`.
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_split_of_a_proxy_entry() {
        // The C code reads the port after the first colon, and it reads the
        // host before the first slash.
        assert_eq!(
            split_proxy("192.0.2.3:3128/user/password", true),
            ("192.0.2.3".to_string(), "3128".to_string())
        );
        assert_eq!(
            split_proxy("192.0.2.3/user/password", true),
            ("192.0.2.3".to_string(), "squid".to_string())
        );
        assert_eq!(
            split_proxy("192.0.2.3", false),
            ("192.0.2.3".to_string(), "socks".to_string())
        );
        assert_eq!(
            split_proxy("192.0.2.3:1080/", false),
            ("192.0.2.3".to_string(), "1080".to_string())
        );
        assert_eq!(
            split_proxy("192.0.2.3:1080", false),
            ("192.0.2.3".to_string(), "1080".to_string())
        );
    }

    #[test]
    fn the_search_of_a_needle() {
        assert_eq!(find(b"abcdef", b"cd"), Some(2));
        assert_eq!(find(b"abcdef", b"xy"), None);
        assert_eq!(find(b"ab", b"abc"), None);
    }
}
