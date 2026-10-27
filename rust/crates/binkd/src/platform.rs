//! The platform values of the binary, the port of the parts of the C code
//! that read the system.

use binkd_log::DateTime;

/// The names of the months, the port of `__DATE__` of the C code.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Returns the time of the system clock, the port of the `time()` and of the
/// `safe_localtime()` of `vLog()` of `tools.c`.
///
/// The C code answers the local time. The port answers the time in UTC until
/// the platform layer holds the timezone of the system.
pub fn now() -> DateTime {
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = epoch.div_euclid(86400);
    let rest = epoch.rem_euclid(86400);
    let (_, month, day) = civil_from_days(days);
    DateTime {
        epoch,
        day,
        month,
        hour: (rest / 3600) as u32,
        minute: (rest % 3600 / 60) as u32,
        second: (rest % 60) as u32,
    }
}

/// Returns the date of a time, the port of `__DATE__` and of `__TIME__` of
/// the C code, for example `Oct 27 2026 02:00:00`.
pub fn date(time: &DateTime) -> String {
    let (year, _, _) = civil_from_days(time.epoch.div_euclid(86400));
    let month = MONTHS
        .get(time.month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("Jan");
    format!(
        "{} {:2} {} {:02}:{:02}:{:02}",
        month, time.day, year, time.hour, time.minute, time.second
    )
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
            day: 25,
            month: 1,
            hour: 23,
            minute: 57,
            second: 8,
        };
        assert_eq!(date(&time), "Jan 25 2027 23:57:08");
    }

    #[test]
    fn the_system_holds_a_name() {
        assert!(!system().is_empty());
    }
}
