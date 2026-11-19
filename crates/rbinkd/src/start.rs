//! The startup order of rbinkd, the port of `main()` of `binkd.c`.

use std::fmt;
use std::path::Path;

use rbinkd_config::Parser;
use rbinkd_log::{Log, FATAL};

use crate::cli;
use crate::daemon::Daemon;
use crate::platform;
use crate::version;

/// The outcome of a run, the port of the ends of `main()` of `binkd.c`.
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
    Daemon(Box<Daemon>),
}

/// The error of a run, the port of the `Log (0, ...)` messages of `main()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The command line, the port of `parseargs()`.
    Cli(cli::Error),
    /// The config, the port of `readcfg()`.
    Config(rbinkd_config::Error),
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

impl From<rbinkd_config::Error> for Error {
    fn from(e: rbinkd_config::Error) -> Error {
        Error::Config(e)
    }
}

/// Runs the command line, the port of `main()` of `binkd.c` up to the point
/// the port reaches.
///
/// The C code parses the command line, reads the config, and runs the daemon. The
/// port of this slice holds the command line, the paths that end the program, and
/// the startup of the daemon: `-d` dumps the config, `-n` checks the config,
/// `-v` states the version, and a run without them starts the daemon. The caller
/// supplies the log, as the C code of the port of `rbinkd-log` keeps the log of
/// the process.
pub fn run(args: &[String], log: &std::sync::Arc<Log>) -> Result<Outcome, Error> {
    run_with(args, log, None)
}

/// Runs the daemon of the Windows service, the port of `ServiceStart()` of
/// `nt/service.c`.
///
/// The display name of the service is the name that the C code reads from the
/// service control manager before `binkd_main()`, so the `BEGIN service` line
/// holds it.
#[cfg(windows)]
pub fn run_service(
    args: &[String],
    log: &std::sync::Arc<Log>,
    display: &str,
) -> Result<Outcome, Error> {
    run_with(args, log, Some(display))
}

/// Runs the startup order with the optional display name of the service.
fn run_with(
    args: &[String],
    log: &std::sync::Arc<Log>,
    service: Option<&str>,
) -> Result<Outcome, Error> {
    let options = cli::parse(args)?;
    #[cfg(windows)]
    let options = {
        let mut options = options;
        if let Some(display) = service {
            options.service_display = Some(display.to_string());
        }
        options
    };
    #[cfg(not(windows))]
    let _ = service;

    // The C code of `parseargs()` writes this message before the config.
    if options.extra {
        log.message(
            rbinkd_log::ERROR,
            "Extra parameters ignored",
            &platform::now(),
        );
    }

    let config_path = options
        .config
        .clone()
        .or_else(|| (options.verbose == 0).then(default_config).flatten());
    if let Some(path) = config_path {
        let mut features = rbinkd_config::Features::platform_default();
        // The C code of `perl_init()` accepts the `perl-*` keywords only in a
        // build of `--with-perl`, the port of the `WITH_PERL` of `readcfg.c`.
        features.perl = cfg!(feature = "perl");
        let mut parser = Parser::new();
        parser.set_features(features);
        parser.set_partition(platform::partition);
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
                text: rbinkd_config::dump_with(&config, &features),
            });
        }
        let log_path = effective_log_path(
            &config.logpath,
            std::env::var("RBINKD_LOG").ok(),
            std::env::var("BINKD_LOG").ok(),
        );
        log.init(
            config.loglevel,
            config.conlog,
            &log_path,
            config.nolog.clone(),
        );
        // The port of the `openlog()` call of `vLog()`: the sink is ready
        // before the `BEGIN` line of `Daemon::new()`.
        #[cfg(unix)]
        crate::syslog::configure(log, config.syslog_facility);
        #[cfg(feature = "perl")]
        let perl = (config.perl_script.clone(), config.perl_strict);
        // The port of `perl_setup()`: the hooks read the whole config, so the
        // binary keeps a copy before the daemon owns it.
        #[cfg(feature = "perl")]
        let perl_config = (!config.perl_script.is_empty()).then(|| config.clone());
        // The port of `print_args()` and of the `Log (4, "BEGIN, ...")`.
        let mut daemon = Daemon::new(config, &options, log.clone(), args).with_features(features);
        // The port of the `perl_init()` block of `main()` of `binkd.c`: the
        // hook runs after the `BEGIN` line of the C code.
        #[cfg(feature = "perl")]
        if !perl.0.is_empty() {
            let config = perl_config.expect("the config of the script");
            match crate::perl::load(log, &perl.0, config) {
                Some(runtime) => {
                    if let Ok(hooks) = runtime.master.lock() {
                        hooks.on_start();
                        hooks.config_loaded();
                    }
                    log.set_hook(Some(runtime.log_hook.clone()));
                    daemon.set_perl(runtime);
                }
                None => {
                    if perl.1 {
                        log.message(
                            FATAL,
                            &format!("error parsing Perl script {}", perl.0),
                            &platform::now(),
                        );
                        return Ok(Outcome::Exit {
                            code: 1,
                            text: String::new(),
                        });
                    }
                    log.message(
                        rbinkd_log::ERROR,
                        &format!("cannot load Perl script {}, hooks disabled", perl.0),
                        &platform::now(),
                    );
                }
            }
        }
        // The C code ends here on the `-n` switch and writes no pid file.
        if options.no {
            log.message(FATAL, "Exit on option '-n'", &platform::now());
            return Ok(Outcome::Exit {
                code: 1,
                text: String::new(),
            });
        }
        // The port of the pid file block of `main()`.
        daemon.write_pid();
        return Ok(Outcome::Daemon(Box::new(daemon)));
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

