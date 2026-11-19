//! The local syslog backend of the log, the port of the syslog branch of
//! `vLog()` of `tools.c`.
//!
//! The C code writes every message of a level at most `current_loglevel` to the
//! local syslog when `syslog_facility` is not negative. The port keeps the
//! platform code out of `rbinkd-log`: this module builds the sink from the
//! `syslog` crate and installs it on the log.

use std::sync::{Arc, Mutex};

use rbinkd_log::{Level, Log, Sink};
use syslog::{Facility, Formatter3164, Logger, LoggerBackend, Severity};

/// Configures the syslog sink of the log, the port of the `openlog()` call of
/// the syslog branch of `vLog()` of `tools.c`.
///
/// A negative facility disables the sink, so the log stops sending messages to
/// syslog. A non-negative facility builds a new sink and installs it on the log.
/// A failed local connection keeps the old sink, so a reload cannot lose the
/// working output of a process that is already connected.
pub(crate) fn configure(log: &Log, facility: i32) {
    if facility < 0 {
        log.set_sink(None);
        return;
    }
    if let Some(sink) = SyslogSink::new(facility) {
        log.set_sink(Some(Arc::new(sink)));
    }
}

/// Returns the facility of a config value, the port of the `facilitynames`
/// table of the C code.
///
/// The values of a config are the ones of the `syslog` directive, the
/// `LOG_*` masks of the C system header. An unknown value answers `None`.
fn facility(value: i32) -> Option<Facility> {
    let facility = match value {
        0 => Facility::LOG_KERN,
        8 => Facility::LOG_USER,
        16 => Facility::LOG_MAIL,
        24 => Facility::LOG_DAEMON,
        32 => Facility::LOG_AUTH,
        40 => Facility::LOG_SYSLOG,
        48 => Facility::LOG_LPR,
        56 => Facility::LOG_NEWS,
        64 => Facility::LOG_UUCP,
        72 => Facility::LOG_CRON,
        80 => Facility::LOG_AUTHPRIV,
        88 => Facility::LOG_FTP,
        96 => Facility::LOG_NTP,
        104 => Facility::LOG_AUDIT,
        112 => Facility::LOG_ALERT,
        120 => Facility::LOG_CLOCK_DAEMON,
        128 => Facility::LOG_LOCAL0,
        136 => Facility::LOG_LOCAL1,
        144 => Facility::LOG_LOCAL2,
        152 => Facility::LOG_LOCAL3,
        160 => Facility::LOG_LOCAL4,
        168 => Facility::LOG_LOCAL5,
        176 => Facility::LOG_LOCAL6,
        184 => Facility::LOG_LOCAL7,
        _ => return None,
    };
    Some(facility)
}

/// Returns the syslog priority of a level, the port of the `log_levels` table
/// of the syslog branch of `vLog()` of `tools.c`.
///
/// The level 0 is `LOG_ERR`, the level 1 is `LOG_WARNING`, the level 2 is
/// `LOG_NOTICE`, the levels 3 to 6 are `LOG_INFO`, and every other level is
/// `LOG_DEBUG`.
fn priority(level: Level) -> Severity {
    match level {
        0 => Severity::LOG_ERR,
        1 => Severity::LOG_WARNING,
        2 => Severity::LOG_NOTICE,
        3..=6 => Severity::LOG_INFO,
        _ => Severity::LOG_DEBUG,
    }
}

/// Writes a message with a severity, the port of the `vsyslog()` call of the
/// syslog branch of `vLog()` of `tools.c`.
///
/// The `syslog` crate exposes one method per severity, so the port matches the
/// severity once and ignores the result: a failed write must not end the
/// daemon, and the text log still works.
fn write_severity<W: std::io::Write>(
    logger: &mut Logger<W, Formatter3164>,
    severity: Severity,
    text: &str,
) {
    match severity {
        Severity::LOG_EMERG => {
            let _ = logger.emerg(text);
        }
        Severity::LOG_ALERT => {
            let _ = logger.alert(text);
        }
        Severity::LOG_CRIT => {
            let _ = logger.crit(text);
        }
        Severity::LOG_ERR => {
            let _ = logger.err(text);
        }
        Severity::LOG_WARNING => {
            let _ = logger.warning(text);
        }
        Severity::LOG_NOTICE => {
            let _ = logger.notice(text);
        }
        Severity::LOG_INFO => {
            let _ = logger.info(text);
        }
        Severity::LOG_DEBUG => {
            let _ = logger.debug(text);
        }
    }
}

