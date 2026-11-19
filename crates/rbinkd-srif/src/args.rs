//! The macro expansion of an `exec` command, the port of `run_args()` of
//! `srif.c`.
//!
//! The C code expands the macros of the command in place and then runs it. The port
//! expands the macros and answers the command, so the caller runs it. The C code
//! also refuses a command that holds `*F` or `*N` when the name of the received
//! file holds a character that it does not accept, and the port answers that
//! decision in the flag of [`Expanded`].

use crate::Session;

/// The characters of a file name that the C code accepts besides the
/// alphanumerics, `valid_filename_chars`.
const VALID: &str = ".\\-_:/@";

/// The lookup of the short name of a file, the port of the
/// `GetShortPathName()` call of `run_args()`.
///
/// The error is the Windows error code of a failed call. The crate stays pure:
/// the caller supplies the lookup, and no platform call lives here.
pub type ShortName = fn(&str) -> Result<String, u32>;

/// The result of the expansion of a command, the port of the state of
/// `run_args()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expanded {
    /// The command with the macros replaced.
    pub command: String,
    /// The C code does not run the command: the name of the received file
    /// holds a character that it does not accept, and the command holds `*F` or
    /// `*N`. The C code logs `Security problem. Execution aborted...`.
    pub aborted: bool,
    /// The error of a failed short-name lookup, the port of the
    /// `GetLastError()` value that the C code logs.
    pub short_name_error: Option<u32>,
}

/// Returns true when the C code accepts every character of a name.
fn accepted(name: &str) -> bool {
    name.chars()
        .all(|c| c.is_alphanumeric() || VALID.contains(c))
}

/// Expands the macros of a command, the port of `run_args()`.
///
/// The macros of the C code, with the same case-insensitive letters:
///
/// - `*N`: the short name of the file when the caller supplies a lookup, the
///   name of the file when it does not. A failed lookup logs its code and
///   falls back to the validated name, the C branch.
/// - `*F`: the name of the file.
/// - `*A*` and `*A@`: the addresses of the peer, separated by spaces.
/// - `*A0` to `*A9`: the address of that number, or `-`.
/// - `*A` with any other character: the first address with the character after
///   it.
/// - `*P`: `1` for a protected session, else `0`.
/// - `*L`: `1` for a listed node, else `0`.
/// - `*H`: the name of the peer, or its address for a name that the C code
///   does not accept.
///
/// The C code leaves a macro that it does not know as it is.
pub fn expand(command: &str, filename: &str, session: &Session) -> Expanded {
    expand_with(command, filename, session, None)
}

