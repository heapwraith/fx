//! `fx-indexd` as a Windows service (T2.5): running under the Service
//! Control Manager, and installing or removing it.
//!
//! Installing copies the executable to `%ProgramFiles%\fx` (only
//! administrators may write there): a SYSTEM service must never run a file
//! the user, or anything running as the user, could replace. Removing stops
//! the service, deletes it, and deletes that copy and its folder.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::System::Services::{
    ChangeServiceConfig2W, CloseServiceHandle, ControlService, CreateServiceW, DeleteService,
    OpenSCManagerW, OpenServiceW, QueryServiceStatus, RegisterServiceCtrlHandlerExW, SC_HANDLE,
    SC_MANAGER_ALL_ACCESS, SC_MANAGER_CONNECT, SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP,
    SERVICE_ALL_ACCESS, SERVICE_AUTO_START, SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
    SERVICE_CONFIG_DESCRIPTION, SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP,
    SERVICE_DELAYED_AUTO_START_INFO, SERVICE_DESCRIPTIONW, SERVICE_ERROR_NORMAL,
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STATUS,
    SERVICE_STATUS_CURRENT_STATE, SERVICE_STATUS_HANDLE, SERVICE_STOP_PENDING, SERVICE_STOPPED,
    SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS, SetServiceStatus, StartServiceCtrlDispatcherW,
    StartServiceW,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

/// The service's name in the Service Control Manager.
pub const SERVICE_NAME: &str = "fx-indexd";
const DISPLAY_NAME: &str = "Pathdash — índice de arquivos";
const DESCRIPTION: &str = "Mantém o índice dos nomes de arquivo dos discos NTFS para a busca \
     instantânea do Pathdash. Opcional: o Pathdash funciona sem ele.";
/// Waiting for the service to stop (when removing it).
const STOP_TIMEOUT: Duration = Duration::from_secs(15);

/// A Service Control Manager handle, closed on drop.
struct Sc(SC_HANDLE);

impl Drop for Sc {
    fn drop(&mut self) {
        // SAFETY: we own the handle and close it once.
        let _ = unsafe { CloseServiceHandle(self.0) };
    }
}

fn manager(access: u32) -> io::Result<Sc> {
    // SAFETY: local machine, default database.
    unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), access) }
        .map(Sc)
        .map_err(io::Error::from)
}

fn open_service(m: &Sc, access: u32) -> io::Result<Sc> {
    // SAFETY: valid manager handle; NUL-terminated name kept by HSTRING.
    unsafe { OpenServiceW(m.0, &HSTRING::from(SERVICE_NAME), access) }
        .map(Sc)
        .map_err(io::Error::from)
}

fn state(s: &Sc) -> io::Result<SERVICE_STATUS_CURRENT_STATE> {
    let mut status = SERVICE_STATUS::default();
    // SAFETY: valid service handle; `status` is writable.
    unsafe { QueryServiceStatus(s.0, &mut status) }.map_err(io::Error::from)?;
    Ok(status.dwCurrentState)
}

/// Installed and running? Works without elevation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    NotInstalled,
    Stopped,
    Running,
    Other,
}

pub fn service_state() -> ServiceState {
    let Ok(m) = manager(SC_MANAGER_CONNECT) else {
        return ServiceState::NotInstalled;
    };
    let Ok(s) = open_service(&m, SERVICE_QUERY_STATUS) else {
        return ServiceState::NotInstalled;
    };
    match state(&s) {
        Ok(SERVICE_RUNNING) => ServiceState::Running,
        Ok(SERVICE_STOPPED) => ServiceState::Stopped,
        _ => ServiceState::Other,
    }
}

/// Where the installed copy lives: `%ProgramFiles%\fx\fx-indexd.exe`.
pub fn installed_exe() -> io::Result<PathBuf> {
    let base = std::env::var_os("ProgramW6432")
        .or_else(|| std::env::var_os("ProgramFiles"))
        .ok_or_else(|| io::Error::other("ProgramFiles is not set"))?;
    Ok(PathBuf::from(base).join("fx").join("fx-indexd.exe"))
}

/// Installs (or reinstalls) and starts the service. Needs Administrator.
/// `exe` is this executable; `user_sid` the only user the pipe will serve.
pub fn install(exe: &Path, user_sid: &str) -> io::Result<PathBuf> {
    if !user_sid.starts_with("S-1-")
        || !user_sid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad SID"));
    }
    if service_state() != ServiceState::NotInstalled {
        uninstall()?;
    }
    let target = installed_exe()?;
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::copy(exe, &target)?;
    let command = format!("\"{}\" service --user {user_sid}", target.display());
    let m = manager(SC_MANAGER_ALL_ACCESS)?;
    // SAFETY: valid manager handle; every string is NUL-terminated and alive
    // for the call (HSTRING); no group, tag, dependencies or account
    // (LocalSystem).
    let service = unsafe {
        CreateServiceW(
            m.0,
            &HSTRING::from(SERVICE_NAME),
            &HSTRING::from(DISPLAY_NAME),
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            &HSTRING::from(command),
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
        )
    }
    .map(Sc)
    .map_err(io::Error::from)?;
    let mut description: Vec<u16> = DESCRIPTION.encode_utf16().chain([0]).collect();
    let info = SERVICE_DESCRIPTIONW {
        lpDescription: PWSTR(description.as_mut_ptr()),
    };
    let delayed = SERVICE_DELAYED_AUTO_START_INFO {
        fDelayedAutostart: true.into(),
    };
    // SAFETY: valid service handle; each info struct (and the description
    // buffer it points to) is alive for its call. Failures only lose the
    // description or the boot delay.
    unsafe {
        let _ = ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_DESCRIPTION,
            Some((&raw const info).cast()),
        );
        let _ = ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            Some((&raw const delayed).cast()),
        );
    }
    // SAFETY: valid service handle, no arguments.
    unsafe { StartServiceW(service.0, None) }.map_err(io::Error::from)?;
    Ok(target)
}