/// Returns the default configuration path for the current directory.
///
/// The new name comes first. The legacy name stays valid for an existing
/// Binkd installation.
fn default_config() -> Option<String> {
    default_config_in(Path::new("."))
}

/// Returns the first default configuration in a directory.
fn default_config_in(dir: &Path) -> Option<String> {
    let names: &[&str] = if cfg!(windows) {
        &["rbinkd.cfg", "binkd.cfg", "rbinkd.conf", "binkd.conf"]
    } else {
        &["rbinkd.conf", "binkd.conf", "rbinkd.cfg", "binkd.cfg"]
    };
    names
        .iter()
        .find(|name| dir.join(name).is_file())
        .map(|name| (*name).to_string())
}

/// Selects the log path.
///
/// The `log` directive wins. An empty directive uses the new environment
/// variable first and the legacy one second.
fn effective_log_path(
    config_path: &str,
    rbinkd_env: Option<String>,
    binkd_env: Option<String>,
) -> String {
    if !config_path.is_empty() {
        return config_path.to_string();
    }
    rbinkd_env
        .filter(|value| !value.is_empty())
        .or_else(|| binkd_env.filter(|value| !value.is_empty()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{default_config_in, effective_log_path};
    use std::fs;

    #[test]
    fn the_config_log_path_wins() {
        assert_eq!(
            effective_log_path(
                "./binkd.log",
                Some("new.log".into()),
                Some("old.log".into())
            ),
            "./binkd.log"
        );
    }

    #[test]
    fn the_new_environment_name_precedes_the_legacy_name() {
        assert_eq!(
            effective_log_path("", Some("new.log".into()), Some("old.log".into())),
            "new.log"
        );
    }

    #[test]
    fn the_legacy_environment_name_is_supported() {
        assert_eq!(
            effective_log_path("", None, Some("old.log".into())),
            "old.log"
        );
    }

    #[test]
    fn an_empty_environment_value_is_ignored() {
        assert_eq!(
            effective_log_path("", Some(String::new()), Some("old.log".into())),
            "old.log"
        );
    }

    #[test]
    fn the_default_config_supports_both_names() {
        let dir =
            std::env::temp_dir().join(format!("rbinkd-default-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("binkd.conf"), "").unwrap();
        assert_eq!(default_config_in(&dir), Some("binkd.conf".to_string()));
        fs::write(dir.join("rbinkd.conf"), "").unwrap();
        assert_eq!(default_config_in(&dir), Some("rbinkd.conf".to_string()));
        let _ = fs::remove_dir_all(&dir);
    }
}
