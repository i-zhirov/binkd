//! The port names, the port of `find_port()` of `iptools.c`.

/// The port of the binkp protocol, the port of `PRTCLNAME` of `Config.h`.
pub const BINKP: u16 = 24554;

/// The port of a protocol that the config names no port for, the port of
/// `DEF_PORT` of `Config.h`.
pub const DEFAULT_PORT: u16 = BINKP;

/// Returns the port of a name or a number, the port of `find_port()` of
/// `iptools.c`.
///
/// The C code asks the system for the service of the name with
/// `getaddrinfo()`. The port holds the names that the C code uses together
/// with the numbers, so the port runs without the services database of the
/// system. An unknown name is an error, the port of the `NULL` of the C code.
pub fn find_port(name: &str) -> Option<u16> {
    if name.is_empty() {
        return Some(BINKP);
    }
    if let Ok(number) = name.parse::<u16>() {
        return Some(number);
    }
    match name.to_ascii_lowercase().as_str() {
        "binkp" => Some(BINKP),
        "ftp" => Some(21),
        "ssh" => Some(22),
        "smtp" => Some(25),
        "http" | "www" | "www-http" => Some(80),
        "pop3" => Some(110),
        "imap" => Some(143),
        "socks" | "socks4" => Some(1080),
        "squid" | "http-proxy" => Some(3128),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_numbers() {
        assert_eq!(find_port(""), Some(BINKP));
        assert_eq!(find_port("binkp"), Some(24554));
        assert_eq!(find_port("BINKP"), Some(24554));
        assert_eq!(find_port("24554"), Some(24554));
        assert_eq!(find_port("socks"), Some(1080));
        assert_eq!(find_port("no-such-service"), None);
    }
}
