//! The SRIF request and response files, the port of `srif_fill()`,
//! `parse_response()`, and `mksrifpaths()` of `srif.c`.
//!
//! The request file is the plain ASCII file that the mailer writes for the
//! external FREQ processor, the ERP, of FSC-0086.001. The response file is the
//! plain ASCII file that the ERP answers with, one file per line. The port holds
//! the two files as values, so the caller reads and writes them.

use rbinkd_ftn::addr::FtnAddr;
use rbinkd_ftn::domain::DomainTable;

/// The path of the request file of an address, the port of `mksrifpaths()`.
///
/// The C code builds the base name from the first address of the peer with
/// `ftnaddress_to_filename()`, the BSO name of the address, and it answers an
/// error for an address whose domain the config does not hold.
pub fn paths(address: &FtnAddr, domains: &DomainTable) -> Option<(String, String)> {
    let base = address.to_filename(domains);
    if base.is_empty() {
        return None;
    }
    Some((format!("{base}.srf"), format!("{base}.rsp")))
}

/// The request of an SRIF exchange, the port of the arguments of `srif_fill()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The name of the sysop of the peer, the port of `peer_sysop`.
    pub sysop: String,
    /// The addresses of the peer, the port of `fa` and `nfa`.
    pub addresses: Vec<FtnAddr>,
    /// The path of the request file, the port of `req`.
    pub request_list: String,
    /// The path of the response file, the port of `rsp`.
    pub response_list: String,
    /// The session is protected, the port of `prot`.
    pub secure: bool,
    /// The peer is a listed node, the port of `listed`.
    pub listed: bool,
    /// The name of the peer, the port of `peer_name`.
    pub caller_id: String,
}

impl Request {
    /// Returns the content of the request file, the port of `srif_fill()`.
    ///
    /// The C code writes the two statements that are fixed, `Baud` and `Time`,
    /// and the statements of the session. It does not write the optional
    /// statements of FSC-0086.001, and it never writes the password.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Sysop {}\n", self.sysop));
        for address in &self.addresses {
            out.push_str(&format!("AKA {}\n", address.format(false)));
        }
        out.push_str("Baud 115200\n");
        out.push_str("Time -1\n");
        out.push_str(&format!("RequestList {}\n", self.request_list));
        out.push_str(&format!("ResponseList {}\n", self.response_list));
        out.push_str(&format!(
            "RemoteStatus {}\n",
            if self.secure {
                "PROTECTED"
            } else {
                "UNPROTECTED"
            }
        ));
        out.push_str(&format!(
            "SystemStatus {}\n",
            if self.listed { "LISTED" } else { "UNLISTED" }
        ));
        out.push_str(&format!("CallerID {}\n", self.caller_id));
        out.push_str("SessionType OTHER\n");
        out
    }
}

/// The action of a response line, the port of the `q_add_last_file()` call of
/// `parse_response()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// `=`: the C code deletes the file after a successful transfer.
    Delete,
    /// `+`: the C code keeps the file.
    Keep,
    /// `-`: the C code deletes the file in any case, the port of the kill list.
    Kill,
}

/// One line of a response file, the port of the loop of `parse_response()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A file of the response.
    File {
        /// The path of the file.
        path: String,
        /// The action after the transfer.
        action: Action,
    },
    /// The C code logs a predictor that it does not know and ignores the line.
    Unknown {
        /// The predictor, or the NUL character of an empty line.
        predictor: char,
        /// The line of the response file.
        line: String,
    },
}

