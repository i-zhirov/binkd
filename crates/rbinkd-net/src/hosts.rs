//! The host list of a node, the port of `get_host_and_port()` of `readcfg.c`
//! and of `ftnaddress_to_domain()` of `ftnaddr.c`.

use rbinkd_config::model::Config;
use rbinkd_ftn::addr::FtnAddr;

/// One entry of the host list of a node, the port of the result of
/// `get_host_and_port()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    /// The host name, or empty when the entry asks for the name of the address.
    pub name: String,
    /// The service, the port of the entry or of the `oport` directive. The C
    /// code holds the text, so a name stays a name and a number a number. The
    /// SRV lookup of `srv_gai.c` asks for a name only.
    pub port: String,
    /// The entry asks for the host name of the address, the port of the `*`
    /// entry of the list.
    pub domain: bool,
}

/// Returns the host list of a node, the port of `get_host_and_port()` of
/// `readcfg.c`.
///
/// The entries are separated by a comma or a semicolon. An entry without a port
/// uses the `oport` directive of the config. The entry `*` asks for the host
/// name of the address of the node, the port of `ftnaddress_to_domain()`.
pub fn hosts(list: &str, fa: &FtnAddr, config: &Config) -> Vec<Host> {
    let mut out = Vec::new();
    for entry in list.split([',', ';']) {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        // The C code finds the first colon, and it reads an IPv6 literal of the
        // form `[<address>]:<port>` first.
        let (name, port) = match entry.strip_prefix('[') {
            Some(rest) => match rest.split_once(']') {
                Some((name, rest)) => (name.to_string(), rest.strip_prefix(':')),
                None => (entry.to_string(), None),
            },
            None => match entry.split_once(':') {
                Some((name, rest)) => (name.to_string(), Some(rest)),
                None => (entry.to_string(), None),
            },
        };
        // The C code holds the text of the port, and it answers an error for a
        // name that the system does not know.
        let port = match port {
            Some(port) => match crate::port::find_port(port) {
                Some(_) => port.to_string(),
                None => continue,
            },
            None => config.oport.clone(),
        };
        if name == "*" {
            out.push(Host {
                name: ftnaddress_to_domain(fa, config),
                port,
                domain: true,
            });
        } else {
            out.push(Host {
                name,
                port,
                domain: false,
            });
        }
    }
    out
}

/// Returns the host name of an address, the port of `ftnaddress_to_domain()` of
/// `ftnaddr.c`.
///
/// The name is `f<node>.n<net>.z<zone>.<domain>`, or `p<point>.` before it
/// for an address with a point. The domain is the `idomain` of the domain of the
/// address, or the `root-domain` directive.
pub fn ftnaddress_to_domain(fa: &FtnAddr, config: &Config) -> String {
    let mut name = if fa.p == 0 {
        format!("f{}.n{}.z{}.", fa.node, fa.net, fa.z)
    } else {
        format!("p{}.f{}.n{}.z{}.", fa.p, fa.node, fa.net, fa.z)
    };
    let domain = config
        .spool
        .domains
        .get(&fa.domain)
        .and_then(|domain| domain.idomain.clone())
        .unwrap_or_else(|| config.root_domain.clone());
    name.push_str(&domain);
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbinkd_config::model::Config;
    use rbinkd_ftn::domain::{DomainTable, FtnDomain};

    fn config() -> Config {
        let mut config = Config {
            oport: "binkp".to_string(),
            root_domain: "binkp.net".to_string(),
            ..Config::new()
        };
        config.spool.domains = DomainTable::new(vec![FtnDomain {
            name: "fidonet".to_string(),
            path: String::new(),
            dir: String::new(),
            z: [2, -1],
            idomain: Some("fidonet.org".to_string()),
            alias4: None,
        }]);
        config
    }

    fn address(s: &str) -> FtnAddr {
        FtnAddr::parse(s, &config().spool.domains).unwrap()
    }

    #[test]
    fn list() {
        let config = config();
        let fa = address("2:5047/13@fidonet");
        let hosts = hosts("host.example.com:24555,other.example.com", &fa, &config);
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].name, "host.example.com");
        assert_eq!(hosts[0].port, "24555");
        assert_eq!(hosts[1].name, "other.example.com");
        // The entry without a port uses the text of the `oport` directive.
        assert_eq!(hosts[1].port, "binkp");
        assert!(!hosts[1].domain);
    }

    #[test]
    fn semicolon_and_asterisk() {
        let config = config();
        let fa = address("2:5047/13@fidonet");
        let hosts = hosts("host.example.com;*", &fa, &config);
        assert_eq!(hosts.len(), 2);
        assert!(hosts[1].domain);
        assert_eq!(hosts[1].name, "f13.n5047.z2.fidonet.org");
    }

    #[test]
    fn address_to_domain() {
        let config = config();
        assert_eq!(
            ftnaddress_to_domain(&address("2:5047/13@fidonet"), &config),
            "f13.n5047.z2.fidonet.org"
        );
        assert_eq!(
            ftnaddress_to_domain(&address("2:5047/13.4@fidonet"), &config),
            "p4.f13.n5047.z2.fidonet.org"
        );
        // An address of an unknown domain uses the `root-domain` directive.
        assert_eq!(
            ftnaddress_to_domain(&address("1:1/1@other"), &config),
            "f1.n1.z1.binkp.net"
        );
    }
}
