//! The binkd binary, the port of `main()` of `binkd.c`.
//!
//! The binary is a thin wrapper: it builds the platform values, installs the
//! signal handlers, maps the outcome of the library to the exit code, and writes
//! the text of the paths that end the program.

use std::sync::Arc;

use std::process::Stdio;

use binkd::cli;
use binkd::daemon;
use binkd::platform;
use binkd::start::{self, Outcome};
use binkd_log::{Log, ERROR, FATAL};

/// Runs the program, the port of `main()` of `binkd.c`.
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let program = args.first().cloned().unwrap_or_default();
    let name = basename(&program).to_string();
    // The C code starts the log with the level 1 and the console level 1.
    // The port of the `-D` switch of `main()` of `binkd.c`: the C code
    // daemonizes the process before the managers run, and the parent ends with
    // the code 0. The mark of the environment holds the port of the
    // `already_daemonized` of `unix/daemonize.c`.
    if let Ok(options) = cli::parse(&args) {
        if options.daemon && std::env::var("BINKD_DAEMONIZED").is_err() {
            std::process::exit(daemonize(&args));
        }
    }
    let log = Arc::new(Log::new(ERROR, ERROR, "", Vec::new()));
    let code = match start::run(&args, &log) {
        Ok(Outcome::Exit { code, text }) => {
            print!("{text}");
            code
        }
        Ok(Outcome::Daemon(daemon)) => run_daemon(daemon, &log).await,
        Err(start::Error::Cli(cli::Error::Usage)) => {
            print!("{}", cli::USAGE);
            1
        }
        Err(start::Error::Cli(cli::Error::UnknownSwitch(c))) => {
            eprintln!("{program}: illegal option -- {c}");
            1
        }
        Err(start::Error::Cli(cli::Error::MissingArgument(c))) => {
            eprintln!("{program}: option requires an argument -- {c}");
            1
        }
        Err(start::Error::Cli(cli::Error::UnknownLong(arg))) => {
            log.message(
                FATAL,
                &format!("{name}: {arg}: unknown command line switch"),
                &platform::now(),
            );
            1
        }
        Err(start::Error::Config(error)) => {
            // The C code of `ConfigError()` ends the line of the level -1.
            log.message(-1, &format!("{error}\n"), &platform::now());
            log.message(FATAL, "error in configuration, aborting", &platform::now());
            1
        }
    };
    std::process::exit(code);
}

/// Detaches the process, the port of `binkd_daemonize()` of
/// `unix/daemonize.c`, and answers the code of the parent.
///
/// The C code forks, the parent ends with the code 0, the child calls
/// `setsid()`, and it moves the standard input, the standard output, and the
/// standard error to `/dev/null`. `std` states no `fork()`, so the port runs the
/// same program again in a new process group with the standard files of
/// `/dev/null`, the port of the same steps. The caller holds the code 0 for the
/// parent and the mark of the environment for the child.
fn daemonize(args: &[String]) -> i32 {
    use std::os::unix::process::CommandExt;
    let Ok(program) = std::env::current_exe() else {
        return 1;
    };
    let child = std::process::Command::new(program)
        .args(&args[1..])
        .env("BINKD_DAEMONIZED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    match child {
        // The parent ends at once, the port of the `exit(0)` of the C code.
        Ok(_) => 0,
        Err(error) => {
            eprintln!("cannot daemonize: {error}");
            1
        }
    }
}

/// Runs the daemon, the port of the rest of `main()` of `binkd.c`.
async fn run_daemon(mut daemon: Box<daemon::Daemon>, log: &Arc<Log>) -> i32 {
    // The port of `set_break_handlers()` of `breaksig.c`.
    let mut signals = match daemon::Signals::install() {
        Ok(signals) => signals,
        Err(error) => {
            log.message(
                ERROR,
                &format!("cannot install the signal handlers: {error}"),
                &platform::now(),
            );
            return 1;
        }
    };
    // The port of the loop of `servmgr()`, of `clientmgr()`, and of `exitfunc()`.
    let code = daemon.serve(&mut signals).await;
    daemon.stop();
    code
}

/// Returns the name of the file of a path, the port of
/// `extract_filename()` of `tools.c`.
fn basename(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(i) => &path[i + 1..],
        None => path,
    }
}
