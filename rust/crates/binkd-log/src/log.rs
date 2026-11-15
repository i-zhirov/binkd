//! The text log, the port of the log part of `tools.c`.
//!
//! The C code keeps the state in the variables `current_loglevel`,
//! `current_conlog`, `current_logpath`, and `current_nolog` of `tools.c` and
//! writes to `stderr` and to the file for every message. The port keeps the
//! state in [`Log`] and renders the two lines with [`file_line()`] and
//! [`console_line()`], so a test reads them without a file.

use std::cell::Cell;
use std::io::Write;
use std::sync::{Arc, Mutex, RwLock};

use crate::level::{level_char, Level, MONTHS};

/// A hook of the log, the port of the call of `perl_on_log()` in `vLog()` of
/// `tools.c`.
///
/// The C code calls the hook of the config before the `nolog` filter, so the
/// hook may replace the message and the level. The port does the same. The
/// hook answers true when the message goes to the output, and false when the
/// hook suppresses it.
///
/// The message is bytes, the port of the buffer of `vLog()`. The port renders
/// the changed message as UTF-8, so an invalid byte becomes the replacement
/// character; the C code writes the bytes.
///
/// The hook may be called from several threads. The guard against the recursion
/// is per thread, not process-wide: a hook that calls `Log` on the same thread
/// is skipped, and a hook on another thread runs. A hook MUST be `Send + Sync`
/// and MUST serialize the state that it shares.
pub trait Hook: Send + Sync {
    /// Calls the hook on a message and its level, the port of `perl_on_log()`.
    fn on_log(&self, message: &mut Vec<u8>, level: &mut Level) -> bool;
}

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
///
/// The state of the log is behind a mutex, and the guard against a recursive
/// call of the hook is thread local. Do not replace the guard with a process
/// wide flag: two sessions of the port run on two threads, and a process wide
/// flag makes one of them skip the hook.
pub struct Log {
    /// The state of the log, the port of the variables of `vLog()`.
    inner: Mutex<Inner>,
    /// The hook of the log, the port of `perl_on_log()`.
    hook: RwLock<Option<Arc<dyn Hook>>>,
}

thread_local! {
    /// The hook of the log runs on this thread, the port of the `__in_log_`
    /// check of `perl_on_log()`.
    ///
    /// The C code holds one flag for the process. The port holds one for the
    /// thread, so two sessions may call the hook; the hook of the port
    /// serializes the interpreter.
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
}

/// Clears the mark of a running hook on the way out, even after a panic.
struct Running;

