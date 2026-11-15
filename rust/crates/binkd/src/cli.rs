//! The command line of binkd, the port of `parseargs()` and of `usage()` of
//! `binkd.c`.

use std::fmt;

/// The text of the usage, the port of `usage()` of `binkd.c`.
///
/// The C code states the name `binkd` in the text, so the text holds no name
/// of the program of the run.
pub const USAGE: &str = "\
usage: binkd [-CcDipqrsvmh] [-P node] config
  -D       run as daemon
  -C       reload on config change
  -c       run client only
  -i       run server on stdin/stdout pipe (by inetd or other)
  -f node  run server protected session with this node
  -a ip    assume remote address when running with '-i' switch
  -P node  poll a node
  -p       run client only, poll, quit
  -q       be quiet
  -r       disable crypt traffic
  -s       run server only
  -v       be verbose / dump version and quit
  -vv      dump version with compilation flags and quit
  -d       dump parsed config and exit
  -m       disable CRAM-MD5 authorization
  -n       don't run binkd-client and binkd-server (check config, make polls)
  -h       print this help

Copyright (c) 1996-2009 Dima Maloff and others.

This program is free software; you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation. See COPYING.

Report bugs to 2:463/68 or binkd-bugs@happy.kiev.ua.
";

/// Returns the usage text of the run, the port of `usage()` of `binkd.c`.
///
/// The Windows build adds the switches of the service mode. The port holds no
/// tray switch.
pub fn usage() -> String {
    #[cfg(not(windows))]
    {
        USAGE.to_string()
    }
    #[cfg(windows)]
    {
        let mut text = USAGE.to_string();
        text.push_str("  -t cmd   (start|stop|restart|status|install|uninstall) service\n");
        text.push_str("  -S name  set the Windows service name\n");
        text
    }
}

/// Returns the arguments of the command line, the port of `print_args()` of
/// `tools.c`.
///
/// The C code puts a space before every argument and quotes an argument that is
/// empty or that holds a character at most `' '`.
pub fn print_args(args: &[String]) -> String {
    let mut text = String::new();
    for arg in args {
        let quote = arg.is_empty() || arg.bytes().any(|c| c <= b' ');
        text.push(' ');
        if quote {
            text.push('"');
        }
        text.push_str(arg);
        if quote {
            text.push('"');
        }
    }
    text
}

/// The switches of the command line, the port of the flags of `parseargs()`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// The `-C` switch: reload on config change.
    pub check_on_change: bool,
    /// The `-c` switch: run client only.
    pub client: bool,
    /// The `-i` switch: run server on the standard input and output.
    pub inetd: bool,
    /// The `-a` switch: assume the remote address.
    pub remote_addr: Option<String>,
    /// The `-f` switch: run a protected session with this node.
    pub remote_node: Option<String>,
    /// The `-P` switches: poll these nodes.
    pub polls: Vec<String>,
    /// The `-p` switch: run client only, poll, quit.
    pub poll: bool,
    /// The `-q` switch: be quiet.
    pub quiet: bool,
    /// The `-s` switch: run server only.
    pub server: bool,
    /// The `-m` switch: disable CRAM-MD5 authorization.
    pub no_md5: bool,
    /// The `-r` switch: disable crypt traffic.
    pub no_crypt: bool,
    /// The `-v` switches, the count of the switches.
    pub verbose: u32,
    /// The `-d` switches, the count of the switches.
    pub dump: u32,
    /// The `-D` switch: run as daemon.
    pub daemon: bool,
    /// The `-n` switch: run no client and no server.
    pub no: bool,
    /// The `-t` switch of Windows: the service command.
    #[cfg(windows)]
    pub service: Option<binkd_win::command::Command>,
    /// The `-S` switch of Windows: the display name of the service.
    #[cfg(windows)]
    pub service_name: Option<String>,
    /// The display name of the service that the service control manager
    /// started, the port of `service_name` of `nt/service.c`.
    #[cfg(windows)]
    pub service_display: Option<String>,
    /// The first argument that is no switch, the config file.
    pub config: Option<String>,
    /// The arguments after the config file, the port of the
    /// `Log (1, "Extra parameters ignored")` of `parseargs()`.
    pub extra: bool,
}

impl Options {
    /// Returns the roles of the run, the port of the check of `main()`: a run
    /// without `-c` and without `-s` runs the client and the server.
    pub fn roles(&self) -> (bool, bool) {
        if !self.client && !self.server {
            (true, true)
        } else {
            (self.client, self.server)
        }
    }
}