/// The local syslog output of the log, the port of the syslog branch of
/// `vLog()` of `tools.c`.
///
/// The C code keeps one `openlog()` state for the process and calls
/// `vsyslog()` for every message. The port keeps the logger behind a mutex,
/// because the messages of several sessions share it.
struct SyslogSink {
    /// The logger of the local syslog, the port of the state of `openlog()`.
    logger: Mutex<Logger<LoggerBackend, Formatter3164>>,
}

impl SyslogSink {
    /// Connects to the local syslog with a facility, the port of `openlog()`.
    ///
    /// The process name is `rbinkd` and the record holds the process id, the port
    /// of the `LOG_PID` argument of `openlog()`. A host without a local syslog
    /// answers `None` instead of failing the daemon.
    fn new(facility: i32) -> Option<SyslogSink> {
        let formatter = Formatter3164 {
            facility: self::facility(facility)?,
            hostname: None,
            process: "rbinkd".into(),
            pid: std::process::id(),
        };
        let logger = syslog::unix(formatter).ok()?;
        Some(SyslogSink {
            logger: Mutex::new(logger),
        })
    }
}

impl Sink for SyslogSink {
    fn write(&self, level: Level, text: &str) {
        let Ok(mut logger) = self.logger.lock() else {
            return;
        };
        write_severity(&mut logger, priority(level), text);
    }
}

#[cfg(test)]
mod tests {
    use super::{configure, facility, priority, write_severity};
    use rbinkd_log::{Level, Log, Sink, DEBUG, FATAL, INFO};
    use std::sync::{Arc, Mutex};
    use syslog::{Facility, Formatter3164, Logger, Severity};

    /// Records the level and the text of every message.
    struct Record(Arc<Mutex<Vec<(Level, String)>>>);

    impl Sink for Record {
        fn write(&self, level: Level, text: &str) {
            if let Ok(mut records) = self.0.lock() {
                records.push((level, text.to_string()));
            }
        }
    }

    fn now() -> rbinkd_log::DateTime {
        rbinkd_log::DateTime {
            epoch: 0,
            year: 2020,
            day: 1,
            month: 1,
            hour: 0,
            minute: 0,
            second: 0,
            offset_minutes: 0,
        }
    }

    #[test]
    fn the_facility_of_a_config_value() {
        assert!(matches!(facility(0), Some(Facility::LOG_KERN)));
        assert!(matches!(facility(8), Some(Facility::LOG_USER)));
        assert!(matches!(facility(24), Some(Facility::LOG_DAEMON)));
        assert!(matches!(facility(32), Some(Facility::LOG_AUTH)));
        assert!(matches!(facility(80), Some(Facility::LOG_AUTHPRIV)));
        assert!(matches!(facility(128), Some(Facility::LOG_LOCAL0)));
        assert!(matches!(facility(184), Some(Facility::LOG_LOCAL7)));
        assert!(facility(200).is_none());
    }

    #[test]
    fn the_priority_of_a_level() {
        assert!(matches!(priority(0), Severity::LOG_ERR));
        assert!(matches!(priority(1), Severity::LOG_WARNING));
        assert!(matches!(priority(2), Severity::LOG_NOTICE));
        assert!(matches!(priority(3), Severity::LOG_INFO));
        assert!(matches!(priority(4), Severity::LOG_INFO));
        assert!(matches!(priority(5), Severity::LOG_INFO));
        assert!(matches!(priority(6), Severity::LOG_INFO));
        assert!(matches!(priority(7), Severity::LOG_DEBUG));
        assert!(matches!(priority(8), Severity::LOG_DEBUG));
        assert!(matches!(priority(-1), Severity::LOG_DEBUG));
    }

    #[test]
    fn the_write_path_uses_the_priority() {
        for (level, pri) in [(0, 3), (1, 4), (2, 5), (3, 6), (7, 7)] {
            let formatter = Formatter3164 {
                facility: Facility::LOG_KERN,
                hostname: None,
                process: "rbinkd".into(),
                pid: 0,
            };
            let mut logger: Logger<Vec<u8>, Formatter3164> = Logger::new(Vec::new(), formatter);
            write_severity(&mut logger, priority(level), "hello");
            let text = String::from_utf8_lossy(&logger.backend);
            assert!(
                text.starts_with(&format!("<{pri}>")),
                "level {level}: {text:?}"
            );
        }
    }

    #[test]
    fn the_disabled_facility_removes_the_sink() {
        let records = Arc::new(Mutex::new(Vec::new()));
        let log = Log::new(DEBUG, FATAL, "", Vec::new());
        log.set_sink(Some(Arc::new(Record(Arc::clone(&records)))));
        configure(&log, -1);
        log.message(INFO, "hello", &now());
        assert!(
            records.lock().unwrap().is_empty(),
            "the disabled facility left the sink in place"
        );
    }
}