impl Drop for Running {
    fn drop(&mut self) {
        IN_HOOK.with(|in_hook| in_hook.set(false));
    }
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
            hook: RwLock::new(None),
        }
    }

    /// Sets the hook of the log, the port of the hook of `vLog()`.
    ///
    /// A hook of `None` removes the hook. The crate holds no dependency of the
    /// port of the hooks of Perl, because that crate depends on this one. The
    /// caller supplies the hook as a trait object.
    pub fn set_hook(&self, hook: Option<Arc<dyn Hook>>) {
        if let Ok(mut slot) = self.hook.write() {
            *slot = hook;
        }
    }

    /// Returns the hook of the log, when the log holds one.
    fn hook(&self) -> Option<Arc<dyn Hook>> {
        self.hook.read().ok().and_then(|slot| slot.clone())
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
        let mut level = level;
        let mut message = text.as_bytes().to_vec();
        // The port of the call of `perl_on_log()` in `vLog()`: the hook runs
        // before the `nolog` filter and it may replace the message and the
        // level. A hook that runs on this thread is not called again: the port
        // holds the check for the thread, so two sessions may call the hook
        // and the hook itself serializes the interpreter.
        if let Some(hook) = self.hook() {
            let enter = IN_HOOK.with(|in_hook| {
                if in_hook.get() {
                    false
                } else {
                    in_hook.set(true);
                    true
                }
            });
            let keep = if enter {
                let _running = Running;
                hook.on_log(&mut message, &mut level)
            } else {
                true
            };
            if !keep {
                // The C code sets `ok` to 0 and still ends the program on the
                // fatal level.
                return if level == crate::level::FATAL {
                    Outcome::Fatal
                } else {
                    Outcome::Written
                };
            }
        }
        let text = String::from_utf8_lossy(&message);
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
                let line = console_line(level, now, inner.pid, text.as_ref());
                let mut stderr = std::io::stderr();
                // The C code ends the line of a level that is no error of
                // the config, and the message itself ends the line of a
                // negative level.
                if level >= 0 {
                    let _ = writeln!(stderr, "{line}");
                } else {
                    let _ = write!(stderr, "{line}");
                }
                let _ = stderr.flush();
                // A negative level goes to the console alone.
                if level < 0 {
                    return Outcome::Written;
                }
            }
            if level <= inner.level && !inner.path.is_empty() {
                let line = file_line(level, now, inner.pid, text.as_ref(), inner.first);
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Returns a path of a log file of a test, without an old file.
    fn temp(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("binkd-log-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// Replaces the message and raises the level of a fatal message.
    struct Rewrite;

    impl Hook for Rewrite {
        fn on_log(&self, message: &mut Vec<u8>, level: &mut Level) -> bool {
            *message = b"rewritten".to_vec();
            *level = crate::level::ERROR;
            true
        }
    }

    /// Suppresses every message.
    struct Suppress;

    impl Hook for Suppress {
        fn on_log(&self, _message: &mut Vec<u8>, _level: &mut Level) -> bool {
            false
        }
    }

    /// Writes a message while it runs.
    struct Reenter {
        log: Arc<Log>,
        calls: Arc<AtomicUsize>,
    }

    impl Hook for Reenter {
        fn on_log(&self, _message: &mut Vec<u8>, _level: &mut Level) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.log.message(INFO, "inner", &time());
            true
        }
    }

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

    #[test]
    fn a_hook_replaces_the_message_and_the_level() {
        let path = temp("hook-rewrite");
        let log = Log::new(DEBUG, FATAL, &path.display().to_string(), Vec::new());
        log.set_hook(Some(Arc::new(Rewrite)));
        // The hook raises the fatal level, so the message is not fatal.
        assert_eq!(log.message(FATAL, "hello", &time()), Outcome::Written);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("rewritten"), "{text:?}");
        // The level 1 has the character `?` of the `marks` table.
        assert!(text.contains("? 03 Oct"), "{text:?}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_hook_suppresses_the_message() {
        let path = temp("hook-suppress");
        let log = Log::new(DEBUG, FATAL, &path.display().to_string(), Vec::new());
        log.set_hook(Some(Arc::new(Suppress)));
        assert_eq!(log.message(INFO, "hello", &time()), Outcome::Written);
        assert!(!path.exists(), "the hook did not suppress the file");
        // The fatal level still ends the program, the port of `vLog()`.
        assert_eq!(log.message(FATAL, "fatal", &time()), Outcome::Fatal);
        assert!(!path.exists(), "the hook did not suppress the fatal file");
    }

    #[test]
    fn the_mask_sees_the_message_of_the_hook() {
        let path = temp("hook-mask");
        let log = Log::new(
            DEBUG,
            FATAL,
            &path.display().to_string(),
            vec!["rewritten".to_string()],
        );
        log.set_hook(Some(Arc::new(Rewrite)));
        assert_eq!(log.message(INFO, "hello", &time()), Outcome::Written);
        assert!(!path.exists(), "the mask did not see the changed message");
    }

    struct Count {
        calls: Arc<AtomicUsize>,
    }

    impl Hook for Count {
        fn on_log(&self, _message: &mut Vec<u8>, _level: &mut Level) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(1));
            true
        }
    }

    #[test]
    fn the_hook_runs_on_two_threads() {
        let log = Arc::new(Log::new(DEBUG, FATAL, "", Vec::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        log.set_hook(Some(Arc::new(Count {
            calls: Arc::clone(&calls),
        })));
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let mut threads = Vec::new();
        for _ in 0..2 {
            let log = Arc::clone(&log);
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..50 {
                    assert_eq!(log.message(INFO, "message", &time()), Outcome::Written);
                }
            }));
        }
        for thread in threads {
            thread.join().expect("the thread panicked");
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            100,
            "a thread skipped the hook"
        );
    }

    #[test]
    fn a_hook_does_not_call_itself() {
        let path = temp("hook-reentry");
        let log = Arc::new(Log::new(
            DEBUG,
            FATAL,
            &path.display().to_string(),
            Vec::new(),
        ));
        let calls = Arc::new(AtomicUsize::new(0));
        log.set_hook(Some(Arc::new(Reenter {
            log: Arc::clone(&log),
            calls: Arc::clone(&calls),
        })));
        assert_eq!(log.message(INFO, "outer", &time()), Outcome::Written);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("outer") && text.contains("inner"), "{text:?}");
        log.set_hook(None);
        let _ = std::fs::remove_file(&path);
    }
}
