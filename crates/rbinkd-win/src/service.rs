//! The Windows service control manager, the port of `nt/service.c` and of the
//! `WIN32` blocks of `binkd.c`.
//!
//! The module holds the `-t` commands, the service dispatcher, the registry
//! records of the arguments of the service, and the status reports. The C code
//! of `nt/service.c` is the reference. This crate and `rbinkd-perl` hold the
//! two exceptions of the lint `unsafe_code` for the FFI, the decision of
//! `doc/development.md`.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_FAILED_SERVICE_CONTROLLER_CONNECT, ERROR_SERVICE_ALREADY_RUNNING,
    ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_EXISTS, ERROR_SERVICE_NOT_ACTIVE,
    ERROR_SERVICE_SPECIFIC_ERROR, WIN32_ERROR,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, REG_BINARY, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, CloseServiceHandle, ControlService, CreateServiceW, DeleteService,
    GetServiceDisplayNameW, GetServiceKeyNameW, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx,
    RegisterServiceCtrlHandlerW, SetServiceStatus, StartServiceCtrlDispatcherW, StartServiceW,
    SC_MANAGER_ALL_ACCESS, SC_MANAGER_ENUMERATE_SERVICE, SC_STATUS_PROCESS_INFO,
    SERVICE_ACCEPT_STOP, SERVICE_ALL_ACCESS, SERVICE_AUTO_START, SERVICE_CONFIG_DESCRIPTION,
    SERVICE_CONTROL_INTERROGATE, SERVICE_CONTROL_STOP, SERVICE_DESCRIPTIONW, SERVICE_ERROR_NORMAL,
    SERVICE_INTERROGATE, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START_PENDING,
    SERVICE_STATUS, SERVICE_STATUS_PROCESS, SERVICE_STOPPED, SERVICE_STOP_PENDING,
    SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS,
};

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
    Windows(WIN32_ERROR),
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

type ServiceFn = Box<dyn FnOnce(ServiceContext) -> i32 + Send>;

static SERVICE: OnceLock<Mutex<Option<ServiceFn>>> = OnceLock::new();
static CONTROL: OnceLock<Control> = OnceLock::new();
static STATUS_HANDLE: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(null_mut());
static STATUS: Mutex<SERVICE_STATUS> = Mutex::new(SERVICE_STATUS {
    dwServiceType: SERVICE_WIN32_OWN_PROCESS,
    dwCurrentState: SERVICE_STOPPED,
    dwControlsAccepted: 0,
    dwWin32ExitCode: 0,
    dwServiceSpecificExitCode: 0,
    dwCheckPoint: 0,
    dwWaitHint: 0,
});

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
    let mut name = vec![0u16];
    let mut table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: name.as_mut_ptr(),
            lpServiceProc: Some(service_main_dispatch),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: null_mut(),
            lpServiceProc: None,
        },
    ];
    let ok = unsafe { StartServiceCtrlDispatcherW(table.as_mut_ptr()) };
    if ok == 0 {
        let code = unsafe { GetLastError() };
        if code == ERROR_FAILED_SERVICE_CONTROLLER_CONNECT {
            return Err(Error::NotService);
        }
        return Err(Error::Windows(code));
    }
    Ok(0)
}

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
    let manager = open_manager_access(SC_MANAGER_ENUMERATE_SERVICE)?;
    let access = SERVICE_QUERY_STATUS | SERVICE_INTERROGATE;
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

fn describe_state(state: u32) -> &'static str {
    match state {
        SERVICE_STOPPED => "stopped",
        SERVICE_START_PENDING => "starting",
        SERVICE_STOP_PENDING => "stopping",
        SERVICE_RUNNING => "running",
        _ => "in an unknown state",
    }
}

fn start(name: &ServiceName) -> Result<bool, Error> {
    let manager = open_manager()?;
    let service = open_service(&manager, name)?;
    match state(&service) {
        Ok(SERVICE_RUNNING) => return Ok(false),
        Ok(_) => {}
        Err(Error::Windows(ERROR_SERVICE_NOT_ACTIVE)) => {}
        Err(error) => return Err(error),
    }
    let ok = unsafe { StartServiceW(service.0, 0, null()) };
    if ok == 0 {
        let code = unsafe { GetLastError() };
        if code == ERROR_SERVICE_ALREADY_RUNNING {
            return Ok(false);
        }
        return Err(Error::Windows(code));
    }
    wait(&service, SERVICE_START_PENDING, SERVICE_RUNNING)?;
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
        Ok(SERVICE_STOPPED) => Ok(()),
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
        Ok(SERVICE_STOPPED) => Ok(StopOutcome::NotRunning),
        Ok(_) => {
            stop_service(&service)?;
            Ok(StopOutcome::Stopped)
        }
        Err(Error::Windows(ERROR_SERVICE_NOT_ACTIVE)) => Ok(StopOutcome::NotRunning),
        Err(error) => Err(error),
    }
}

