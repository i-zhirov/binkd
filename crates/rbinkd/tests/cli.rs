//! The command line of the binary against the C oracle.
//!
//! The test runs the binary of the port and checks the text, the error text,
//! and the exit code of every path of the command line. When the `BINKD_C`
//! environment variable names the C binary, the test also runs it and checks
//! that the two answer the same.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns the path of the binary of the port.
fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rbinkd"))
}

/// Returns the directory of the config corpus.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/config")
}

/// Returns the path of the C binary, when the environment names it.
fn c_binary() -> Option<PathBuf> {
    std::env::var("BINKD_C").ok().map(PathBuf::from)
}

/// Normalizes the line endings of the output of the C and Rust binaries.
fn normalize(text: String) -> String {
    text.replace("\r\n", "\n")
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
        normalize(String::from_utf8_lossy(&out.stdout).into_owned()),
        normalize(String::from_utf8_lossy(&out.stderr).into_owned()),
    )
}

/// Runs the binary of the port.
fn run_port(args: &[&str], dir: &Path) -> (i32, String, String) {
    run(&binary(), args, dir)
}

/// Runs the binary of the port with extra environment values.
#[cfg(unix)]
fn run_port_env(args: &[&str], dir: &Path, env: &[(&str, &str)]) -> (i32, String, String) {
    let mut command = Command::new(binary());
    command.args(args).current_dir(dir);
    for (key, value) in env {
        command.env(key, value);
    }
    let out = command.output().expect("cannot run the binary");
    (
        out.status.code().unwrap_or(-1),
        normalize(String::from_utf8_lossy(&out.stdout).into_owned()),
        normalize(String::from_utf8_lossy(&out.stderr).into_owned()),
    )
}

/// Returns the name, the version, the date, and the system of a version line,
/// the shape of the line of the `-v` switch.
fn version_shape(line: &str) -> Option<(String, String, String)> {
    let rest = line
        .strip_prefix("rbinkd ")
        .or_else(|| line.strip_prefix("Binkd "))?;
    let open = rest.find(" (")?;
    let close = rest.rfind(')')?;
    if close < open {
        return None;
    }
    let version = &rest[..open];
    let inside = &rest[open + 2..close];
    let slash = inside.rfind('/')?;
    let date = &inside[..slash];
    let system = &inside[slash + 1..];
    if version.is_empty() || date.is_empty() || system.is_empty() {
        return None;
    }
    Some((version.to_string(), date.to_string(), system.to_string()))
}

#[test]
fn usage_with_h() {
    let (code, out, _err) = run_port(&["-h"], &corpus_dir());
    assert_eq!(code, 1);
    assert_eq!(out, rbinkd::cli::usage());
    if let Some(c) = c_binary() {
        let (c_code, _c_out, _err) = run(&c, &["-h"], &corpus_dir());
        // The Rust help names rbinkd, so the text intentionally differs from
        // the C help. The switch and the exit code stay compatible.
        assert_eq!(c_code, code);
    }
}

#[test]
fn version_with_v() {
    let (code, out, _err) = run_port(&["-v"], &corpus_dir());
    assert_eq!(code, 0);
    let (version, _date, _system) = version_shape(&out).expect("the shape of the line");
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
    if let Some(c) = c_binary() {
        let (c_code, c_out, _err) = run(&c, &["-v"], &corpus_dir());
        assert_eq!(c_code, code);
        assert!(version_shape(&c_out).is_some());
    }
}

#[cfg(unix)]
#[test]
fn the_version_date_is_local() {
    if time::OffsetDateTime::now_local().is_err() {
        println!("the system holds no local offset; skipping the local date check");
        return;
    }
    let dir = corpus_dir();
    // The two zones are on opposite sides of the line: the west zone is
    // twelve hours behind UTC, and the east zone is fourteen hours ahead.
    let (west_code, west, _) = run_port_env(&["-v"], &dir, &[("TZ", "Etc/GMT+12")]);
    let (east_code, east, _) = run_port_env(&["-v"], &dir, &[("TZ", "Pacific/Kiritimati")]);
    assert_eq!(west_code, 0);
    assert_eq!(east_code, 0);
    let (_, west_date, _) = version_shape(&west).expect("the shape of the west line");
    let (_, east_date, _) = version_shape(&east).expect("the shape of the east line");
    assert_ne!(west_date, east_date, "the version date is not local");
}

#[test]
fn version_with_vv() {
    let (code, out, _err) = run_port(&["-vv"], &corpus_dir());
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(version_shape(lines[0]).is_some());
    assert!(lines[1].starts_with("Compilation flags: "));
    assert!(lines[1].ends_with('.'));
    assert!(lines[2].starts_with("Facilities: "));
    if let Some(c) = c_binary() {
        let (c_code, c_out, _err) = run(&c, &["-vv"], &corpus_dir());
        assert_eq!(c_code, code);
        assert_eq!(c_out.lines().count(), out.lines().count());
    }
}

