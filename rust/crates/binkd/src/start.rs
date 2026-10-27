//! The startup order of binkd, the port of `main()` of `binkd.c`.

use std::fmt;

use binkd_config::Parser;
use binkd_log::{Log, ERROR, FATAL};

use crate::cli::{self, Options};
use crate::platform;
use crate::version;

/// The outcome of a run, the port of the ends of `main()` of `binkd.c`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The program writes the text to the standard output and ends with the
    /// code, the port of the `printf()` and of the `exit()` of `main()`.
    Exit {
        /// The code of the end, the port of the argument of `exit()`.
        code: i32,
        /// The text of the standard output, or empty.
        text: String,
    },
    /// The command line runs the daemon, the port of the rest of `main()`.
    ///
    /// The port of this slice holds no daemon: the startup order, the
    /// signals, the pid file, the config reload, and the external program
    /// start arrive with the next slices.
    Daemon(Box<Options>),
}

/// The error of a run, the port of the `Log (0, ...)` messages of `main()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The command line, the port of `parseargs()`.
    Cli(cli::Error),
    /// The config, the port of `readcfg()`.
    Config(binkd_config::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Cli(e) => write!(f, "{e}"),
            Error::Config(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<cli::Error> for Error {
    fn from(e: cli::Error) -> Error {
        Error::Cli(e)
    }
}

impl From<binkd_config::Error> for Error {
    fn from(e: binkd_config::Error) -> Error {
        Error::Config(e)
    }
}

/// Runs the command line, the port of `main()` of `binkd.c` up to the point
/// the port reaches.
///
/// The C code parses the command line, reads the config, and runs the daemon.
/// The port of this slice holds the command line and the paths that end the
/// program: `-d` dumps the config, `-n` checks the config, and `-v` states
/// the version. The caller supplies the log, as the C code of the port of
/// `binkd-log` keeps the log of the process.
pub fn run(args: &[String], log: &Log) -> Result<Outcome, Error> {
    let options = cli::parse(args)?;

    // The C code of `parseargs()` writes this message before the config.
    if options.extra {
        log.message(ERROR, "Extra parameters ignored", &platform::now());
    }

    if let Some(path) = options.config.clone() {
        let mut parser = Parser::new();
        parser.set_quiet(options.quiet);
        parser.set_verbose(options.verbose as i32);
        // The C code resolves the paths of the config against the current
        // directory, the port of `readcfg0()`.
        if let Ok(dir) = std::env::current_dir() {
            parser.set_base_dir(&dir);
        }
        let config = parser.parse(&path)?;
        // The C code dumps the config before it opens the log.
        if options.dump > 0 {
            return Ok(Outcome::Exit {
                code: 0,
                text: binkd_config::dump(&config),
            });
        }
        log.init(
            config.loglevel,
            config.conlog,
            &config.logpath,
            config.nolog.clone(),
        );
        if options.no {
            log.message(FATAL, "Exit on option '-n'", &platform::now());
            return Ok(Outcome::Exit {
                code: 1,
                text: String::new(),
            });
        }
        return Ok(Outcome::Daemon(Box::new(options)));
    }

    if options.verbose > 0 {
        let now = platform::now();
        let date = platform::date(&now);
        let system = platform::system();
        let text = if options.verbose > 1 {
            version::verbose(&date, system)
        } else {
            version::line(&date, system)
        };
        return Ok(Outcome::Exit { code: 0, text });
    }

    if args.len() > 1 {
        log.message(
            FATAL,
            "invalid command line: config name must be specified",
            &platform::now(),
        );
        return Ok(Outcome::Exit {
            code: 1,
            text: String::new(),
        });
    }

    Err(cli::Error::Usage.into())
}