fn stop_service(service: &ServiceHandle) -> Result<(), Error> {
    let mut status = SERVICE_STATUS::default();
    let ok = unsafe { ControlService(service.0, SERVICE_CONTROL_STOP, &mut status) };
    if ok == 0 {
        // The C code of `stop_service()` reports success when `ControlService()`
        // fails, and the caller observes the state. The control pipe of the
        // service can break while the service stops, the error 109.
        return Ok(());
    }
    wait(service, SERVICE_STOP_PENDING, SERVICE_STOPPED)
}

fn state(service: &ServiceHandle) -> Result<u32, Error> {
    let mut status = SERVICE_STATUS_PROCESS::default();
    let mut needed = 0u32;
    let ok = unsafe {
        QueryServiceStatusEx(
            service.0,
            SC_STATUS_PROCESS_INFO,
            &mut status as *mut SERVICE_STATUS_PROCESS as *mut u8,
            std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(Error::Windows(unsafe { GetLastError() }));
    }
    Ok(status.dwCurrentState)
}

/// Waits for a service operation, the port of `wait_service_operation()` of
/// `nt/service.c`.
fn wait(service: &ServiceHandle, pending: u32, finish: u32) -> Result<(), Error> {
    for i in 0..30u32 {
        let current = state(service)?;
        if current == finish {
            return Ok(());
        }
        if current != pending && i >= 3 {
            return Err(Error::Message(format!(
                "the service did not reach the state {finish}"
            )));
        }
        std::thread::sleep(Duration::from_millis(if i < 3 { 100 } else { 300 }));
    }
    Err(Error::Message(format!(
        "the service did not reach the state {finish}"
    )))
}

fn install(name: &ServiceName, args: &[String]) -> Result<bool, Error> {
    let manager = open_manager()?;
    let binary_path = std::env::current_exe()?;
    let binary = wide_path(&binary_path);
    let key = wide(&name.key);
    let display = wide(&name.display);
    let dependencies = wide_list(&["Tcpip", "Afd"]);
    let service = unsafe {
        CreateServiceW(
            manager.0,
            key.as_ptr(),
            display.as_ptr(),
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            binary.as_ptr(),
            null(),
            null_mut(),
            dependencies.as_ptr(),
            null(),
            null(),
        )
    };
    if service.is_null() {
        let code = unsafe { GetLastError() };
        if code == ERROR_SERVICE_EXISTS {
            return Ok(false);
        }
        return Err(Error::Windows(code));
    }
    let service = ServiceHandle(service);
    let description = wide(DESCRIPTION);
    let description = SERVICE_DESCRIPTIONW {
        lpDescription: description.as_ptr() as PWSTR,
    };
    let _ = unsafe {
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_DESCRIPTION,
            &description as *const SERVICE_DESCRIPTIONW as *const core::ffi::c_void,
        )
    };
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
    let ok = unsafe { DeleteService(service.0) };
    if ok == 0 {
        return Err(Error::Windows(unsafe { GetLastError() }));
    }
    Ok(true)
}

fn open_manager() -> Result<ServiceHandle, Error> {
    open_manager_access(SC_MANAGER_ALL_ACCESS)
}

fn open_manager_access(access: u32) -> Result<ServiceHandle, Error> {
    let manager = unsafe { OpenSCManagerW(null(), null(), access) };
    if manager.is_null() {
        return Err(Error::Windows(unsafe { GetLastError() }));
    }
    Ok(ServiceHandle(manager))
}

fn open_service(manager: &ServiceHandle, name: &ServiceName) -> Result<ServiceHandle, Error> {
    open_service_access(manager, name, SERVICE_ALL_ACCESS)
}

fn open_service_access(
    manager: &ServiceHandle,
    name: &ServiceName,
    access: u32,
) -> Result<ServiceHandle, Error> {
    let key = wide(&name.key);
    let service = unsafe { OpenServiceW(manager.0, key.as_ptr(), access) };
    if service.is_null() {
        return Err(Error::Windows(unsafe { GetLastError() }));
    }
    Ok(ServiceHandle(service))
}

