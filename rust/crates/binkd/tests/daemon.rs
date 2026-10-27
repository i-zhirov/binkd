//! The daemon startup of the binary: the `BEGIN` line, the pid file, the
//! signals, the exit, and the cleanup.
//!
//! The test runs the binary with a config of a temporary directory, waits for
//! the pid file, sends the signals with the `kill` utility, and reads the log.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Returns the path of the binary of the port.
fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_binkd"))
}

/// Builds a config of the daemon for the test in a directory and answers its
/// path.
fn write_config(dir: &Path) -> String {
    let _ = fs::create_dir_all(dir.join("in"));
    let _ = fs::create_dir_all(dir.join("outb"));
    let text = format!(
        "domain fidonet {} 2\n\
         address 2:5047/13@fidonet\n\
         sysname \"Test system\"\n\
         sysop \"Test sysop\"\n\
         location \"Test location\"\n\
         nodeinfo \"Test node\"\n\
         inbound {}\n\
         log {}\n\
         loglevel 8\n\
         conlog 8\n\
         pid-file {}\n",
        dir.join("outb").display(),
        dir.join("in").display(),
        dir.join("binkd.log").display(),
        dir.join("binkd.pid").display()
    );
    let path = dir.join("binkd.conf");
    fs::write(&path, text).unwrap();
    path.display().to_string()
}

/// Runs a binary with the arguments in a directory, and answers the code, the
/// standard output, and the standard error.
fn run(program: &Path, args: &[&str], dir: &Path) -> (i32, String, String) {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("cannot run the binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Returns the path of the C binary, when the environment names it.
fn c_binary() -> Option<PathBuf> {
    std::env::var("BINKD_C").ok().map(PathBuf::from)
}

/// Returns the `BEGIN` line of a log text.
fn begin_line(text: &str) -> Option<String> {
    text.lines()
        .find(|line| line.contains("BEGIN, binkd/"))
        .map(|line| line.to_string())
}

/// Sends a signal to a process, the port of the `kill` utility.
fn kill(signal: &str, pid: u32) -> bool {
    Command::new("kill")
        .arg(signal)
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Waits for the child and answers its code.
fn wait_for(child: &mut Child, timeout: Duration) -> Option<i32> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

#[test]
fn the_begin_line_matches_the_oracle() {
    let Some(c) = c_binary() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("binkd-begin-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let config = write_config(&dir);
    let args = ["-n", config.as_str()];

    let (code, _out, err) = run(&binary(), &args, &dir);
    assert_eq!(code, 1);
    let (c_code, _c_out, c_err) = run(&c, &args, &dir);
    assert_eq!(c_code, code);

    // Both hold the name of the mailer and the arguments of the command line,
    // and the version and the system of the two differ.
    let port = begin_line(&err).expect("the port holds no BEGIN line");
    let oracle = begin_line(&c_err).expect("the C code holds no BEGIN line");
    let tail = format!(" -n {config}");
    assert!(port.ends_with(&tail), "{port}");
    assert!(oracle.ends_with(&tail), "{oracle}");
    assert!(port.contains("binkd/"), "{port}");
    assert!(oracle.contains("binkd/"), "{oracle}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_daemon_starts_and_the_signals_end_it() {
    let dir = std::env::temp_dir().join(format!("binkd-daemon-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let config = write_config(&dir);
    let pid_file = dir.join("binkd.pid");
    let log_file = dir.join("binkd.log");

    let mut child = Command::new(binary())
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("cannot run the binary");

    // The daemon writes the pid file with the pid of its process.
    let start = Instant::now();
    while !pid_file.is_file() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(pid_file.is_file(), "the pid file did not appear");
    assert_eq!(
        fs::read_to_string(&pid_file).unwrap().trim(),
        child.id().to_string()
    );

    // The SIGHUP checks the config files and keeps the daemon.
    assert!(kill("-HUP", child.id()), "cannot send the SIGHUP");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        let log = fs::read_to_string(&log_file).unwrap_or_default();
        if log.contains("got SIGHUP") {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        child.try_wait().unwrap().is_none(),
        "the SIGHUP ended the daemon"
    );

    // The SIGTERM ends the daemon with the code 0.
    assert!(kill("-TERM", child.id()), "cannot send the SIGTERM");
    assert_eq!(wait_for(&mut child, Duration::from_secs(10)), Some(0));

    // The daemon removes the pid file at the end.
    assert!(!pid_file.exists(), "the pid file stays");

    // The log holds the lines of the startup and of the signals.
    let log = fs::read_to_string(&log_file).unwrap();
    assert!(log.contains("BEGIN, binkd/"), "{log}");
    assert!(log.contains("got SIGHUP"), "{log}");
    assert!(log.contains("got signal #15."), "{log}");
    assert!(log.contains("exitfunc()"), "{log}");
    let _ = fs::remove_dir_all(&dir);
}