/// Expands the macros of a command with a short-name lookup, the port of
/// `run_args()` with the Windows branch.
///
/// The lookup answers the short path, or the Windows error code of a failed
/// `GetShortPathName()`. The C code falls back to the validated name and logs
/// the code when the call fails.
pub fn expand_with(
    command: &str,
    filename: &str,
    session: &Session,
    short_name: Option<ShortName>,
) -> Expanded {
    // The C code answers `-` for a name that it does not accept, and it holds
    // the flag of the abort for a command that uses the name.
    let name_ok = accepted(filename);
    let name = if name_ok { filename } else { "-" };
    // The C code answers the address of the peer for a name that it does not
    // accept, the empty name included.
    let peer = if !session.peer_name.is_empty() && accepted(&session.peer_name) {
        session.peer_name.as_str()
    } else {
        session.ipaddr.as_str()
    };

    let mut out = String::with_capacity(command.len());
    let mut chars = command.chars().peekable();
    let mut used_name = false;
    let mut short_name_error = None;
    while let Some(c) = chars.next() {
        if c != '*' {
            out.push(c);
            continue;
        }
        let Some(letter) = chars.next() else {
            // A `*` at the end of the command stays as it is.
            out.push('*');
            break;
        };
        match letter.to_ascii_uppercase() {
            'N' => match short_name {
                Some(lookup) => match lookup(filename) {
                    Ok(short) => out.push_str(&short),
                    Err(code) => {
                        short_name_error = Some(code);
                        used_name = true;
                        out.push_str(name);
                    }
                },
                None => {
                    used_name = true;
                    out.push_str(name);
                }
            },
            'F' => {
                out.push_str(name);
                used_name = true;
            }
            'A' => {
                match chars.peek().copied() {
                    Some('@') | Some('*') => {
                        chars.next();
                        for (i, address) in session.addresses.iter().enumerate() {
                            if i > 0 {
                                out.push(' ');
                            }
                            out.push_str(&address.format(false));
                        }
                    }
                    third => {
                        let index = match third {
                            Some(c) if c.is_ascii_digit() => c as usize - '0' as usize,
                            _ => 0,
                        };
                        let address = match session.addresses.get(index) {
                            Some(address) => address.format(false),
                            None => "-".to_string(),
                        };
                        out.push_str(&address);
                        // A character that is not a digit comes after the
                        // address. The C code answers `*A` at the end of the
                        // command as the first address.
                        if let Some(c) = third {
                            chars.next();
                            if !c.is_ascii_digit() {
                                out.push(c);
                            }
                        }
                    }
                }
            }
            'P' => out.push(if session.secure { '1' } else { '0' }),
            'L' => out.push(if session.listed { '1' } else { '0' }),
            'H' => out.push_str(peer),
            _ => {
                // The C code leaves a macro that it does not know as it is.
                out.push('*');
                out.push(letter);
            }
        }
    }
    Expanded {
        command: out,
        aborted: !name_ok && used_name,
        short_name_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbinkd_ftn::addr::FtnAddr;
    use rbinkd_ftn::domain::DomainTable;

    fn address(s: &str) -> FtnAddr {
        let table = DomainTable::new(Vec::new());
        FtnAddr::parse(s, &table).unwrap()
    }

    fn session() -> Session {
        Session {
            addresses: vec![address("2:5047/13@fidonet"), address("2:5047/13.4@fidonet")],
            secure: true,
            listed: false,
            peer_name: "example.org".to_string(),
            ipaddr: "192.0.2.1".to_string(),
            sysop: "Peer sysop".to_string(),
        }
    }

    #[test]
    fn the_macros_of_a_file() {
        let session = session();
        let expanded = expand("program *F", "13b70064.pkt", &session);
        assert_eq!(expanded.command, "program 13b70064.pkt");
        assert!(!expanded.aborted);
        // The C code answers the name of the file for `*N` outside Windows.
        let expanded = expand("program *N", "13b70064.pkt", &session);
        assert_eq!(expanded.command, "program 13b70064.pkt");
        // The letters of a macro are not case sensitive.
        let expanded = expand("program *f", "13b70064.pkt", &session);
        assert_eq!(expanded.command, "program 13b70064.pkt");
    }

    fn short_name_ok(_filename: &str) -> Result<String, u32> {
        Ok("13B700~1.PKT".to_string())
    }

    fn short_name_failure(_filename: &str) -> Result<String, u32> {
        Err(5)
    }

    #[test]
    fn the_short_name_of_a_file() {
        let session = session();
        let expanded = expand_with("program *N", "13b70064.pkt", &session, Some(short_name_ok));
        assert_eq!(expanded.command, "program 13B700~1.PKT");
        assert!(!expanded.aborted);
        assert_eq!(expanded.short_name_error, None);
        // `*F` still answers the long name of the file.
        let expanded = expand_with("program *F", "13b70064.pkt", &session, Some(short_name_ok));
        assert_eq!(expanded.command, "program 13b70064.pkt");
        assert!(!expanded.aborted);
        assert_eq!(expanded.short_name_error, None);
    }

    #[test]
    fn the_short_name_falls_back_to_the_name() {
        let session = session();
        let expanded = expand_with(
            "program *N",
            "13b70064.pkt",
            &session,
            Some(short_name_failure),
        );
        assert_eq!(expanded.command, "program 13b70064.pkt");
        assert!(!expanded.aborted);
        assert_eq!(expanded.short_name_error, Some(5));
        // A failed lookup of an invalid name uses `-` and aborts, the C branch.
        let expanded = expand_with(
            "program *N",
            "13b70064.pkt;rm -rf",
            &session,
            Some(short_name_failure),
        );
        assert_eq!(expanded.command, "program -");
        assert!(expanded.aborted);
        assert_eq!(expanded.short_name_error, Some(5));
    }

    #[test]
    fn the_short_name_of_an_invalid_name_does_not_abort() {
        let session = session();
        // The C Windows branch does not set `use_fn` when GetShortPathName()
        // succeeds, so an invalid received name does not abort the command.
        let expanded = expand_with(
            "program *N",
            "13b70064.pkt;rm -rf",
            &session,
            Some(short_name_ok),
        );
        assert_eq!(expanded.command, "program 13B700~1.PKT");
        assert!(!expanded.aborted);
        assert_eq!(expanded.short_name_error, None);
    }

    #[test]
    fn the_short_name_is_absent_outside_windows() {
        let session = session();
        let expanded = expand("*N", "13b70064.pkt", &session);
        assert_eq!(expanded.command, "13b70064.pkt");
        assert!(!expanded.aborted);
        assert_eq!(expanded.short_name_error, None);
        // Without a lookup, an invalid name with `*N` aborts.
        let expanded = expand("*N", "13b70064.pkt;rm -rf", &session);
        assert_eq!(expanded.command, "-");
        assert!(expanded.aborted);
        assert_eq!(expanded.short_name_error, None);
    }

    #[test]
    fn the_macros_of_the_addresses() {
        let session = session();
        // The first address, and the address of a number.
        assert_eq!(expand("*A0", "f", &session).command, "2:5047/13@fidonet");
        assert_eq!(expand("*A1", "f", &session).command, "2:5047/13.4@fidonet");
        // The C code answers `-` for an address that the peer does not hold.
        assert_eq!(expand("*A9", "f", &session).command, "-");
        // Every address of the peer, separated by spaces.
        assert_eq!(
            expand("*A@", "f", &session).command,
            "2:5047/13@fidonet 2:5047/13.4@fidonet"
        );
        assert_eq!(
            expand("*A*", "f", &session).command,
            "2:5047/13@fidonet 2:5047/13.4@fidonet"
        );
        // A character that is not a digit comes after the first address.
        assert_eq!(expand("*A.", "f", &session).command, "2:5047/13@fidonet.");
        // The C code answers the first address for `*A` at the end.
        assert_eq!(expand("*A", "f", &session).command, "2:5047/13@fidonet");
    }

    #[test]
    fn the_macros_of_the_session() {
        let session = session();
        assert_eq!(expand("*P", "f", &session).command, "1");
        assert_eq!(expand("*L", "f", &session).command, "0");
        assert_eq!(expand("*H", "f", &session).command, "example.org");
        // The C code answers the address of the peer for a name that it does
        // not accept.
        let mut other = session.clone();
        other.peer_name = "example org".to_string();
        assert_eq!(expand("*H", "f", &other).command, "192.0.2.1");
        // An empty name asks for the address as well.
        other.peer_name = String::new();
        assert_eq!(expand("*H", "f", &other).command, "192.0.2.1");
        // A macro that the C code does not know stays as it is.
        assert_eq!(expand("*X", "f", &session).command, "*X");
        assert_eq!(expand("*", "f", &session).command, "*");
    }

    #[test]
    fn the_security_check_of_a_name() {
        let session = session();
        // The name of the file holds a character that the C code does not
        // accept, and the command holds `*F`, so the C code does not run it.
        let expanded = expand("program *F", "13b70064.pkt;rm -rf", &session);
        assert_eq!(expanded.command, "program -");
        assert!(expanded.aborted);
        // A command without `*F` or `*N` runs in any case.
        let expanded = expand("program", "13b70064.pkt;rm -rf", &session);
        assert_eq!(expanded.command, "program");
        assert!(!expanded.aborted);
        // The characters that the C code accepts.
        let expanded = expand("*F", ".\\-_:/@abc123", &session);
        assert_eq!(expanded.command, ".\\-_:/@abc123");
        assert!(!expanded.aborted);
    }
}
