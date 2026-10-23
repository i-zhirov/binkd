//! The DNS client of the SRV lookup, the port of the `res_search()` and of the
//! `ns_initparse()`/`ns_parserr()` calls of `srv_gai.c`.
//!
//! The C code asks the resolver library of the system. The port holds its own
//! small client, so the port keeps the dependency footprint at zero. The port
//! reproduces the parts of the library that the SRV lookup uses: the query name,
//! the search list, the answer order, and the name decompression.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::UdpSocket;

/// The type of an SRV record, `ns_t_srv`.
pub const TYPE_SRV: u16 = 33;
/// The class of the internet, `ns_c_in`.
pub const CLASS_IN: u16 = 1;
/// The largest answer, the port of `SRVGAI_DNSRESPLEN`.
pub const RESPONSE_LEN: usize = 1024;
/// The largest name of an answer, the port of `BINKD_FQDNLEN` and of the
/// buffer of `dn_expand()`.
const NAME_LEN: usize = 1025;
/// The largest number of the jumps of a compressed name.
const MAX_JUMPS: usize = 128;

/// The data of an SRV record, the port of the fields that `srv_gai.c` reads.
///
/// The C code reads the port and the target only, and it never reads the
/// priority and the weight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Srv {
    /// The priority, the port of `p[0..2]`.
    pub priority: u16,
    /// The weight, the port of `p[2..4]`.
    pub weight: u16,
    /// The port of the service, the port of `p[4..6]`.
    pub port: u16,
    /// The target of the service, the port of `dn_expand()`.
    pub target: String,
}

/// One answer of a query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The type of the record, the port of `ns_rr_type()`.
    pub kind: u16,
    /// The class of the record, the port of `ns_rr_class()`.
    pub class: u16,
    /// The data of an SRV record, or `None`.
    pub srv: Option<Srv>,
    /// The name of the record, the port of `ns_rr_name()`.
    pub name: String,
}

/// The settings of a resolver, the port of the state of the C library.
///
/// The C code reads the settings from the system through `res_init()`. The port
/// reads `/etc/resolv.conf` itself, so a test can supply its own settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolver {
    /// The servers, the port of the `nameserver` lines.
    pub nameservers: Vec<IpAddr>,
    /// The port of the servers, the port of the `port` of the C library.
    pub port: u16,
    /// The search list, the port of the `search` and the `domain` lines.
    pub search: Vec<String>,
    /// The number of the dots that asks for the search list, the port of the
    /// `ndots` option.
    pub ndots: usize,
    /// The time that a query waits, the port of the `timeout` option.
    pub timeout: Duration,
    /// The number of the tries, the port of the `attempts` option.
    pub attempts: usize,
}

impl Default for Resolver {
    /// Returns the settings of the system, the port of `res_init()`.
    fn default() -> Resolver {
        Resolver::system()
    }
}

impl Resolver {
    /// Returns the settings of the system, the port of `res_init()`.
    ///
    /// The C code reads `/etc/resolv.conf`. The port does the same, and it holds
    /// the defaults of the C library for a file that is absent or empty. The
    /// default server is the loopback address, the port of `_res`.
    pub fn system() -> Resolver {
        let mut resolver = Resolver {
            nameservers: Vec::new(),
            port: 53,
            search: Vec::new(),
            ndots: 1,
            timeout: Duration::from_secs(5),
            attempts: 2,
        };
        if let Ok(text) = std::fs::read_to_string("/etc/resolv.conf") {
            resolver.read(&text);
        }
        // The C library holds the loopback address for a file that is absent.
        // The Windows build of the C code holds no resolver by default, so the
        // port answers no server there and falls back to the plain resolution.
        if resolver.nameservers.is_empty() && cfg!(unix) {
            resolver.nameservers.push(IpAddr::V4(Ipv4Addr::LOCALHOST));
        }
        resolver
    }