/// Resolves the key of a service from its display name, the port of
/// `get_service_name()` of `nt/w32tools.c`.
fn resolve_name(name: ServiceName) -> ServiceName {
    let Ok(manager) = open_manager_access(SC_MANAGER_ENUMERATE_SERVICE) else {
        return name;
    };
    match service_key_name(&manager, &name.display) {
        Some(key) => ServiceName { key, ..name },
        None => name,
    }
}

/// Returns the display name of a service, the port of `NTServiceDisplayName()`
/// of `nt/service.c`.
fn display_name(key: &str) -> String {
    let Ok(manager) = open_manager_access(SC_MANAGER_ENUMERATE_SERVICE) else {
        return key.to_string();
    };
    service_display_name(&manager, key).unwrap_or_else(|| key.to_string())
}

fn service_key_name(manager: &ServiceHandle, display: &str) -> Option<String> {
    let name = wide(display);
    let mut buffer = vec![0u16; 256];
    let mut length = buffer.len() as u32;
    let ok =
        unsafe { GetServiceKeyNameW(manager.0, name.as_ptr(), buffer.as_mut_ptr(), &mut length) };
    if ok == 0 {
        return None;
    }
    Some(wide_to_string(&buffer))
}

fn service_display_name(manager: &ServiceHandle, key: &str) -> Option<String> {
    let name = wide(key);
    let mut buffer = vec![0u16; 256];
    let mut length = buffer.len() as u32;
    let ok = unsafe {
        GetServiceDisplayNameW(manager.0, name.as_ptr(), buffer.as_mut_ptr(), &mut length)
    };
    if ok == 0 {
        return None;
    }
    Some(wide_to_string(&buffer))
}

fn wide_to_string(buffer: &[u16]) -> String {
    let length = buffer
        .iter()
        .position(|word| *word == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

fn store_registry(key: &str, args: &[String]) -> Result<(), Error> {
    let path = format!("{REGISTRY_PREFIX}{key}{REGISTRY_SUFFIX}");
    let path = wide(&path);
    let mut handle: HKEY = null_mut();
    let mut disposition = 0u32;
    let code = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            path.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_ALL_ACCESS,
            null(),
            &mut handle,
            &mut disposition,
        )
    };
    if code != 0 {
        return Err(Error::Windows(code));
    }
    let handle = RegistryKey(handle);
    let cwd = std::env::current_dir()?;
    let cwd = wide_path(&cwd);
    set_registry(&handle, "path", REG_SZ, &bytes(&cwd))?;
    let args = wide_arguments(args);
    set_registry(&handle, "args", REG_BINARY, &bytes(&args))?;
    Ok(())
}

fn set_registry(key: &RegistryKey, name: &str, kind: u32, data: &[u8]) -> Result<(), Error> {
    let name = wide(name);
    let code = unsafe {
        RegSetValueExW(
            key.0,
            name.as_ptr(),
            0,
            kind,
            data.as_ptr(),
            data.len() as u32,
        )
    };
    if code != 0 {
        return Err(Error::Windows(code));
    }
    Ok(())
}

fn registry_path(key: &str) -> Vec<u16> {
    wide(&format!("{REGISTRY_PREFIX}{key}{REGISTRY_SUFFIX}"))
}

fn registry(key: &str) -> Result<RegistryKey, Error> {
    let path = registry_path(key);
    let mut handle: HKEY = null_mut();
    let code = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            path.as_ptr(),
            0,
            KEY_ALL_ACCESS,
            &mut handle,
        )
    };
    if code != 0 {
        return Err(Error::Windows(code));
    }
    Ok(RegistryKey(handle))
}

fn query_registry(key: &RegistryKey, name: &str) -> Result<(Vec<u8>, u32), Error> {
    let name = wide(name);
    let mut kind = 0u32;
    let mut size = 0u32;
    let code = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            null_mut(),
            &mut size,
        )
    };
    if code != 0 {
        return Err(Error::Windows(code));
    }
    let mut data = vec![0u8; size as usize];
    let code = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            data.as_mut_ptr(),
            &mut size,
        )
    };
    if code != 0 {
        return Err(Error::Windows(code));
    }
    data.truncate(size as usize);
    Ok((data, kind))
}

