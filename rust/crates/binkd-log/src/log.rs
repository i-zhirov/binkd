//! The text log, the port of the log part of `tools.c`.
//!
//! The C code keeps the state in the variables `current_loglevel`,
//! `current_conlog`, `current_logpath`, and `current_nolog` of `tools.c` and
//! writes to `stderr` and to the file for every message. The port keeps the
//! state in [`Log`] and renders the two lines with [`file_line()`] and
//! [`console_line()`], so a test reads them without a file.

use std::io::Write;
use std::sync::Mutex;

use crate::level::{level_char, Level, MONTHS};

/// The broken down local time of a message, the port of the `struct tm` that
/// `vLog()` fills with `safe_localtime()`.
///
/// The C code uses the local time, so the caller supplies it: `std` has no local
/// time, and the platform layer of the binary owns the timezone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    /// The time in seconds since the epoch, the port of `time_t`.
    pub epoch: i64,
    /// The day of the month, 1 to 31, the port of `tm_mday`.
    pub day: u32,
    /// The month, 1 to 12, the port of `tm_mon` plus one.
    pub month: u32,
    /// The hour, the port of `tm_hour`.
    pub hour: u32,
    /// The minute, the port of `tm_min`.
    pub minute: u32,
    /// The second, the port of `tm_sec`.
    pub second: u32,
}

/// The result of a message, the port of the return of `vLog()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The message was written, or the log holds no path for it.
    Written,
    /// The level is 0: the C code calls `exit(1)`, and the caller of the port
    /// must end the process with the code 1.
    Fatal,
}

/// The text log of binkd, the port of the state of `vLog()` of `tools.c`.
///
/// The C code keeps the state in process wide variables and opens and closes the
/// file for every message. The port keeps the state in one value that several
/// tasks may share, so a session writes to the log of its process.
pub struct Log {
    /// The state of the log, the port of the variables of `vLog()`.
    inner: Mutex<Inner>,
}

/// The state of a log.
struct Inner {
    /// The level of the file, the port of `current_loglevel`.
    level: Level,
    /// The level of the console, the port of `current_conlog`.
    console_level: Level,
    /// The path of the file, or empty, the port of `current_logpath`.
    path: String,
    /// The masks of the `nolog` directive, the port of `current_nolog`.
    nolog: Vec<String>,
    /// The console is off, the port of `inetd_flag` of `vLog()`.
    no_console: bool,
    /// The process id of the lines, the port of `PID()`.
    pid: u32,
    /// No message of the file was written yet, the port of the `first_time`
    /// flag of `vLog()`: the first line starts with a blank line.
    first: bool,
}

impl Log {
    /// Creates a log, the port of `InitLog()` of `tools.c`.
    ///
    /// The C code starts with the level 1 and the console level 1, so a log
    /// without a path still writes the errors to the console.
    pub fn new(level: Level, console_level: Level, path: &str, nolog: Vec<String>) -> Log {
        Log {
            inner: Mutex::new(Inner {
                level,
                console_level,
                path: path.to_string(),
                nolog,
                no_console: false,
                pid: std::process::id(),
                first: true,
            }),
        }
    }

    /// Sets the state of the log, the port of `InitLog()`.
    pub fn init(&self, level: Level, console_level: Level, path: &str, nolog: Vec<String>) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        inner.level = level;
        inner.console_level = console_level;
        inner.path = path.to_string();
        inner.nolog = nolog;
    }

    /// Sets the console of the log, the port of the `conlog` directive.
    pub fn set_console(&self, no_console: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.no_console = no_console;
        }
    }

    /// Writes a message, the port of `vLog()` of `tools.c`.
    ///
    /// The console line goes to `stderr` when the level is at most the console
    /// level and the console is not off. The file line goes to the path when the
    /// level is at most the level of the file. A message of the level 0 is fatal,
    /// even when the `nolog` masks or a missing path suppress the output.
    pub fn message(&self, level: Level, text: &str, now: &DateTime) -> Outcome {
        let Ok(mut inner) = self.inner.lock() else {
            return Outcome::Written;
        };
        // The port of the `mask_test()` filter of `vLog()`: the first mask that
        // matches the message suppresses the whole output.
        let suppressed = inner
            .nolog
            .iter()
            .any(|mask| binkd_ftn::pattern::pmatch_ncase(mask.as_bytes(), text.as_bytes()));
        if !suppressed {
            if level <= inner.console_level && !inner.no_console {
                let line = console_line(level, now, inner.pid, text);
                let mut stderr = std::io::stderr();
                let _ = write!(stderr, "{line}");
                let _ = stderr.flush();
                // A negative level goes to the console alone.
                if level < 0 {
                    return Outcome::Written;
                }
            }
            if level <= inner.level && !inner.path.is_empty() {
                let line = file_line(level, now, inner.pid, text, inner.first);
                // The C code tries the open ten times.
                for _ in 0..10 {
                    let file = std::fs::OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(&inner.path);
                    if let Ok(mut file) = file {
                        if file.write_all(line.as_bytes()).is_ok() {
                            inner.first = false;
                        }
                        break;
                    }
                }
            }
        }
        if level == crate::level::FATAL {
            return Outcome::Fatal;
        }
        Outcome::Written
    }
}

