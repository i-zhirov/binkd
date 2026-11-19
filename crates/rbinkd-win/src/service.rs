//! The Windows service control manager, the port of `nt/service.c` and of the
//! `WIN32` blocks of `binkd.c`.
//!
//! The module holds the `-t` commands, the service dispatcher, the registry
//! records of the arguments of the service, and the status reports. The C code
//! of `nt/service.c` is the reference. The raw Win32 calls are behind the safe
//! API of `windows-service` and `winreg`; this crate keeps
//! `unsafe_code = "forbid"`.

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceDependency, ServiceErrorControl,
    ServiceExitCode, ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{
    self, ServiceControlHandlerResult, ServiceStatusHandle,
};
use windows_service::service_dispatcher;
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::Error as WindowsServiceError;
use winreg::enums::{HKEY_LOCAL_MACHINE, REG_BINARY, REG_SZ};
use winreg::{RegKey, RegValue};

use crate::command::{service_arguments, Command, ServiceName};

/// The name of the registry prefix of the services, the port of
/// `reg_path_prefix` of `nt/service.c`.
const REGISTRY_PREFIX: &str = "SYSTEM\\CurrentControlSet\\Services\\";
/// The suffix of the parameters of the service, the port of
/// `reg_path_suffix`.
const REGISTRY_SUFFIX: &str = "\\Parameters";
/// The description of the service, the port of `description` of
/// `nt/service.c`.
const DESCRIPTION: &str = "BinkD: Fidonet TCP/IP mailer uses binkp protocol";

/// The Win32 error of a program that is not started by the service control
/// manager, the port of `ERROR_FAILED_SERVICE_CONTROLLER_CONNECT`.
const ERROR_FAILED_SERVICE_CONTROLLER_CONNECT: i32 = 1063;
/// The Win32 error of a service that already runs.
const ERROR_SERVICE_ALREADY_RUNNING: i32 = 1056;
/// The Win32 error of a missing service.
const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;
/// The Win32 error of an existing service.
const ERROR_SERVICE_EXISTS: i32 = 1073;
/// The Win32 error of a service that is not active.
const ERROR_SERVICE_NOT_ACTIVE: i32 = 1062;

/// The event of the service control, the part of the control handler of the
/// port of `ServiceCtrl()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The service control manager asks the service to stop.
    Stop,
}

type EventHandler = Box<dyn Fn(Event) + Send + Sync>;

/// The values of a control event of the service.
struct ControlInner {
    handler: Option<EventHandler>,
    pending: Option<Event>,
    status: ServiceStatus,
    handle: Option<ServiceStatusHandle>,
}

/// The control channel of one service, the port of the `ServiceCtrl()` path of
/// `nt/service.c`.
#[derive(Clone)]
pub struct Control {
    inner: Arc<Mutex<ControlInner>>,
}

impl Control {
    fn new() -> Control {
        Control {
            inner: Arc::new(Mutex::new(ControlInner {
                handler: None,
                pending: None,
                status: initial_status(),
                handle: None,
            })),
        }
    }

    /// Sets the handler of the control events.
    ///
    /// A stop event that arrives before the handler is held until the handler
    /// is set, so the daemon does not miss the stop of a service that starts
    /// slowly.
    pub fn set_handler(&self, handler: impl Fn(Event) + Send + Sync + 'static) {
        let handler: EventHandler = Box::new(handler);
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if let Some(event) = inner.pending.take() {
            handler(event);
        }
        inner.handler = Some(handler);
    }

    fn event(&self, event: Event) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        match &inner.handler {
            Some(handler) => handler(event),
            None => inner.pending = Some(event),
        }
    }

    fn set_handle(&self, handle: ServiceStatusHandle) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.handle = Some(handle);
        }
    }

    /// Reports a state to the service control manager.
    fn report(&self, state: ServiceState, accepted: ServiceControlAccept, exit: u32, wait: u32) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let checkpoint = if matches!(
            state,
            ServiceState::StartPending | ServiceState::StopPending
        ) {
            inner.status.checkpoint.wrapping_add(1)
        } else {
            0
        };
        inner.status = ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: accepted,
            exit_code: if state == ServiceState::Stopped && exit != 0 {
                ServiceExitCode::ServiceSpecific(exit)
            } else {
                ServiceExitCode::Win32(exit)
            },
            checkpoint,
            wait_hint: Duration::from_millis(u64::from(wait)),
            process_id: None,
        };
        if let Some(handle) = inner.handle {
            let _ = handle.set_service_status(inner.status.clone());
        }
    }

    /// Sends the current status again, the answer to an interrogate event.
    fn resend_status(&self) {
        let Ok(inner) = self.inner.lock() else {
            return;
        };
        if let Some(handle) = inner.handle {
            let _ = handle.set_service_status(inner.status.clone());
        }
    }
}

