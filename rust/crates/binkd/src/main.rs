//! The binkd binary, the port of `main()` of `binkd.c`.
//!
//! The binary is a thin wrapper: it builds the platform values, maps the
//! outcome of the library to the exit code, and writes the text of the paths
//! that end the program.

use binkd::cli;
use binkd::platform;
use binkd::start::{self, Outcome};
use binkd_log::{Log, ERROR, FATAL};

/// Runs the program, the port of `main()` of `binkd.c`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let program = args.first().cloned().unwrap_or_default();
    let name = basename(&program).to_string();
    // The C code starts the log with the level 1 and the console level 1.
    let log = Log::new(ERROR, ERROR, "", Vec::new());
    let code = match start::run(&args, &log) {
        Ok(Outcome::Exit { code, text }) => {
            print!("{text}");
            code
        }
        Ok(Outcome::Daemon(_options)) => {
            // The port of this slice holds no daemon yet.
            eprintln!("{name}: the daemon is not built yet");
            1
        }
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

/// Returns the name of the file of a path, the port of
/// `extract_filename()` of `tools.c`.
fn basename(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(i) => &path[i + 1..],
        None => path,
    }
}
