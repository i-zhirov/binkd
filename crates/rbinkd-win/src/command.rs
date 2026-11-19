//! The `-t` commands and the `-S` service name, the port of the command line
//! part of `nt/service.c`.

/// A `-t` command of the service control, the port of `enum serviceflags` of
/// `binkd.c`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// `-t install`: install the service and start it.
    Install,
    /// `-t uninstall`: stop and remove the service.
    Uninstall,
    /// `-t start`: start the service.
    Start,
    /// `-t stop`: stop the service.
    Stop,
    /// `-t restart`: stop and start the service.
    Restart,
    /// `-t status`: report the state of the service.
    Status,
}

impl Command {
    /// Parses the argument of `-t`, the port of the `strcmp()` chain of
    /// `parseargs()`.
    pub fn parse(value: &str) -> Option<Command> {
        match value {
            "install" => Some(Command::Install),
            "uninstall" => Some(Command::Uninstall),
            "start" => Some(Command::Start),
            "stop" => Some(Command::Stop),
            "restart" => Some(Command::Restart),
            "status" => Some(Command::Status),
            _ => None,
        }
    }
}

/// The default display name of the service, the port of `service_name` of
/// `nt/service.c` when the command line holds no `-S`.
pub const DEFAULT_DISPLAY_NAME: &str = "rbinkd-service";

/// The display name and the key name of a service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceName {
    /// The display name, the port of `service_name`.
    pub display: String,
    /// The key name of the SCM, the port of `srvname`.
    pub key: String,
}

impl ServiceName {
    /// Builds the name from the `-S` value, the port of the fallback of
    /// `get_service_name()` of `nt/w32tools.c`.
    pub fn new(display: Option<&str>) -> ServiceName {
        let display = display.unwrap_or(DEFAULT_DISPLAY_NAME).to_string();
        let key = service_key(&display);
        ServiceName { display, key }
    }
}

/// Returns the key name of a display name, the fallback of `get_service_name()`
/// when the SCM holds no name.
pub fn service_key(display: &str) -> String {
    display
        .chars()
        .map(|c| match c {
            ' ' => '_',
            ',' | '/' => '-',
            '\\' => '=',
            other => other,
        })
        .collect()
}

/// Returns the command line of the service: the program and the arguments
/// without the switches of the service, the port of `build_service_arguments()`
/// of `nt/w32tools.c`.
pub fn service_arguments(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip = false;
    for (index, arg) in args.iter().enumerate() {
        if index == 0 {
            out.push(arg.clone());
            continue;
        }
        if skip {
            skip = false;
            continue;
        }
        if arg == "-t" || arg == "-S" {
            skip = true;
            continue;
        }
        if arg.starts_with("-t") || arg.starts_with("-S") {
            continue;
        }
        if arg == "-i" || arg == "-u" {
            continue;
        }
        out.push(arg.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_parse() {
        assert_eq!(Command::parse("install"), Some(Command::Install));
        assert_eq!(Command::parse("uninstall"), Some(Command::Uninstall));
        assert_eq!(Command::parse("start"), Some(Command::Start));
        assert_eq!(Command::parse("stop"), Some(Command::Stop));
        assert_eq!(Command::parse("restart"), Some(Command::Restart));
        assert_eq!(Command::parse("status"), Some(Command::Status));
        assert_eq!(Command::parse("other"), None);
    }

    #[test]
    fn the_default_name_is_the_rust_name() {
        let name = ServiceName::new(None);
        assert_eq!(name.display, "rbinkd-service");
        assert_eq!(name.key, "rbinkd-service");
    }

    #[test]
    fn the_legacy_service_name_is_explicit() {
        let name = ServiceName::new(Some("binkd-service"));
        assert_eq!(name.display, "binkd-service");
        assert_eq!(name.key, "binkd-service");
    }

    #[test]
    fn the_key_follows_the_c_fallback() {
        assert_eq!(service_key("binkd service"), "binkd_service");
        assert_eq!(service_key("a,b/c\\d"), "a-b-c=d");
        assert_eq!(ServiceName::new(Some("Binkd Mailer")).key, "Binkd_Mailer");
    }

    #[test]
    fn the_service_arguments_drop_the_service_switches() {
        let args = vec![
            "binkd".to_string(),
            "-t".to_string(),
            "install".to_string(),
            "-S".to_string(),
            "Binkd mailer".to_string(),
            "binkd.conf".to_string(),
        ];
        assert_eq!(
            service_arguments(&args),
            vec!["binkd".to_string(), "binkd.conf".to_string()]
        );
        let args = vec![
            "binkd".to_string(),
            "-tinstall".to_string(),
            "-SOther".to_string(),
            "-C".to_string(),
            "binkd.conf".to_string(),
        ];
        assert_eq!(
            service_arguments(&args),
            vec![
                "binkd".to_string(),
                "-C".to_string(),
                "binkd.conf".to_string()
            ]
        );
        let args = vec![
            "binkd".to_string(),
            "-C".to_string(),
            "binkd.conf".to_string(),
        ];
        assert_eq!(service_arguments(&args), args);
    }
}