fn initial_status() -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

/// The values of one service start, the port of the arguments of
/// `ServiceStart()` of `nt/service.c`.
pub struct ServiceContext {
    /// The command line of the service, the port of `serv_argv`.
    pub args: Vec<String>,
    /// The display name of the service, the port of `service_name` of
    /// `nt/service.c`.
    pub display: String,
    /// The control channel of the service.
    pub control: Control,
}

/// The error of the service mode.
#[derive(Debug)]
pub enum Error {
    /// The program is not started by the service control manager.
    NotService,
    /// The Windows API failed with this code.
    Windows(i32),
    /// The port itself failed.
    Message(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotService => write!(f, "the program is not a service"),
            Error::Windows(code) => write!(f, "the Windows API failed with the code {code}"),
            Error::Message(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Error {
        Error::Message(error.to_string())
    }
}

impl From<WindowsServiceError> for Error {
    fn from(error: WindowsServiceError) -> Error {
        if let Some(code) = error_code(&error) {
            Error::Windows(code)
        } else {
            Error::Message(error.to_string())
        }
    }
}

/// Returns the Win32 code of a service error.
fn error_code(error: &WindowsServiceError) -> Option<i32> {
    match error {
        WindowsServiceError::Winapi(error) => error.raw_os_error(),
        _ => None,
    }
}

/// Returns true for a service error with this code.
fn is_code(error: &WindowsServiceError, code: i32) -> bool {
    error_code(error) == Some(code)
}

type ServiceFn = Box<dyn FnOnce(ServiceContext) -> i32 + Send>;

static SERVICE: OnceLock<Mutex<Option<ServiceFn>>> = OnceLock::new();

/// Runs the service dispatcher, the port of `tell_start_ntservice()` of
/// `nt/service.c`.
///
/// The dispatcher answers [`Error::NotService`] when the program runs from the
/// console. The closure runs on the thread of the service control manager and
/// holds the daemon.
pub fn run<F>(service: F) -> Result<i32, Error>
where
    F: FnOnce(ServiceContext) -> i32 + Send + 'static,
{
    let _ = SERVICE.set(Mutex::new(Some(Box::new(service))));
    service_dispatcher::start("", ffi_service_main).map_err(|error| {
        if is_code(&error, ERROR_FAILED_SERVICE_CONTROLLER_CONNECT) {
            Error::NotService
        } else {
            Error::from(error)
        }
    })?;
    Ok(0)
}

windows_service::define_windows_service!(ffi_service_main, service_main);

/// Runs a `-t` command, the port of `service()` of `nt/service.c`.
pub fn command(command: Command, name: Option<&str>, config: Option<&str>, args: &[String]) -> i32 {
    let name = resolve_name(ServiceName::new(name));
    match run_command(command, &name, config, args) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn run_command(
    command: Command,
    name: &ServiceName,
    config: Option<&str>,
    args: &[String],
) -> Result<(), Error> {
    match command {
        Command::Install => {
            if config.is_none() {
                return Err(Error::Message(
                    "the config file is missing for the service".to_string(),
                ));
            }
            if !install(name, &service_arguments(args))? {
                println!("Service '{}' is already installed.", name.display);
                return Ok(());
            }
            start(name)?;
            println!(
                "Service '{}' installed and started successfully.",
                name.display
            );
        }
        Command::Uninstall => {
            stop_if_running(name)?;
            if uninstall(name)? {
                println!("Service '{}' uninstalled successfully.", name.display);
            } else {
                println!("Service '{}' is already uninstalled.", name.display);
            }
        }
        Command::Start => {
            if start(name)? {
                println!("Service '{}' started successfully.", name.display);
            } else {
                println!("Service '{}' is already started.", name.display);
            }
        }
        Command::Stop => match stop(name)? {
            StopOutcome::Stopped => {
                println!("Service '{}' stopped successfully.", name.display);
            }
            StopOutcome::NotInstalled => {
                println!("Service '{}' is not installed.", name.display);
            }
            StopOutcome::NotRunning => {
                println!("Service '{}' is not running.", name.display);
            }
        },
        Command::Restart => {
            stop_if_running(name)?;
            start(name)?;
            println!("Service '{}' restarted successfully.", name.display);
        }
        Command::Status => status(name)?,
    }
    Ok(())
}

fn status(name: &ServiceName) -> Result<(), Error> {
    let manager = open_manager_access(
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::ENUMERATE_SERVICE,
    )?;
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::INTERROGATE;
    let service = match open_service_access(&manager, name, access) {
        Ok(service) => service,
        Err(Error::Windows(ERROR_SERVICE_DOES_NOT_EXIST)) => {
            println!("Service '{}' is not installed.", name.display);
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    let state = state(&service)?;
    println!(
        "Service '{}' is installed and is {}.",
        name.display,
        describe_state(state)
    );
    Ok(())
}

fn describe_state(state: ServiceState) -> &'static str {
    match state {
        ServiceState::Stopped => "stopped",
        ServiceState::StartPending => "starting",
        ServiceState::StopPending => "stopping",
        ServiceState::Running => "running",
        _ => "in an unknown state",
    }
}

fn start(name: &ServiceName) -> Result<bool, Error> {
    let manager = open_manager()?;
    let service = open_service(&manager, name)?;
    match state(&service) {
        Ok(ServiceState::Running) => return Ok(false),
        Ok(_) => {}
        Err(Error::Windows(ERROR_SERVICE_NOT_ACTIVE)) => {}
        Err(error) => return Err(error),
    }
    let args: [&OsStr; 0] = [];
    match service.start(&args) {
        Ok(()) => {}
        Err(error) if is_code(&error, ERROR_SERVICE_ALREADY_RUNNING) => return Ok(false),
        Err(error) => return Err(error.into()),
    }
    wait(&service, ServiceState::StartPending, ServiceState::Running)?;
    Ok(true)
}

fn stop_if_running(name: &ServiceName) -> Result<(), Error> {
    let manager = open_manager()?;
    let service = match open_service(&manager, name) {
        Ok(service) => service,
        Err(Error::Windows(ERROR_SERVICE_DOES_NOT_EXIST)) => return Ok(()),
        Err(error) => return Err(error),
    };
    match state(&service) {
        Ok(ServiceState::Stopped) => Ok(()),
        Ok(_) => stop_service(&service),
        Err(Error::Windows(ERROR_SERVICE_NOT_ACTIVE)) => Ok(()),
        Err(error) => Err(error),
    }
}

/// The outcome of a stop command.
enum StopOutcome {
    /// The service stopped.
    Stopped,
    /// The service is not installed.
    NotInstalled,
    /// The service is installed but stopped.
    NotRunning,
}

fn stop(name: &ServiceName) -> Result<StopOutcome, Error> {
    let manager = open_manager()?;
    let service = match open_service(&manager, name) {
        Ok(service) => service,
        Err(Error::Windows(ERROR_SERVICE_DOES_NOT_EXIST)) => return Ok(StopOutcome::NotInstalled),
        Err(error) => return Err(error),
    };
    match state(&service) {
        Ok(ServiceState::Stopped) => Ok(StopOutcome::NotRunning),
        Ok(_) => {
            stop_service(&service)?;
            Ok(StopOutcome::Stopped)
        }
        Err(Error::Windows(ERROR_SERVICE_NOT_ACTIVE)) => Ok(StopOutcome::NotRunning),
        Err(error) => Err(error),
    }
}

fn stop_service(service: &windows_service::service::Service) -> Result<(), Error> {
    // The C code of `stop_service()` reports success when `ControlService()`
    // fails, and the caller observes the state. The control pipe of the
    // service can break while the service stops, the error 109.
    let _ = service.stop();
    wait(service, ServiceState::StopPending, ServiceState::Stopped)
}

fn state(service: &windows_service::service::Service) -> Result<ServiceState, Error> {
    service
        .query_status()
        .map(|status| status.current_state)
        .map_err(Into::into)
}

/// Waits for a service operation, the port of `wait_service_operation()` of
/// `nt/service.c`.
fn wait(
    service: &windows_service::service::Service,
    pending: ServiceState,
    finish: ServiceState,
) -> Result<(), Error> {
    for i in 0..30u32 {
        let current = state(service)?;
        if current == finish {
            return Ok(());
        }
        if current != pending && i >= 3 {
            return Err(Error::Message(format!(
                "the service did not reach the state {finish:?}"
            )));
        }
        std::thread::sleep(Duration::from_millis(if i < 3 { 100 } else { 300 }));
    }
    Err(Error::Message(format!(
        "the service did not reach the state {finish:?}"
    )))
}

fn install(name: &ServiceName, args: &[String]) -> Result<bool, Error> {
    let manager =
        open_manager_access(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    let binary_path = std::env::current_exe()?;
    let info = ServiceInfo {
        name: OsString::from(&name.key),
        display_name: OsString::from(&name.display),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: binary_path.clone(),
        launch_arguments: Vec::new(),
        dependencies: vec![
            ServiceDependency::Service(OsString::from("Tcpip")),
            ServiceDependency::Service(OsString::from("Afd")),
        ],
        account_name: None,
        account_password: None,
    };
    let service = match manager.create_service(&info, ServiceAccess::ALL_ACCESS) {
        Ok(service) => service,
        Err(error) if is_code(&error, ERROR_SERVICE_EXISTS) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let _ = service.set_description(DESCRIPTION);
    let mut stored = args.to_vec();
    if let Some(first) = stored.first_mut() {
        *first = binary_path.to_string_lossy().into_owned();
    }
    store_registry(&name.key, &stored)?;
    Ok(true)
}

fn uninstall(name: &ServiceName) -> Result<bool, Error> {
    let manager = open_manager()?;
    let service = match open_service(&manager, name) {
        Ok(service) => service,
        Err(Error::Windows(ERROR_SERVICE_DOES_NOT_EXIST)) => return Ok(false),
        Err(error) => return Err(error),
    };
    service.delete().map_err(Error::from)?;
    Ok(true)
}

fn open_manager() -> Result<ServiceManager, Error> {
    open_manager_access(ServiceManagerAccess::ALL_ACCESS)
}

fn open_manager_access(access: ServiceManagerAccess) -> Result<ServiceManager, Error> {
    ServiceManager::local_computer(None::<&OsStr>, access).map_err(Into::into)
}

fn open_service(
    manager: &ServiceManager,
    name: &ServiceName,
) -> Result<windows_service::service::Service, Error> {
    open_service_access(manager, name, ServiceAccess::ALL_ACCESS)
}

fn open_service_access(
    manager: &ServiceManager,
    name: &ServiceName,
    access: ServiceAccess,
) -> Result<windows_service::service::Service, Error> {
    manager.open_service(&name.key, access).map_err(Into::into)
}

/// Resolves the key of a service from its display name, the port of
/// `get_service_name()` of `nt/w32tools.c`.
fn resolve_name(name: ServiceName) -> ServiceName {
    let Ok(manager) = open_manager_access(
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::ENUMERATE_SERVICE,
    ) else {
        return name;
    };
    match manager.service_name_from_display_name(&name.display) {
        Ok(key) => ServiceName {
            key: key.to_string_lossy().into_owned(),
            ..name
        },
        Err(_) => name,
    }
}

/// Returns the display name of a service, the port of `NTServiceDisplayName()`
/// of `nt/service.c`.
fn display_name(key: &str) -> String {
    let Ok(manager) = open_manager_access(
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::ENUMERATE_SERVICE,
    ) else {
        return key.to_string();
    };
    let Ok(service) = manager.open_service(key, ServiceAccess::QUERY_CONFIG) else {
        return key.to_string();
    };
    service
        .query_config()
        .map(|config| config.display_name.to_string_lossy().into_owned())
        .unwrap_or_else(|_| key.to_string())
}

fn store_registry(key: &str, args: &[String]) -> Result<(), Error> {
    let path = format!("{REGISTRY_PREFIX}{key}{REGISTRY_SUFFIX}");
    let (parameters, _) = RegKey::predef(HKEY_LOCAL_MACHINE).create_subkey(path)?;
    let cwd = std::env::current_dir()?;
    let cwd = wide_path(&cwd);
    let args = wide_arguments(args);
    parameters.set_raw_value(
        "path",
        &RegValue {
            vtype: REG_SZ,
            bytes: bytes(&cwd).into(),
        },
    )?;
    parameters.set_raw_value(
        "args",
        &RegValue {
            vtype: REG_BINARY,
            bytes: bytes(&args).into(),
        },
    )?;
    Ok(())
}

fn read_registry(key: &str) -> Result<(Vec<String>, PathBuf), Error> {
    let path = format!("{REGISTRY_PREFIX}{key}{REGISTRY_SUFFIX}");
    let parameters = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(path)?;
    let path = parameters.get_raw_value("path")?;
    let args = parameters.get_raw_value("args")?;
    let cwd = string_from_utf16(path.bytes.as_ref());
    let args = arguments_from_utf16(args.bytes.as_ref());
    Ok((args, PathBuf::from(cwd)))
}

fn service_main(arguments: Vec<OsString>) {
    let name = arguments
        .first()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = if name.is_empty() {
        ServiceName::new(None).key
    } else {
        name
    };
    // The control channel exists before the stop can arrive, so the daemon
    // does not miss a stop of a service that starts slowly.
    let control = Control::new();
    let handler_control = control.clone();
    let status_handle = match service_control_handler::register(&name, move |event| match event {
        ServiceControl::Stop => {
            handler_control.report(
                ServiceState::StopPending,
                ServiceControlAccept::empty(),
                0,
                0,
            );
            handler_control.event(Event::Stop);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => {
            handler_control.resend_status();
            ServiceControlHandlerResult::NoError
        }
        _ => ServiceControlHandlerResult::NotImplemented,
    }) {
        Ok(handle) => handle,
        Err(_) => return,
    };
    control.set_handle(status_handle);
    control.report(
        ServiceState::StartPending,
        ServiceControlAccept::empty(),
        0,
        3000,
    );
    let Ok((args, cwd)) = read_registry(&name) else {
        control.report(ServiceState::Stopped, ServiceControlAccept::empty(), 0, 0);
        return;
    };
    if !cwd.as_os_str().is_empty() {
        let _ = std::env::set_current_dir(cwd);
    }
    let display = display_name(&name);
    control.report(ServiceState::Running, ServiceControlAccept::STOP, 0, 0);
    let service = SERVICE
        .get()
        .and_then(|service| service.lock().ok().and_then(|mut slot| slot.take()));
    if let Some(service) = service {
        let code = service(ServiceContext {
            args,
            display,
            control: control.clone(),
        });
        control.report(
            ServiceState::Stopped,
            ServiceControlAccept::empty(),
            code as u32,
            0,
        );
    } else {
        control.report(ServiceState::Stopped, ServiceControlAccept::empty(), 0, 0);
    }
}

fn wide_path(path: &std::path::Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn wide_arguments(args: &[String]) -> Vec<u16> {
    let mut out = Vec::new();
    for arg in args {
        out.extend(OsStr::new(arg).encode_wide());
        out.push(0);
    }
    out.push(0);
    out
}

fn bytes(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn string_from_utf16(data: &[u8]) -> String {
    let words: Vec<u16> = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|word| *word != 0)
        .collect();
    String::from_utf16_lossy(&words)
}

fn arguments_from_utf16(data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    for chunk in data.chunks_exact(2) {
        let word = u16::from_le_bytes([chunk[0], chunk[1]]);
        if word == 0 {
            if !current.is_empty() {
                out.push(String::from_utf16_lossy(&current));
                current.clear();
            }
        } else {
            current.push(word);
        }
    }
    if !current.is_empty() {
        out.push(String::from_utf16_lossy(&current));
    }
    out
}