/// Stops and deletes the service, then deletes its executable and folder.
/// Needs Administrator. Not installed: nothing to do.
pub fn uninstall() -> io::Result<()> {
    let m = manager(SC_MANAGER_ALL_ACCESS)?;
    match open_service(&m, SERVICE_ALL_ACCESS) {
        Ok(s) => {
            let mut status = SERVICE_STATUS::default();
            // SAFETY: valid service handle; `status` is writable. Fails if it
            // is already stopped, which is fine.
            let _ = unsafe { ControlService(s.0, SERVICE_CONTROL_STOP, &mut status) };
            let start = Instant::now();
            while state(&s).is_ok_and(|st| st != SERVICE_STOPPED) && start.elapsed() < STOP_TIMEOUT
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            // SAFETY: valid service handle.
            unsafe { DeleteService(s.0) }.map_err(io::Error::from)?;
        }
        Err(e) if e.raw_os_error() == Some(1060) => {} // ERROR_SERVICE_DOES_NOT_EXIST
        Err(e) => return Err(e),
    }
    let exe = installed_exe()?;
    // The process may take a moment to exit and release its file.
    let start = Instant::now();
    while exe.exists() && start.elapsed() < STOP_TIMEOUT {
        if std::fs::remove_file(&exe).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if exe.exists() {
        return Err(io::Error::other(format!(
            "could not delete {}",
            exe.display()
        )));
    }
    if let Some(dir) = exe.parent() {
        let _ = std::fs::remove_dir(dir); // only if empty
    }
    Ok(())
}

// ---- Running under the Service Control Manager ----

static BODY: OnceLock<Box<dyn Fn(&'static AtomicBool) + Send + Sync>> = OnceLock::new();
static STOP: AtomicBool = AtomicBool::new(false);
static STATUS: AtomicIsize = AtomicIsize::new(0);
static ON_STOP: OnceLock<fn()> = OnceLock::new();

fn report(state: SERVICE_STATUS_CURRENT_STATE) {
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        },
        dwWaitHint: 10_000,
        ..Default::default()
    };
    let handle = SERVICE_STATUS_HANDLE(STATUS.load(Ordering::SeqCst) as *mut _);
    // SAFETY: `handle` came from RegisterServiceCtrlHandlerExW; `status` is
    // a valid struct for the call.
    let _ = unsafe { SetServiceStatus(handle, &status) };
}

unsafe extern "system" fn control(
    code: u32,
    _event: u32,
    _data: *mut std::ffi::c_void,
    _context: *mut std::ffi::c_void,
) -> u32 {
    if code == SERVICE_CONTROL_STOP || code == SERVICE_CONTROL_SHUTDOWN {
        STOP.store(true, Ordering::SeqCst);
        report(SERVICE_STOP_PENDING);
        if let Some(wake) = ON_STOP.get() {
            wake();
        }
    }
    0 // NO_ERROR
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut PWSTR) {
    // SAFETY: NUL-terminated name; `control` matches LPHANDLER_FUNCTION_EX.
    let Ok(handle) = (unsafe {
        RegisterServiceCtrlHandlerExW(&HSTRING::from(SERVICE_NAME), Some(control), None)
    }) else {
        return;
    };
    STATUS.store(handle.0 as isize, Ordering::SeqCst);
    report(SERVICE_START_PENDING);
    report(SERVICE_RUNNING);
    if let Some(body) = BODY.get() {
        body(&STOP);
    }
    report(SERVICE_STOPPED);
}

/// Runs `body` as the service: returns after the Service Control Manager
/// stops it. `body` must return once its stop flag is set; `on_stop` is
/// called when the stop request arrives (to unblock waits).
pub fn run_service(
    body: impl Fn(&'static AtomicBool) + Send + Sync + 'static,
    on_stop: fn(),
) -> io::Result<()> {
    let _ = BODY.set(Box::new(body));
    let _ = ON_STOP.set(on_stop);
    let mut name: Vec<u16> = SERVICE_NAME.encode_utf16().chain([0]).collect();
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: PWSTR(name.as_mut_ptr()),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW::default(),
    ];
    // SAFETY: the table (null-terminated) and the name it points to live
    // until the dispatcher returns, after the service stopped.
    unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) }.map_err(io::Error::from)
}
