//! The binkd binary, the port of `main()` of `binkd.c`.
//!
//! The binary is a thin wrapper: it builds the platform values, installs the
//! signal handlers, maps the outcome of the library to the exit code, and writes
//! the text of the paths that end the program.

use std::sync::Arc;

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
    // The port of the `-D` switch of `main()` of `binkd.c`: the C code
    // daemonizes the process before the managers run, and the parent ends with
    // the code 0. The mark of the environment holds the port of the
    // `already_daemonized` of `unix/daemonize.c`.
    #[cfg(unix)]
    if let Ok(options) = cli::parse(&args) {
        if options.daemon && std::env::var("BINKD_DAEMONIZED").is_err() {
            std::process::exit(daemonize(&args));
        }
    }
    // The port of `service()` of `nt/service.c`: the `-t` command runs before
    // the config and the daemon.
    #[cfg(windows)]
    if let Ok(options) = cli::parse(&args) {
        if let Some(command) = options.service {
            std::process::exit(binkd_win::service::command(
                command,
                options.service_name.as_deref(),
                options.config.as_deref(),
                &args,
            ));
        }
    }
    // The port of `tell_start_ntservice()` of `nt/service.c`: a process with
    // no arguments that the service control manager starts dispatches the
    // service. An interactive process gets `NotService` and runs as usual.
    #[cfg(windows)]
    if args.len() == 1 {
        match binkd_win::service::run(run_service_daemon) {
            Ok(code) => std::process::exit(code),
            Err(binkd_win::service::Error::NotService) => {}
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    let log = Arc::new(Log::new(ERROR, ERROR, "", Vec::new()));
    let code = match start::run(&args, &log) {
        Ok(Outcome::Exit { code, text }) => {
            print!("{text}");
            code
        }
        Ok(Outcome::Daemon(daemon)) => run_daemon(daemon, &log).await,
        Err(error) => start_error(error, &log, &program, &name),
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
#[cfg(unix)]
fn daemonize(args: &[String]) -> i32 {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

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
async fn run_daemon(daemon: Box<daemon::Daemon>, log: &Arc<Log>) -> i32 {
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
    finish_daemon(daemon, &mut signals).await
}

/// Runs the daemon until a signal or a service event ends it.
async fn finish_daemon(mut daemon: Box<daemon::Daemon>, signals: &mut daemon::Signals) -> i32 {
    let code = daemon.serve(signals).await;
    daemon.stop();
    code
}

/// Runs the daemon of the Windows service, the port of `ServiceStart()` of
/// `nt/service.c`.
#[cfg(windows)]
fn run_service_daemon(context: binkd_win::service::ServiceContext) -> i32 {
    let log = Arc::new(Log::new(ERROR, ERROR, "", Vec::new()));
    let panic_log = Arc::clone(&log);
    std::panic::set_hook(Box::new(move |info| {
        panic_log.message(
            ERROR,
            &format!("panic in the service: {info}"),
            &platform::now(),
        );
    }));
    let (sender, receiver) = tokio::sync::mpsc::channel(4);
    context.control.set_handler(move |event| {
        let event = match event {
            binkd_win::service::Event::Stop => daemon::Event::Exit(0),
        };
        // The control handler can run on the thread of the runtime, so it
        // must not block. A full channel holds a stop of the daemon already.
        let _ = sender.try_send(event);
    });
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("cannot start the runtime of the service: {error}");
            return 1;
        }
    };
    let program = context.args.first().cloned().unwrap_or_default();
    let name = basename(&program).to_string();
    runtime.block_on(async {
        match start::run_service(&context.args, &log, &context.display) {
            Ok(Outcome::Daemon(daemon)) => {
                let mut signals = daemon::Signals::service(receiver);
                finish_daemon(daemon, &mut signals).await
            }
            Ok(Outcome::Exit { code, text }) => {
                print!("{text}");
                code
            }
            Err(error) => start_error(error, &log, &program, &name),
        }
    })
}

/// Maps an error of the start to the code and the text of the end, the port of
/// the error paths of `main()` of `binkd.c`.
fn start_error(error: start::Error, log: &Arc<Log>, program: &str, name: &str) -> i32 {
    match error {
        start::Error::Cli(cli::Error::Usage) => {
            print!("{}", cli::usage());
            1
        }
        start::Error::Cli(cli::Error::UnknownSwitch(c)) => {
            eprintln!("{program}: illegal option -- {c}");
            1
        }
        start::Error::Cli(cli::Error::MissingArgument(c)) => {
            eprintln!("{program}: option requires an argument -- {c}");
            1
        }
        start::Error::Cli(cli::Error::UnknownLong(arg)) => {
            log.message(
                FATAL,
                &format!("{name}: {arg}: unknown command line switch"),
                &platform::now(),
            );
            1
        }
        #[cfg(windows)]
        start::Error::Cli(cli::Error::ServiceCommand(value)) => {
            eprintln!("{program}: unknown service command -- {value}");
            1
        }
        start::Error::Config(error) => {
            // The C code of `ConfigError()` ends the line of the level -1.
            log.message(-1, &format!("{error}\n"), &platform::now());
            log.message(FATAL, "error in configuration, aborting", &platform::now());
            1
        }
    }
}

/// Returns the name of the file of a path, the port of
/// `extract_filename()` of `tools.c`.
fn basename(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(i) => &path[i + 1..],
        None => path,
    }
}