fn read_registry(key: &str) -> Result<(Vec<String>, PathBuf), Error> {
    let key = registry(key)?;
    let (path, _) = query_registry(&key, "path")?;
    let (args, _) = query_registry(&key, "args")?;
    let cwd = string_from_utf16(&path);
    let args = arguments_from_utf16(&args);
    Ok((args, PathBuf::from(cwd)))
}

fn service_main(argc: u32, argv: *mut PWSTR) {
    let name = if argc == 0 || argv.is_null() {
        String::new()
    } else {
        string_from_wide(unsafe { *argv })
    };
    let name = if name.is_empty() {
        crate::command::ServiceName::new(None).key
    } else {
        name
    };
    // The control channel exists before the stop can arrive, so the daemon
    // does not miss a stop of a service that starts slowly.
    let control = Control::new();
    let _ = CONTROL.set(control.clone());
    let name_wide = wide(&name);
    let handle = unsafe { RegisterServiceCtrlHandlerW(name_wide.as_ptr(), Some(control_handler)) };
    if handle.is_null() {
        return;
    }
    STATUS_HANDLE.store(handle.cast(), Ordering::SeqCst);
    report(SERVICE_START_PENDING, 0, 0, 3000);
    let Ok((args, cwd)) = read_registry(&name) else {
        report(SERVICE_STOPPED, 0, 0, 0);
        return;
    };
    if !cwd.as_os_str().is_empty() {
        let _ = std::env::set_current_dir(cwd);
    }
    let display = display_name(&name);
    report(SERVICE_RUNNING, SERVICE_ACCEPT_STOP, 0, 0);
    let service = SERVICE
        .get()
        .and_then(|service| service.lock().ok().and_then(|mut slot| slot.take()));
    if let Some(service) = service {
        let code = service(ServiceContext {
            args,
            display,
            control,
        });
        report(SERVICE_STOPPED, 0, code as u32, 0);
    } else {
        report(SERVICE_STOPPED, 0, 0, 0);
    }
}

unsafe extern "system" fn service_main_dispatch(argc: u32, argv: *mut PWSTR) {
    service_main(argc, argv)
}

unsafe extern "system" fn control_handler(control: u32) {
    match control {
        SERVICE_CONTROL_STOP => {
            report(SERVICE_STOP_PENDING, 0, 0, 0);
            if let Some(control) = CONTROL.get() {
                control.event(Event::Stop);
            }
        }
        SERVICE_CONTROL_INTERROGATE => {
            let status = STATUS.lock().map(|status| *status).unwrap_or_default();
            let handle = STATUS_HANDLE.load(Ordering::SeqCst);
            if !handle.is_null() {
                unsafe {
                    SetServiceStatus(handle, &status);
                }
            }
        }
        _ => {}
    }
}

fn report(state: u32, accepted: u32, exit: u32, wait: u32) {
    let handle = STATUS_HANDLE.load(Ordering::SeqCst);
    if handle.is_null() {
        return;
    }
    let Ok(mut status) = STATUS.lock() else {
        return;
    };
    status.dwCurrentState = state;
    status.dwControlsAccepted = accepted;
    if state == SERVICE_STOPPED && exit != 0 {
        // A daemon code is not a Win32 error, so the status holds it as the
        // service-specific code, the error code of the service.
        status.dwWin32ExitCode = ERROR_SERVICE_SPECIFIC_ERROR;
        status.dwServiceSpecificExitCode = exit;
    } else {
        status.dwWin32ExitCode = exit;
        status.dwServiceSpecificExitCode = 0;
    }
    status.dwWaitHint = wait;
    status.dwCheckPoint = if state == SERVICE_START_PENDING || state == SERVICE_STOP_PENDING {
        status.dwCheckPoint.wrapping_add(1)
    } else {
        0
    };
    unsafe {
        SetServiceStatus(handle, &*status);
    }
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn wide_path(path: &std::path::Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn wide_list(values: &[&str]) -> Vec<u16> {
    let mut out = Vec::new();
    for value in values {
        out.extend(OsStr::new(value).encode_wide());
        out.push(0);
    }
    out.push(0);
    out
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

fn string_from_wide(pointer: PWSTR) -> String {
    if pointer.is_null() {
        return String::new();
    }
    let mut length = 0;
    while unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(pointer, length) })
}

struct ServiceHandle(*mut core::ffi::c_void);

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CloseServiceHandle(self.0);
            }
        }
    }
}

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
}
