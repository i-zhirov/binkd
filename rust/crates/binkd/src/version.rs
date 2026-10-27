//! The version text of binkd, the port of the version block of `main()` of
//! `binkd.c`.

/// The name of the mailer, the port of `MYNAME` of `Config.h`.
pub const NAME: &str = "binkd";

/// The version of the port, the port of `MYVER` of `Config.h`.
///
/// The C code states the version of the C project in `Config.h`, and the
/// release process updates it. The port states the version of its own package,
/// as `mailer()` of `binkd-net` does.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The build flags of the port, the port of `_DBNKD` of `confopt.h`.
pub const FLAGS: &str = "rustc";

/// The facilities of the port, the port of the facilities of `main()` of
/// `binkd.c`.
pub const FACILITIES: &str = "ipv6";

/// Returns the line of the `-v` switch, the port of the version block of
/// `main()` of `binkd.c`.
///
/// The C code states the version, the build date and time of the compiler,
/// and the system name of `uname()`. The port states the version of its own
/// package and takes the date and the system name from the caller, as
/// `binkd-log` takes the local time.
pub fn line(date: &str, system: &str) -> String {
    format!("Binkd {VERSION} ({date}/{system})\n")
}

/// Returns the text of the `-vv` switches, the port of the version block of
/// `main()` of `binkd.c`.
pub fn verbose(date: &str, system: &str) -> String {
    format!(
        "{}Compilation flags: {FLAGS}.\nFacilities: {FACILITIES}\n",
        line(date, system)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_holds_the_version_and_the_parenthesis() {
        let text = line("Oct 27 2026 02:00:00", "Darwin");
        assert!(text.starts_with("Binkd "));
        assert!(text.ends_with("(Oct 27 2026 02:00:00/Darwin)\n"));
        assert!(text.contains(VERSION));
    }

    #[test]
    fn the_verbose_text_holds_the_flags_and_the_facilities() {
        let text = verbose("Oct 27 2026 02:00:00", "Darwin");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1], format!("Compilation flags: {FLAGS}."));
        assert_eq!(lines[2], format!("Facilities: {FACILITIES}"));
    }
}