/// Returns the line of the console, the port of the `fprintf(stderr, ...)` of
/// `vLog()`.
///
/// The format is 30 spaces, a carriage return, the character of the level, the
/// hour and the minute, the process id, and the message.
pub fn console_line(level: Level, now: &DateTime, pid: u32, text: &str) -> String {
    format!(
        "{:>30}\r{} {:02}:{:02} [{}] {}",
        "",
        level_char(level),
        now.hour,
        now.minute,
        pid,
        text
    )
}

/// Returns the line of the file, the port of the `fprintf(logfile, ...)` of
/// `vLog()`.
///
/// The format is the character of the level, the day, the name of the month, the
/// time, the process id, and the message. The first line of the file starts with
/// a blank line. The C code always ends the line with a newline, and the caller
/// appends the newline of the message itself.
pub fn file_line(level: Level, now: &DateTime, pid: u32, text: &str, first: bool) -> String {
    let month = MONTHS
        .get(now.month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("Jan");
    format!(
        "{}{} {:02} {} {:02}:{:02}:{:02} [{}] {}\n",
        if first { "\n" } else { "" },
        level_char(level),
        now.day,
        month,
        now.hour,
        now.minute,
        now.second,
        pid,
        text
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{DEBUG, FATAL, INFO};

    fn time() -> DateTime {
        DateTime {
            epoch: 1_600_991_262,
            day: 3,
            month: 10,
            hour: 9,
            minute: 5,
            second: 7,
        }
    }

    #[test]
    fn console_format() {
        let line = console_line(INFO, &time(), 1234, "hello");
        // The level 3 has the character `-` of the `marks` table.
        assert_eq!(line, format!("{}\r- 09:05 [1234] hello", " ".repeat(30)));
    }

    #[test]
    fn file_format() {
        let line = file_line(INFO, &time(), 1234, "hello", false);
        assert_eq!(line, "- 03 Oct 09:05:07 [1234] hello\n");
    }

    #[test]
    fn file_first_line() {
        let line = file_line(DEBUG, &time(), 1234, "hello", true);
        assert_eq!(line, "\n  03 Oct 09:05:07 [1234] hello\n");
    }

    #[test]
    fn fatal_stays_fatal_under_a_mask() {
        let log = Log::new(FATAL, FATAL, "", vec!["*".to_string()]);
        assert_eq!(log.message(FATAL, "error", &time()), Outcome::Fatal);
    }

    #[test]
    fn a_mask_suppresses_the_file() {
        let dir = std::env::temp_dir().join(format!("binkd-log-{}", std::process::id()));
        let _ = std::fs::remove_file(&dir);
        let log = Log::new(
            DEBUG,
            FATAL,
            &dir.display().to_string(),
            vec!["*socket*".to_string()],
        );
        assert_eq!(
            log.message(INFO, "socket closed", &time()),
            Outcome::Written
        );
        assert!(!dir.exists(), "the mask did not suppress the file");
        assert_eq!(log.message(INFO, "file closed", &time()), Outcome::Written);
        let text = std::fs::read_to_string(&dir).unwrap();
        // The first line of the file starts with a blank line.
        assert!(text.starts_with("\n- 03 Oct 09:05:07"), "{text:?}");
        let _ = std::fs::remove_file(&dir);
    }
}