/// Parses a response file, the port of `parse_response()`.
///
/// Every line holds a predictor and a path, the predictor first. The C code reads
/// the path up to the first whitespace and it does not test the file.
pub fn parse_response(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let token = line.split_whitespace().next().unwrap_or_default();
        let mut chars = token.chars();
        let Some(predictor) = chars.next() else {
            // The C code reads the NUL character of an empty line.
            out.push(Entry::Unknown {
                predictor: '\0',
                line: line.to_string(),
            });
            continue;
        };
        let path = chars.as_str().to_string();
        match predictor {
            '=' => out.push(Entry::File {
                path,
                action: Action::Delete,
            }),
            '+' => out.push(Entry::File {
                path,
                action: Action::Keep,
            }),
            '-' => out.push(Entry::File {
                path,
                action: Action::Kill,
            }),
            other => out.push(Entry::Unknown {
                predictor: other,
                line: line.to_string(),
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(s: &str) -> FtnAddr {
        let table = DomainTable::new(Vec::new());
        FtnAddr::parse(s, &table).unwrap()
    }

    #[test]
    fn the_paths_of_a_request() {
        let domains = DomainTable::new(vec![rbinkd_ftn::domain::FtnDomain {
            name: "fidonet".to_string(),
            path: "/var/spool/fido".to_string(),
            dir: "outb".to_string(),
            z: [2, -1],
            idomain: None,
            alias4: None,
        }]);
        let (srf, rsp) = paths(&address("2:5047/13@fidonet"), &domains).unwrap();
        assert_eq!(srf, "/var/spool/fido/outb/13b7000d.srf");
        assert_eq!(rsp, "/var/spool/fido/outb/13b7000d.rsp");
        // The C code answers an error for an address of another domain.
        assert_eq!(paths(&address("1:1/1@other"), &domains), None);
    }

    #[test]
    fn the_content_of_a_request() {
        let request = Request {
            sysop: "Peer sysop".to_string(),
            addresses: vec![address("2:5047/13@fidonet"), address("2:5047/13.4@fidonet")],
            request_list: "/var/spool/fido/in/13b70064.req".to_string(),
            response_list: "/var/spool/fido/outb/5047000d.rsp".to_string(),
            secure: true,
            listed: false,
            caller_id: "example.org".to_string(),
        };
        assert_eq!(
            request.to_text(),
            "Sysop Peer sysop\n\
             AKA 2:5047/13@fidonet\n\
             AKA 2:5047/13.4@fidonet\n\
             Baud 115200\n\
             Time -1\n\
             RequestList /var/spool/fido/in/13b70064.req\n\
             ResponseList /var/spool/fido/outb/5047000d.rsp\n\
             RemoteStatus PROTECTED\n\
             SystemStatus UNLISTED\n\
             CallerID example.org\n\
             SessionType OTHER\n"
        );
        // The C code writes the other two values for an unprotected session
        // of a listed node.
        let mut other = request.clone();
        other.secure = false;
        other.listed = true;
        let text = other.to_text();
        assert!(text.contains("RemoteStatus UNPROTECTED\n"), "{text}");
        assert!(text.contains("SystemStatus LISTED\n"), "{text}");
    }

    #[test]
    fn the_lines_of_a_response() {
        let entries = parse_response(
            "=/var/spool/fido/outb/13b7000d.pkt\n\
             +/var/spool/fido/outb/13b7000e.pkt\n\
             -/var/spool/fido/outb/13b7000f.pkt\n",
        );
        assert_eq!(
            entries,
            vec![
                Entry::File {
                    path: "/var/spool/fido/outb/13b7000d.pkt".to_string(),
                    action: Action::Delete,
                },
                Entry::File {
                    path: "/var/spool/fido/outb/13b7000e.pkt".to_string(),
                    action: Action::Keep,
                },
                Entry::File {
                    path: "/var/spool/fido/outb/13b7000f.pkt".to_string(),
                    action: Action::Kill,
                },
            ]
        );
    }

    #[test]
    fn the_errors_of_a_response() {
        // The C code logs a predictor that it does not know.
        let entries = parse_response("?/var/spool/fido/outb/13b7000d.pkt\n");
        assert_eq!(
            entries,
            vec![Entry::Unknown {
                predictor: '?',
                line: "?/var/spool/fido/outb/13b7000d.pkt".to_string(),
            }]
        );
        // An empty line answers the NUL character.
        let entries = parse_response("\n");
        assert_eq!(
            entries,
            vec![Entry::Unknown {
                predictor: '\0',
                line: String::new(),
            }]
        );
        // The C code reads the path up to the first whitespace.
        let entries = parse_response("=/var/spool/fido/outb/13b7000d.pkt extra words\n");
        assert_eq!(
            entries,
            vec![Entry::File {
                path: "/var/spool/fido/outb/13b7000d.pkt".to_string(),
                action: Action::Delete,
            }]
        );
        // The C code holds a response file without a line as empty.
        assert!(parse_response("").is_empty());
    }
}
