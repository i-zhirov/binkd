//! The port names, the port of `find_port()` of `iptools.c`.

/// The name of the protocol port.
pub const DEFAULT_PORT: &str = "binkp";

/// Resolves a port name or a port number.
///
/// The C code asks the service database of the system. The port takes a closure,
/// so the parser stays free of the system calls.
pub type PortResolver<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Returns the port of a name or a number.
///
/// The C code resolves the name with `getaddrinfo()` and keeps the name. The
/// default resolver of the port knows the numbers, the name of the protocol, and a
/// few common service names.
pub fn resolve(port: &str) -> Option<String> {
    if port.is_empty() {
        return Some(DEFAULT_PORT.to_string());
    }
    if port.bytes().all(|c| c.is_ascii_digit()) {
        return Some(port.to_string());
    }
    const SERVICES: &[&str] = &["binkp", "ftp", "http", "https", "smtp", "ssh"];
    if SERVICES.iter().any(|s| s.eq_ignore_ascii_case(port)) {
        return Some(port.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_port() {
        assert_eq!(resolve("").as_deref(), Some("binkp"));
        assert_eq!(resolve("24554").as_deref(), Some("24554"));
        assert_eq!(resolve("BINKP").as_deref(), Some("BINKP"));
        assert_eq!(resolve("nosuchservice"), None);
    }
}