#[cfg(all(unix, not(feature = "perl")))]
#[test]
fn dump_with_d() {
    let dir = corpus_dir();
    let (code, out, err) = run_port(&["-d", "minimal.conf"], &dir);
    assert_eq!(code, 0);
    assert_eq!(
        out,
        normalize(std::fs::read_to_string(dir.join("minimal.expect")).unwrap())
    );
    assert_eq!(err, "");
    if let Some(c) = c_binary() {
        let (c_code, c_out, _err) = run(&c, &["-d", "minimal.conf"], &dir);
        assert_eq!(c_code, code);
        assert_eq!(c_out, out);
    }
}

#[cfg(all(unix, feature = "perl"))]
#[test]
fn dump_with_d_perl() {
    let dir = corpus_dir();
    let (code, out, err) = run_port(&["-d", "minimal.conf"], &dir);
    assert_eq!(code, 0);
    assert_eq!(err, "");
    for name in ["perl-hooks", "perl-dll", "perl-strict", "perl-var"] {
        assert!(
            out.lines().any(|line| line.starts_with(name)),
            "{name} is missing:
{out}"
        );
    }
    if let Ok(binary) = std::env::var("BINKD_C_PERL") {
        let (c_code, c_out, _err) = run(Path::new(&binary), &["-d", "minimal.conf"], &dir);
        assert_eq!(c_code, code);
        assert_eq!(c_out, out);
    }
}

#[cfg(windows)]
#[test]
fn dump_with_d_windows() {
    let dir = corpus_dir();
    let (code, out, err) = run_port(&["-d", "minimal.conf"], &dir);
    assert_eq!(code, 0);
    assert_eq!(err, "");
    for name in ["proxy", "socks", "limit-rate", "zlevel"] {
        assert!(
            out.lines().any(|line| line.starts_with(name)),
            "{name} is missing:
{out}"
        );
    }
    for name in ["aso", "syslog", "zlib-dll", "bzlib2-dll"] {
        assert!(
            !out.lines().any(|line| line.starts_with(name)),
            "{name} is present:
{out}"
        );
    }
}

#[test]
fn check_with_n() {
    let (code, _out, err) = run_port(&["-n", "minimal.conf"], &corpus_dir());
    assert_eq!(code, 1);
    assert!(err.ends_with("] Exit on option '-n'\n"));
    if let Some(c) = c_binary() {
        let (c_code, _out, c_err) = run(&c, &["-n", "minimal.conf"], &corpus_dir());
        assert_eq!(c_code, code);
        assert_eq!(
            c_err.ends_with("] Exit on option '-n'\n"),
            err.ends_with("] Exit on option '-n'\n")
        );
    }
}

#[test]
fn usage_without_arguments() {
    let (code, out, _err) = run_port(&[], &corpus_dir());
    assert_eq!(code, 1);
    assert_eq!(out, rbinkd::cli::usage());
    if let Some(c) = c_binary() {
        let (c_code, _c_out, _err) = run(&c, &[], &corpus_dir());
        // The Rust help names rbinkd, so the text intentionally differs from
        // the C help. The exit code stays compatible.
        assert_eq!(c_code, code);
    }
}

#[test]
fn invalid_command_line() {
    let (code, _out, err) = run_port(&["-q"], &corpus_dir());
    assert_eq!(code, 1);
    assert!(err.ends_with("] invalid command line: config name must be specified\n"));
}

#[test]
fn unknown_switch() {
    let (code, _out, err) = run_port(&["-z"], &corpus_dir());
    assert_eq!(code, 1);
    assert!(err.ends_with(": illegal option -- z\n"));
}

#[test]
fn unknown_long_switch() {
    let (code, _out, err) = run_port(&["--other"], &corpus_dir());
    assert_eq!(code, 1);
    let name = binary().file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        err.ends_with(&format!("] {name}: --other: unknown command line switch\n")),
        "{err}"
    );
}

#[test]
fn extra_parameters() {
    let (code, _out, err) = run_port(&["-d", "minimal.conf", "extra"], &corpus_dir());
    assert_eq!(code, 0);
    assert!(err.ends_with("] Extra parameters ignored\n"));
}

#[test]
fn missing_config() {
    let (code, _out, err) = run_port(&["-d", "no-such.conf"], &corpus_dir());
    assert_eq!(code, 1);
    assert!(err.ends_with("] error in configuration, aborting\n"));
    if let Some(c) = c_binary() {
        let (c_code, _out, c_err) = run(&c, &["-d", "no-such.conf"], &corpus_dir());
        assert_eq!(c_code, code);
        assert_eq!(
            c_err.ends_with("] error in configuration, aborting\n"),
            err.ends_with("] error in configuration, aborting\n")
        );
    }
}
