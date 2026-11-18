//! The `TIME` banner of the session, the port of the time text of `banner()`
//! of `protocol.c`.
//!
//! The C code renders the local time with `sprintf()` and sends it as the
//! `M_NUL TIME` frame. The port renders the same text from a fixed value, so a
//! session and a test produce the same banner.

/// The fields of the `TIME` banner, the port of the `struct tm` and the zone
/// offset that `banner()` of `protocol.c` formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BannerTime {
    /// The year, the port of `tm_year` plus 1900.
    pub year: i32,
    /// The month, 1 to 12, the port of `tm_mon` plus one.
    pub month: u32,
    /// The day of the month, 1 to 31, the port of `tm_mday`.
    pub day: u32,
    /// The hour, the port of `tm_hour`.
    pub hour: u32,
    /// The minute, the port of `tm_min`.
    pub minute: u32,
    /// The second, the port of `tm_sec`.
    pub second: u32,
    /// The day of the week, 0 for Sunday, the port of `tm_wday`.
    pub weekday: u32,
    /// The zone offset in minutes, the value of `tz_off()`.
    pub offset_minutes: i32,
}

impl BannerTime {
    /// Returns the banner of a local time and a zone offset.
    ///
    /// The day of the week comes from the date with the Sakamoto algorithm;
    /// Sunday is 0.
    pub fn new(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        offset_minutes: i32,
    ) -> BannerTime {
        BannerTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            weekday: weekday(year, month, day),
            offset_minutes,
        }
    }

    /// Returns the text of the banner, the C time format of `banner()`.
    pub fn text(&self) -> String {
        const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let day = DAYS.get(self.weekday as usize).copied().unwrap_or("");
        let month = MONTHS
            .get(self.month.wrapping_sub(1) as usize)
            .copied()
            .unwrap_or("");
        let sign = if self.offset_minutes >= 0 { '+' } else { '-' };
        let offset = self.offset_minutes.unsigned_abs();
        format!(
            "{}, {:2} {} {} {:02}:{:02}:{:02} {}{:02}{:02}",
            day,
            self.day,
            month,
            self.year,
            self.hour,
            self.minute,
            self.second,
            sign,
            offset / 60,
            offset % 60
        )
    }
}

/// Returns the day of the week of a date, 0 for Sunday, with the Sakamoto
/// algorithm.
fn weekday(year: i32, month: u32, day: u32) -> u32 {
    const T: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut y = i64::from(year);
    if month < 3 {
        y -= 1;
    }
    let t = T.get(month.wrapping_sub(1) as usize).copied().unwrap_or(0);
    let w = (y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400) + t + i64::from(day))
        .rem_euclid(7);
    w as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_text_of_a_negative_offset() {
        assert_eq!(
            BannerTime::new(2026, 11, 15, 9, 5, 7, -180).text(),
            "Sun, 15 Nov 2026 09:05:07 -0300"
        );
    }

    #[test]
    fn the_time_text_of_a_positive_offset() {
        assert_eq!(
            BannerTime::new(2024, 2, 29, 23, 59, 59, 330).text(),
            "Thu, 29 Feb 2024 23:59:59 +0530"
        );
    }

    #[test]
    fn the_time_text_of_zero() {
        assert_eq!(
            BannerTime::new(1970, 1, 1, 0, 0, 0, 0).text(),
            "Thu,  1 Jan 1970 00:00:00 +0000"
        );
    }

    #[test]
    fn the_weekday_of_year_zero() {
        assert_eq!(
            BannerTime::new(0, 1, 1, 0, 0, 0, 0).text(),
            "Sat,  1 Jan 0 00:00:00 +0000"
        );
    }

    #[test]
    fn the_weekday_of_a_negative_year() {
        assert_eq!(
            BannerTime::new(-1, 1, 1, 0, 0, 0, 0).text(),
            "Fri,  1 Jan -1 00:00:00 +0000"
        );
    }
}
