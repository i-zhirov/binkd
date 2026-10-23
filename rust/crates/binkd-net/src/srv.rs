//! The SRV lookup of a name, the port of `srv_gai.c`.
//!
//! The C code asks the resolver library of the system through `res_search()` and
//! parses the answer with the BIND `ns_*` API. The port holds its own client in
//! `dns.rs`, so the port keeps the dependency footprint at zero and the tests
//! can supply their own server.
//!
//! The C code takes the records of the answer in the order that the resolver
//! returned them. It reads the port and the target of every record and it ignores
//! the priority and the weight, so it does not follow RFC 2782. The port does the
//! same.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};

use crate::dns::{Resolver, CLASS_IN, TYPE_SRV};

/// The address family of a resolution, the port of `ai_family`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Family {
    /// Every family, `AF_UNSPEC`.
    #[default]
    Unspecified,
    /// The IPv4 family, `AF_INET`.
    V4,
    /// The IPv6 family, `AF_INET6`.
    V6,
}

/// The socket type of a resolution, the port of `ai_socktype`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SockType {
    /// A stream, `SOCK_STREAM`.
    #[default]
    Stream,
    /// A datagram, `SOCK_DGRAM`.
    Dgram,
}

/// The hints of a resolution, the port of the `addrinfo` hints.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hints {
    /// The address family, the port of `ai_family`.
    pub family: Family,
    /// The socket type, the port of `ai_socktype`.
    pub socktype: SockType,
    /// The name is numeric, the port of `AI_NUMERICHOST`.
    pub numeric_host: bool,
    /// The service is numeric, the port of `AI_NUMERICSERV`.
    pub numeric_service: bool,
}

impl Hints {
    /// Creates the hints of a stream of every family.
    pub fn stream() -> Hints {
        Hints::default()
    }
}

/// Returns the addresses of a name and a service, the port of
/// `srv_getaddrinfo()` of `srv_gai.c`.
///
/// The C code asks the resolver library of the system. The port reads the
/// settings of `/etc/resolv.conf` itself, the port of `res_init()`.
pub async fn srv_getaddrinfo(
    node: &str,
    service: &str,
    hints: &Hints,
) -> io::Result<Vec<SocketAddr>> {
    let resolver = Resolver::system();
    srv_getaddrinfo_with(node, service, hints, &resolver).await
}

/// Returns the addresses of a name and a service with the given settings.
///
/// The settings are the port of the resolver of the system, so a test supplies
/// its own server. The C code holds no such test.
pub async fn srv_getaddrinfo_with(
    node: &str,
    service: &str,
    hints: &Hints,
    resolver: &Resolver,
) -> io::Result<Vec<SocketAddr>> {
    // The guards of the C code: a plain resolution answers every other case.
    if node.is_empty() || service.is_empty() || !applies(node, service, hints) {
        return plain(node, service, hints).await;
    }
    // The query name of the C code: `_<service>._<tcp|udp>.<node>`.
    let name = format!(
        "_{service}._{}.{node}",
        if hints.socktype == SockType::Stream {
            "tcp"
        } else {
            "udp"
        }
    );
    let Ok(answers) = resolver.search(&name, TYPE_SRV).await else {
        return plain(node, service, hints).await;
    };
    // The C code takes the records in the order of the answer. It reads the
    // port and the target of every record, and it resolves the target with a
    // second lookup. The port of the record wins.
    let mut out = Vec::new();
    for record in &answers {
        if record.class != CLASS_IN || record.kind != TYPE_SRV {
            continue;
        }
        let Some(srv) = &record.srv else {
            continue;
        };
        // The C code skips a target that is the root, the port of the check of
        // `dn_expand()`.
        if srv.target.is_empty() {
            continue;
        }
        if let Ok(mut addrs) = plain(&srv.target, &srv.port.to_string(), hints).await {
            out.append(&mut addrs);
        }
    }
    // The fallback of the C code: no target of the answer resolved.
    if out.is_empty() {
        return plain(node, service, hints).await;
    }
    Ok(out)
}

/// Reports whether the SRV lookup applies, the port of the guards of
/// `srv_getaddrinfo()`.
fn applies(node: &str, service: &str, hints: &Hints) -> bool {
    // A numeric name asks for no lookup, the port of `AI_NUMERICHOST`.
    if hints.numeric_host {
        return false;
    }
    // An address literal asks for no lookup. The C code tests an IPv4 literal
    // for the families of IPv4, and a colon for the families of IPv6.
    if matches!(hints.family, Family::Unspecified | Family::V4) && node.parse::<Ipv4Addr>().is_ok()
    {
        return false;
    }
    if matches!(hints.family, Family::Unspecified | Family::V6) && node.contains(':') {
        return false;
    }
    // A numeric service asks for no lookup, the port of `AI_NUMERICSERV` and of
    // the test of `atoi()`.
    if hints.numeric_service || service.starts_with('0') || number(service) > 0 {
        return false;
    }
    true
}

/// Returns the number at the start of a text, the port of `atoi()`.
fn number(text: &str) -> u64 {
    let digits: String = text
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().unwrap_or(0)
}

/// Returns the addresses of a plain lookup, the port of `getaddrinfo()`.
///
/// The C code asks the services database of the system for a name. The port
/// holds the names of the C code, the port of `find_port()`, so an unknown name
/// is an error for the port.
async fn plain(node: &str, service: &str, hints: &Hints) -> io::Result<Vec<SocketAddr>> {
    // The C code holds NULL for an empty service: the port is zero.
    let port = if service.is_empty() {
        0
    } else {
        crate::port::find_port(service)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown service"))?
    };
    let addrs = tokio::net::lookup_host((node, port)).await?;
    Ok(addrs
        .filter(|addr| match hints.family {
            Family::Unspecified => true,
            Family::V4 => addr.is_ipv4(),
            Family::V6 => addr.is_ipv6(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guards_of_the_lookup() {
        let hints = Hints::stream();
        assert!(applies("example.org", "binkp", &hints));
        // An address literal asks for no lookup.
        assert!(!applies("192.0.2.1", "binkp", &hints));
        assert!(!applies("2001:db8::1", "binkp", &hints));
        // A numeric service asks for no lookup.
        assert!(!applies("example.org", "24554", &hints));
        assert!(!applies("example.org", "0", &hints));
        assert!(!applies("example.org", "0abc", &hints));
        assert!(!applies("example.org", "24554abc", &hints));
        // A numeric name asks for no lookup.
        let mut numeric = hints;
        numeric.numeric_host = true;
        assert!(!applies("example.org", "binkp", &numeric));
        // The family of IPv4 asks for no lookup of an IPv6 literal.
        let mut v4 = hints;
        v4.family = Family::V4;
        assert!(applies("2001:db8::1", "binkp", &v4));
        // The empty name and service hold no guard of their own: the caller
        // of the lookup answers a plain resolution for them.
    }

    #[test]
    fn the_number_of_a_service() {
        assert_eq!(number("24554"), 24554);
        assert_eq!(number("binkp"), 0);
        assert_eq!(number("24554abc"), 24554);
        assert_eq!(number(" 12"), 12);
    }
}
