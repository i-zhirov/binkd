//! The platform values of the binary, the port of the parts of the C code
//! that read the system.

use std::path::Path;

use binkd_log::DateTime;

/// The names of the months, the port of `__DATE__` of the C code.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Returns the time of the system clock, the port of the `time()` and of the
/// `safe_localtime()` of `vLog()` of `tools.c`.
///
/// The C code answers the local time. The port asks the system for the local
/// offset and falls back to UTC when the lookup fails, so a host without zone
/// data still holds a civil time. The `epoch` is the real epoch, and the civil
/// fields and the year are the local time.
pub fn now() -> DateTime {
    let time =
        time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let offset = time.offset();
    DateTime {
        epoch: time.unix_timestamp(),
        year: time.year(),
        day: u32::from(time.day()),
        month: u32::from(u8::from(time.month())),
        hour: u32::from(time.hour()),
        minute: u32::from(time.minute()),
        second: u32::from(time.second()),
        offset_minutes: i32::from(offset.whole_minutes()),
    }
}

/// Returns the date of a local time, the port of `__DATE__` and of `__TIME__`
/// of the C code, for example `Oct 27 2026 02:00:00`.
///
/// The fields and the year of the time are local, so the text holds the local
/// date and time.
pub fn date(time: &DateTime) -> String {
    let month = MONTHS
        .get(time.month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("Jan");
    format!(
        "{} {:2} {} {:02}:{:02}:{:02}",
        month, time.day, time.year, time.hour, time.minute, time.second
    )
}

/// Returns the next random number, the port of `rnd()` of `tools.c`.
///
/// The C code seeds `rand()` with the time on the first call and answers
/// `time(0) + rand()` masked to 32 bits. The port keeps the state of the
/// sequence in the closure, so no global state holds it.
pub fn rnd() -> impl FnMut() -> u32 {
    let mut state = now().epoch as u32 ^ std::process::id();
    move || {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (now().epoch as u32).wrapping_add(state & 0x7fff_ffff)
    }
}

/// Returns the free space of a directory in kilobytes, the port of
/// `getfree()` of `unix/getfree.c` and `nt/getfree.c`.
///
/// `fs2::available_space()` answers bytes. A failed query answers
/// `u64::MAX`, the value of the C `ULONG_MAX` error path.
pub fn free_space(path: &Path) -> u64 {
    fs2::available_space(path)
        .map(|bytes| bytes / 1024)
        .unwrap_or(u64::MAX)
}

/// Returns the name of the system, the port of `uname()` of
/// `get_os_string()` of `tools.c`.
///
/// The C code answers the `sysname` of `uname()`, for example `Darwin`. The
/// port answers the name of `std`, which is `macos` on macOS.
pub fn system() -> &'static str {
    std::env::consts::OS
}

/// Returns the civil date of the days since the epoch: the year, the month,
/// and the day, the algorithm of Howard Hinnant.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_civil_date_of_an_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19723), (2024, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn the_date_of_a_time() {
        let time = DateTime {
            epoch: 1_800_000_000,
            year: 2027,
            day: 25,
            month: 1,
            hour: 23,
            minute: 57,
            second: 8,
            offset_minutes: 0,
        };
        assert_eq!(date(&time), "Jan 25 2027 23:57:08");
    }

    #[test]
    fn the_local_offset_of_now() {
        let time = now();
        assert!(
            (-720..=840).contains(&time.offset_minutes),
            "the offset is {} minutes",
            time.offset_minutes
        );
        assert!(
            (1970..=9999).contains(&time.year),
            "the year is {}",
            time.year
        );
    }

    #[test]
    fn the_system_holds_a_name() {
        assert!(!system().is_empty());
    }

    #[test]
    fn the_free_space_of_a_directory() {
        let dir = std::env::temp_dir().join(format!("binkd-free-space-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(free_space(&dir) > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_free_space_of_a_missing_directory() {
        // A missing directory of an existing volume answers the volume on
        // Windows, so the error path needs a missing drive.
        assert_eq!(
            free_space(Path::new("Z:/binkd-free-space-missing")),
            u64::MAX
        );
    }
}