/// The error of the command line, the port of the ends of `parseargs()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The `-h` switch or `--help`, the port of `usage()`.
    Usage,
    /// An unknown switch, the port of the message of the `getopt()` of the
    /// C code.
    UnknownSwitch(char),
    /// A switch without its argument, the port of the message of the
    /// `getopt()` of the C code.
    MissingArgument(char),
    /// A GNU style option that is no `--help`, the port of the
    /// `Log (0, ...)` of the `'-'` case of `parseargs()`.
    UnknownLong(String),
    /// The `-t` switch of Windows holds an unknown service command.
    #[cfg(windows)]
    ServiceCommand(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage => write!(f, "the usage"),
            Error::UnknownSwitch(c) => write!(f, "unknown switch: {c}"),
            Error::MissingArgument(c) => write!(f, "the switch {c} misses its argument"),
            Error::UnknownLong(arg) => write!(f, "unknown command line switch: {arg}"),
            #[cfg(windows)]
            Error::ServiceCommand(value) => write!(f, "unknown service command: {value}"),
        }
    }
}

impl std::error::Error for Error {}

/// Parses the command line, the port of `parseargs()` of `binkd.c`.
///
/// The C code calls the `getopt()` of the system. The port reads the arguments
/// itself: the switches bundle as the C code bundles them, the argument of a
/// switch follows the switch or stands in the next argument, and `--` ends the
/// switches. The port of the C code reads no switch of the Windows layer.
pub fn parse(args: &[String]) -> Result<Options, Error> {
    let mut options = Options::default();
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--" {
            index += 1;
            break;
        }
        if arg == "-" || !arg.starts_with('-') {
            break;
        }
        if arg.starts_with("--") {
            // The GNU style options of the `'-'` case of `parseargs()`.
            if arg == "--help" {
                return Err(Error::Usage);
            }
            return Err(Error::UnknownLong(arg.clone()));
        }
        let switches: Vec<char> = arg.chars().skip(1).collect();
        let mut i = 0;
        while i < switches.len() {
            let c = switches[i];
            i += 1;
            match c {
                'C' => options.check_on_change = true,
                'c' => options.client = true,
                'i' => options.inetd = true,
                'a' => {
                    options.remote_addr = Some(argument(&switches, &mut i, args, &mut index, c)?)
                }
                'f' => {
                    options.remote_node = Some(argument(&switches, &mut i, args, &mut index, c)?)
                }
                'P' => options
                    .polls
                    .push(argument(&switches, &mut i, args, &mut index, c)?),
                'p' => {
                    options.poll = true;
                    options.client = true;
                }
                'q' => options.quiet = true,
                's' => options.server = true,
                // The C code falls through from `-m` to `-r`.
                'm' => {
                    options.no_md5 = true;
                    options.no_crypt = true;
                }
                'r' => options.no_crypt = true,
                'v' => options.verbose += 1,
                'd' => options.dump += 1,
                'D' => options.daemon = true,
                'n' => options.no = true,
                #[cfg(windows)]
                't' => {
                    let value = argument(&switches, &mut i, args, &mut index, c)?;
                    let command = binkd_win::command::Command::parse(&value)
                        .ok_or(Error::ServiceCommand(value))?;
                    options.service = Some(command);
                }
                #[cfg(windows)]
                'S' => {
                    options.service_name = Some(argument(&switches, &mut i, args, &mut index, c)?);
                }
                'h' => return Err(Error::Usage),
                // The C code falls through from `default` to the `-n` case,
                // so a `-?` of the system sets the `no_flag`.
                '?' => options.no = true,
                _ => return Err(Error::UnknownSwitch(c)),
            }
        }
        index += 1;
    }
    // The C code reads the first argument that is no switch as the config
    // file, the port of `cfgfile = argv[optind++]`.
    options.config = args.get(index).cloned();
    if options.config.is_some() {
        index += 1;
    }
    options.extra = index < args.len();
    Ok(options)
}

