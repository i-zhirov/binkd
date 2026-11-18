//! The levels of a message, the port of the `lev` argument of `vLog()`.

/// The level of a message of the log, the port of the `lev` argument of
/// `vLog()`.
///
/// The C code passes plain integers and compares them with `<=`. The levels of the
/// reference are in `doc/binkd-ref-en.txt`: 0 is a critical error, 1 an ordinary
/// error, 2 an error on a connection attempt, 3 the extensions and the files, 4 the
/// start of binkd, 5 the DNS and the files, 6 the events of the protocol, 7 the
/// addresses and the data blocks, and 8 and above the debug messages.
pub type Level = i32;

/// The level of a critical error, the port of the level 0 of `vLog()`. The C code
/// calls `exit(1)` for it.
pub const FATAL: Level = 0;
/// The level of an ordinary error, the port of the level 1.
pub const ERROR: Level = 1;
/// The level of a connection attempt, the port of the level 2.
pub const CONNECTION: Level = 2;
/// The level of the files and the extensions, the port of the level 3.
pub const INFO: Level = 3;
/// The level of the start and the high level files, the port of the level 4.
pub const START: Level = 4;
/// The level of the DNS and the sockets, the port of the level 5.
pub const TRACE: Level = 5;
/// The level of the protocol, the port of the level 6.
pub const PROTOCOL: Level = 6;
/// The level of the data blocks, the port of the level 7.
pub const DEBUG: Level = 7;
/// The level of the naming and the addresses, the port of the level 8.
pub const DEBUG2: Level = 8;
/// The level of the proxy, the port of the level 10.
pub const DEBUG3: Level = 10;

/// The character of a level in a line, the port of the `marks` table of `vLog()`.
///
/// The table is `"!?+-"`, so the levels 0 to 3 have a character of their own
/// and every other level a space.
pub fn level_char(level: Level) -> char {
    const MARKS: [char; 4] = ['!', '?', '+', '-'];
    if (0..MARKS.len() as Level).contains(&level) {
        MARKS[level as usize]
    } else {
        ' '
    }
}

/// The names of the months of a line, the port of the `month` table of `vLog()`.
pub(crate) const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks() {
        assert_eq!(level_char(FATAL), '!');
        assert_eq!(level_char(ERROR), '?');
        assert_eq!(level_char(CONNECTION), '+');
        assert_eq!(level_char(INFO), '-');
        assert_eq!(level_char(START), ' ');
        assert_eq!(level_char(DEBUG3), ' ');
        // The level -1 of `ConfigError()` has no character of its own.
        assert_eq!(level_char(-1), ' ');
    }
}
