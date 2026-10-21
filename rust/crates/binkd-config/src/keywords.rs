//! The keyword table of `readcfg.c`.
//!
//! The order of the table is the order of the `-d` dump of the C code, so a
//! keyword stays in its place.
//!
//! The C code has one entry per keyword with the callback and the target of the
//! value. The port dispatches on the name and keeps the table for the order and
//! the lookup.

/// The keywords of the config file, in the order of the C table.
pub const KEYWORDS: &[&str] = &[
    "include",
    "passwords",
    "log",
    "loglevel",
    "conlog",
    "binlog",
    "fdinhist",
    "fdouthist",
    "tzoff",
    "domain",
    "address",
    "sysname",
    "bindaddr",
    "listen",
    "sysop",
    "location",
    "nodeinfo",
    "iport",
    "oport",
    "rescan-delay",
    "call-delay",
    "timeout",
    "oblksize",
    "maxservers",
    "maxclients",
    "inbound",
    "inbound-nonsecure",
    "temp-inbound",
    "node",
    "defnode",
    "kill-dup-partial-files",
    "kill-old-partial-files",
    "kill-old-bsy",
    "percents",
    "minfree",
    "minfree-nonsecure",
    "flag",
    "exec",
    "printq",
    "try",
    "hold",
    "hold-skipped",
    "backresolv",
    "pid-file",
    "remove-try-files",
    "proxy",
    "socks",
    "syslog",
    "ftrans",
    "send-if-pwd",
    "root-domain",
    "prescan",
    "connect-timeout",
    "filebox",
    "brakebox",
    "deletebox",
    "skip",
    "skipmask",
    "inboundcase",
    "deletedirs",
    "overwrite",
    "dont-send-empty",
    "rename-style",
    "share",
    "check-pkthdr",
    "aso",
    "perl-hooks",
    "perl-dll",
    "perl-strict",
    "perl-var",
    "nolog",
    "hide-aka",
    "present-aka",
    "zlevel",
    "zminsize",
    "zallow",
    "zdeny",
    "zlib-dll",
    "bzlib2-dll",
    "limit-rate",
];

/// Returns true when the name is a keyword. The lookup ignores case.
pub fn is_keyword(name: &str) -> bool {
    position(name).is_some()
}

/// Returns the position of a keyword in the table.
pub fn position(name: &str) -> Option<usize> {
    KEYWORDS.iter().position(|k| k.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup() {
        assert!(is_keyword("INCLUDE"));
        assert!(is_keyword("defnode"));
        assert!(!is_keyword("nosuchkeyword"));
        assert_eq!(position("include"), Some(0));
    }
}