/// Returns the argument of a switch, the port of the `optarg` of `getopt()`.
///
/// The argument follows the switch in the same argument, or it stands in the
/// next argument. A switch without its argument answers an error.
fn argument(
    switches: &[char],
    i: &mut usize,
    args: &[String],
    index: &mut usize,
    c: char,
) -> Result<String, Error> {
    if *i < switches.len() {
        let value: String = switches[*i..].iter().collect();
        *i = switches.len();
        return Ok(value);
    }
    *index += 1;
    match args.get(*index) {
        Some(value) => Ok(value.clone()),
        None => Err(Error::MissingArgument(c)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn every_switch() {
        let options = parse(&args(&[
            "binkd",
            "-Cci",
            "-a",
            "127.0.0.1",
            "-f",
            "2:5047/13",
            "-P2:5047/13",
            "-P",
            "2:5047/14",
            "-pqsmrvvdnD",
            "binkd.conf",
        ]))
        .unwrap();
        assert!(options.check_on_change);
        assert!(options.client);
        assert!(options.inetd);
        assert_eq!(options.remote_addr.as_deref(), Some("127.0.0.1"));
        assert_eq!(options.remote_node.as_deref(), Some("2:5047/13"));
        assert_eq!(options.polls, vec!["2:5047/13", "2:5047/14"]);
        assert!(options.poll);
        assert!(options.quiet);
        assert!(options.server);
        assert!(options.no_md5);
        assert!(options.no_crypt);
        assert_eq!(options.verbose, 2);
        assert_eq!(options.dump, 1);
        assert!(options.daemon);
        assert!(options.no);
        assert_eq!(options.config.as_deref(), Some("binkd.conf"));
        assert!(!options.extra);
    }

    #[test]
    fn the_config_is_the_first_argument_that_is_no_switch() {
        let options = parse(&args(&["binkd", "binkd.conf", "extra"])).unwrap();
        assert_eq!(options.config.as_deref(), Some("binkd.conf"));
        assert!(options.extra);
        let options = parse(&args(&["binkd", "-v", "binkd.conf"])).unwrap();
        assert_eq!(options.verbose, 1);
        assert_eq!(options.config.as_deref(), Some("binkd.conf"));
    }

    #[test]
    fn the_double_dash_ends_the_switches() {
        let options = parse(&args(&["binkd", "--", "-v"])).unwrap();
        assert_eq!(options.verbose, 0);
        assert_eq!(options.config.as_deref(), Some("-v"));
    }

    #[test]
    fn the_roles_default_to_both() {
        let options = parse(&args(&["binkd"])).unwrap();
        assert_eq!(options.roles(), (true, true));
        let options = parse(&args(&["binkd", "-c"])).unwrap();
        assert_eq!(options.roles(), (true, false));
        let options = parse(&args(&["binkd", "-s"])).unwrap();
        assert_eq!(options.roles(), (false, true));
        let options = parse(&args(&["binkd", "-p"])).unwrap();
        assert_eq!(options.roles(), (true, false));
        assert!(options.poll);
    }

    #[test]
    fn the_help_switch_asks_for_the_usage() {
        assert_eq!(parse(&args(&["binkd", "-h"])), Err(Error::Usage));
        assert_eq!(parse(&args(&["binkd", "--help"])), Err(Error::Usage));
    }

    #[test]
    fn an_unknown_switch_holds_its_name() {
        assert_eq!(
            parse(&args(&["binkd", "-z"])),
            Err(Error::UnknownSwitch('z'))
        );
        assert_eq!(
            parse(&args(&["binkd", "-cz"])),
            Err(Error::UnknownSwitch('z'))
        );
        assert_eq!(
            parse(&args(&["binkd", "--other"])),
            Err(Error::UnknownLong("--other".to_string()))
        );
    }

    #[test]
    fn a_switch_without_its_argument_holds_its_name() {
        assert_eq!(
            parse(&args(&["binkd", "-P"])),
            Err(Error::MissingArgument('P'))
        );
        assert_eq!(
            parse(&args(&["binkd", "-a"])),
            Err(Error::MissingArgument('a'))
        );
    }

    #[test]
    fn the_question_mark_switch_sets_the_no_flag() {
        // The port of the fallthrough of the `default` case to `case 'n'`.
        let options = parse(&args(&["binkd", "-?"])).unwrap();
        assert!(options.no);
    }

    #[cfg(windows)]
    #[test]
    fn the_service_switches_parse() {
        let options = parse(&args(&[
            "binkd",
            "-t",
            "install",
            "-S",
            "Binkd mailer",
            "binkd.conf",
        ]))
        .unwrap();
        assert_eq!(options.service, Some(binkd_win::command::Command::Install));
        assert_eq!(options.service_name.as_deref(), Some("Binkd mailer"));
        assert_eq!(options.config.as_deref(), Some("binkd.conf"));
    }

    #[cfg(windows)]
    #[test]
    fn the_unknown_service_command_is_an_error() {
        let error = parse(&args(&["binkd", "-t", "other"])).unwrap_err();
        assert_eq!(error, Error::ServiceCommand("other".to_string()));
    }

    #[test]
    fn the_arguments_of_the_begin_line() {
        let list = args(&["-d", "my config.conf", ""]);
        assert_eq!(print_args(&list), " -d \"my config.conf\" \"\"");
    }

    #[test]
    fn the_m_switch_disables_the_crypt_too() {
        let options = parse(&args(&["binkd", "-m"])).unwrap();
        assert!(options.no_md5);
        assert!(options.no_crypt);
    }
}