    /// Reads the settings of a `resolv.conf`, the port of `res_init()`.
    pub fn read(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.split(['#', ';']).next().unwrap_or_default();
            let mut words = line.split_whitespace();
            match words.next() {
                Some("nameserver") => {
                    if let Some(server) = words.next().and_then(|s| s.parse().ok()) {
                        self.nameservers.push(server);
                    }
                }
                Some("domain") => {
                    if let Some(domain) = words.next() {
                        self.search = vec![domain.to_string()];
                    }
                }
                Some("search") => {
                    self.search = words.map(|s| s.to_string()).collect();
                }
                Some("options") => {
                    for option in words {
                        if let Some(value) = option.strip_prefix("ndots:") {
                            if let Ok(ndots) = value.parse() {
                                self.ndots = ndots;
                            }
                        }
                        if let Some(value) = option.strip_prefix("timeout:") {
                            if let Ok(seconds) = value.parse::<u64>() {
                                self.timeout = Duration::from_secs(seconds);
                            }
                        }
                        if let Some(value) = option.strip_prefix("attempts:") {
                            if let Ok(attempts) = value.parse() {
                                self.attempts = attempts;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Returns the names of a query, the port of the search list of
    /// `res_search()`.
    ///
    /// A name of `ndots` dots or more asks for the name itself first. A name
    /// with fewer dots asks for the search list first, and the name itself last.
    pub fn names(&self, name: &str) -> Vec<String> {
        if name.ends_with('.') {
            return vec![name.to_string()];
        }
        let dots = name.matches('.').count();
        let mut names = Vec::new();
        if dots >= self.ndots {
            names.push(name.to_string());
        }
        for domain in &self.search {
            names.push(format!("{name}.{domain}"));
        }
        if dots < self.ndots {
            names.push(name.to_string());
        }
        names
    }

    /// Returns the answers of a query, the port of `res_search()`.
    ///
    /// The C code tries every server, and it tries the servers again after a
    /// timeout. The port does the same.
    pub async fn search(&self, name: &str, kind: u16) -> io::Result<Vec<Record>> {
        let mut last = None;
        for name in self.names(name) {
            for _ in 0..self.attempts {
                for server in &self.nameservers {
                    match self.ask(server, &name, kind).await {
                        Ok(answers) => return Ok(answers),
                        Err(e) => last = Some(e),
                    }
                }
            }
        }
        Err(last.unwrap_or_else(|| io::Error::other("no nameserver")))
    }

    /// Returns the answers of one server, the port of one `res_query()`.
    async fn ask(&self, server: &IpAddr, name: &str, kind: u16) -> io::Result<Vec<Record>> {
        let id = query_id();
        let request = build_query(id, name, kind);
        let bind = if server.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let socket = UdpSocket::bind(bind).await?;
        socket.connect(SocketAddr::new(*server, self.port)).await?;
        socket.send(&request).await?;
        let mut buf = [0u8; RESPONSE_LEN];
        let len = tokio::time::timeout(self.timeout, socket.recv(&mut buf))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "the resolver timed out"))??;
        parse_response(&buf[..len], id)
    }
}

/// Returns the query of a name, the port of the request of `res_search()`.
fn build_query(id: u16, name: &str, kind: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(NAME_LEN + 16);
    out.extend_from_slice(&id.to_be_bytes());
    // The flags ask for a recursion, the port of `RES_RECURSE`.
    out.extend_from_slice(&0x0100u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    encode_name(&mut out, name);
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    out
}

/// Writes the labels of a name, the port of the `dn_comp()` of the C library.
///
/// The name ends with the root label, the port of the trailing dot that the
/// C library adds.
pub fn encode_name(out: &mut Vec<u8>, name: &str) {
    for label in name.trim_end_matches('.').split('.') {
        let label = label.as_bytes();
        if label.is_empty() || label.len() > 63 {
            continue;
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label);
    }
    out.push(0);
}

/// Returns the records of an answer, the port of `ns_initparse()` and of the
/// loop of `ns_parserr()` of `srv_gai.c`.
pub fn parse_response(message: &[u8], id: u16) -> io::Result<Vec<Record>> {
    let header = |at: usize| -> Option<u16> {
        Some(u16::from_be_bytes([
            *message.get(at)?,
            *message.get(at + 1)?,
        ]))
    };
    if header(0) != Some(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the answer holds another id",
        ));
    }
    let questions = header(4).unwrap_or(0) as usize;
    let answers = header(6).unwrap_or(0) as usize;
    let mut at = 12;
    for _ in 0..questions {
        let (_, next) = expand_name(message, at).ok_or_else(bad)?;
        // The question holds the name, the type, and the class.
        at = next + 4;
    }
    let mut records = Vec::new();
    for _ in 0..answers {
        let Some((name, next)) = expand_name(message, at) else {
            break;
        };
        let kind = u16::from_be_bytes([
            *message.get(next).ok_or_else(bad)?,
            *message.get(next + 1).ok_or_else(bad)?,
        ]);
        let class = u16::from_be_bytes([
            *message.get(next + 2).ok_or_else(bad)?,
            *message.get(next + 3).ok_or_else(bad)?,
        ]);
        let length = u16::from_be_bytes([
            *message.get(next + 8).ok_or_else(bad)?,
            *message.get(next + 9).ok_or_else(bad)?,
        ]) as usize;
        let start = next + 10;
        let data = message.get(start..start + length).ok_or_else(bad)?;
        // The data of an SRV record: the priority, the weight, the port, and
        // the target.
        let srv = if kind == TYPE_SRV && length >= 8 {
            Some(Srv {
                priority: u16::from_be_bytes([data[0], data[1]]),
                weight: u16::from_be_bytes([data[2], data[3]]),
                port: u16::from_be_bytes([data[4], data[5]]),
                target: expand_name(message, start + 6)
                    .map(|(name, _)| name)
                    .unwrap_or_default(),
            })
        } else {
            None
        };
        records.push(Record {
            kind,
            class,
            srv,
            name,
        });
        at = start + length;
    }
    Ok(records)
}

/// Returns the name at an offset and the offset after it, the port of
/// `dn_expand()` of the C library.
///
/// The name may hold a pointer to another name, so the port follows the
/// pointers. The port gives up after `MAX_JUMPS` pointers.
pub fn expand_name(message: &[u8], start: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut at = start;
    let mut after = None;
    let mut jumps = 0;
    loop {
        let length = *message.get(at)? as usize;
        if length == 0 {
            at += 1;
            break;
        }
        if length & 0xc0 == 0xc0 {
            let pointer = ((length & 0x3f) << 8) | *message.get(at + 1)? as usize;
            if after.is_none() {
                after = Some(at + 2);
            }
            at = pointer;
            jumps += 1;
            if jumps > MAX_JUMPS {
                return None;
            }
            continue;
        }
        let label = message.get(at + 1..at + 1 + length)?;
        if !name.is_empty() {
            name.push('.');
        }
        name.push_str(&String::from_utf8_lossy(label));
        if name.len() > NAME_LEN {
            return None;
        }
        at += 1 + length;
    }
    Some((name, after.unwrap_or(at)))
}

/// Returns an error for a bad answer.
fn bad() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "the answer is not valid")
}

/// Returns the number of a query, the port of the random number of the C library.
fn query_id() -> u16 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos as u16) ^ (std::process::id() as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The answer of the test: the header, the question, and one SRV record
    /// with a compressed name.
    fn answer() -> Vec<u8> {
        let mut message = Vec::new();
        message.extend_from_slice(&0x1234u16.to_be_bytes());
        message.extend_from_slice(&0x8180u16.to_be_bytes());
        message.extend_from_slice(&1u16.to_be_bytes());
        message.extend_from_slice(&1u16.to_be_bytes());
        message.extend_from_slice(&0u16.to_be_bytes());
        message.extend_from_slice(&0u16.to_be_bytes());
        encode_name(&mut message, "_binkp._tcp.example.org");
        message.extend_from_slice(&TYPE_SRV.to_be_bytes());
        message.extend_from_slice(&CLASS_IN.to_be_bytes());
        // The record: a pointer to the question, the type, the class, the
        // time, and the data of the SRV record.
        message.extend_from_slice(&[0xc0, 0x0c]);
        message.extend_from_slice(&TYPE_SRV.to_be_bytes());
        message.extend_from_slice(&CLASS_IN.to_be_bytes());
        message.extend_from_slice(&300u32.to_be_bytes());
        let mut data = Vec::new();
        data.extend_from_slice(&0u16.to_be_bytes());
        data.extend_from_slice(&0u16.to_be_bytes());
        data.extend_from_slice(&24555u16.to_be_bytes());
        encode_name(&mut data, "target.example.org");
        message.extend_from_slice(&(data.len() as u16).to_be_bytes());
        message.extend_from_slice(&data);
        message
    }

    #[test]
    fn the_query_of_a_name() {
        let query = build_query(0x1234, "_binkp._tcp.example.org", TYPE_SRV);
        assert_eq!(&query[..2], &[0x12, 0x34]);
        assert_eq!(&query[2..4], &[0x01, 0x00]);
        assert_eq!(&query[4..6], &[0x00, 0x01]);
        // The label holds its length first: "_binkp" is six bytes.
        assert_eq!(&query[12..19], b"\x06_binkp");
        assert_eq!(&query[19..24], b"\x04_tcp");
        assert_eq!(&query[query.len() - 4..], &[0x00, 0x21, 0x00, 0x01]);
    }

    #[test]
    fn the_name_of_an_answer() {
        let message = answer();
        // The name of the record points at the name of the question.
        assert_eq!(
            expand_name(&message, 12),
            Some(("_binkp._tcp.example.org".to_string(), 37))
        );
        let records = parse_response(&message, 0x1234).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kind, TYPE_SRV);
        assert_eq!(records[0].class, CLASS_IN);
        let srv = records[0].srv.clone().unwrap();
        assert_eq!(srv.priority, 0);
        assert_eq!(srv.weight, 0);
        assert_eq!(srv.port, 24555);
        assert_eq!(srv.target, "target.example.org");
    }

    #[test]
    fn the_errors_of_an_answer() {
        let message = answer();
        assert!(parse_response(&message, 0x5678).is_err());
        // The answer stops in the middle of the question.
        assert!(parse_response(&message[..14], 0x1234).is_err());
        assert!(parse_response(&[], 0x1234).is_err());
    }

    #[test]
    fn the_search_list() {
        let mut resolver = Resolver {
            nameservers: Vec::new(),
            port: 53,
            search: vec!["example.org".to_string()],
            ndots: 1,
            timeout: Duration::from_secs(5),
            attempts: 2,
        };
        // A name with a dot asks for itself first, the port of `ndots:1`.
        assert_eq!(
            resolver.names("host.example.net"),
            vec!["host.example.net", "host.example.net.example.org"]
        );
        assert_eq!(resolver.names("host"), vec!["host.example.org", "host"]);
        resolver.ndots = 2;
        assert_eq!(
            resolver.names("host.example"),
            vec!["host.example.example.org", "host.example"]
        );
        // A name with the trailing dot asks for itself only.
        assert_eq!(resolver.names("host."), vec!["host."]);
    }

    #[test]
    fn the_settings_of_resolv_conf() {
        let mut resolver = Resolver {
            nameservers: Vec::new(),
            port: 53,
            search: Vec::new(),
            ndots: 1,
            timeout: Duration::from_secs(5),
            attempts: 2,
        };
        resolver.read(
            "nameserver 192.0.2.1\n\
             nameserver 2001:db8::1 # a comment\n\
             domain example.org\n\
             options ndots:2 timeout:3 attempts:4\n",
        );
        assert_eq!(
            resolver.nameservers,
            vec![
                "192.0.2.1".parse::<IpAddr>().unwrap(),
                "2001:db8::1".parse::<IpAddr>().unwrap()
            ]
        );
        assert_eq!(resolver.search, vec!["example.org"]);
        assert_eq!(resolver.ndots, 2);
        assert_eq!(resolver.timeout, Duration::from_secs(3));
        assert_eq!(resolver.attempts, 4);
    }
}
