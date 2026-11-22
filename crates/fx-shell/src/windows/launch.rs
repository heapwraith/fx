//! Windows launching (task T1.9): `ShellExecuteExW` on a COM STA thread (the
//! thread `crate::launch::spawn_launch` creates for each launch).

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::mpsc::Receiver;

use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, SHOP_FILEPATH, SHObjectProperties,
    ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow, SW_SHOWNORMAL};
use windows::core::PCWSTR;

use super::to_io;
use crate::launch::{Launch, spawn_launch};

fn wide_z(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

/// Starts `what` without waiting; the result arrives later.
pub fn launch(what: Launch) -> Receiver<io::Result<()>> {
    spawn_launch(move || {
        // SAFETY: first COM call of this fresh thread; it ends right after.
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // The app we start should be able to come to the front.
        // SAFETY: plain call; failure only affects window activation.
        let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
        match what {
            Launch::Open(path) => shell_execute(None, path.as_os_str(), None, None, 0),
            Launch::OpenWith(path) => {
                shell_execute(Some("openas"), path.as_os_str(), None, None, 0)
            }
            Launch::Terminal(dir) => terminal(&dir),
            Launch::Properties(items) => properties(&items),
        }
    })
}

/// The shell's Properties sheet: `SHObjectProperties` for one item (it
/// needs nothing from this short-lived thread), the multi-item sheet for
/// several (it sums sizes, like Explorer's), hosted by `super::sheets`.
fn properties(items: &[std::path::PathBuf]) -> io::Result<()> {
    match items {
        [] => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nada selecionado",
        )),
        [one] => {
            let path = wide_z(one.as_os_str());
            // SAFETY: `path` is NUL-terminated and alive for the call; no owner
            // window, default page.
            let ok = unsafe {
                SHObjectProperties(None, SHOP_FILEPATH, PCWSTR(path.as_ptr()), PCWSTR::null())
            };
            if ok.as_bool() {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        }
        many => super::sheets::multi(many.to_vec()),
    }
}

/// Windows Terminal in `dir`, or PowerShell there if Terminal is missing.
fn terminal(dir: &Path) -> io::Result<()> {
    // A trailing backslash would escape the closing quote ("C:\").
    let mut d = dir.as_os_str().to_os_string();
    if d.to_string_lossy().ends_with('\\') {
        d.push(".");
    }
    let mut args = std::ffi::OsString::from("-d \"");
    args.push(&d);
    args.push("\"");
    shell_execute(
        None,
        OsStr::new("wt.exe"),
        Some(&args),
        None,
        SEE_MASK_FLAG_NO_UI,
    )
    .or_else(|_| {
        shell_execute(
            None,
            OsStr::new("powershell.exe"),
            Some(OsStr::new("-NoExit")),
            Some(dir.as_os_str()),
            0,
        )
    })
}

fn shell_execute(
    verb: Option<&str>,
    file: &OsStr,
    params: Option<&OsStr>,
    dir: Option<&OsStr>,
    extra_mask: u32,
) -> io::Result<()> {
    let verb = verb.map(|v| wide_z(OsStr::new(v)));
    let file = wide_z(file);
    let params = params.map(wide_z);
    let dir = dir.map(wide_z);
    let ptr = |v: &Option<Vec<u16>>| v.as_ref().map_or(PCWSTR::null(), |w| PCWSTR(w.as_ptr()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        // NOASYNC: this thread ends right after; the call must finish first.
        fMask: SEE_MASK_NOASYNC | extra_mask,
        lpVerb: ptr(&verb),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: ptr(&params),
        lpDirectory: ptr(&dir),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: `info` is a properly sized SHELLEXECUTEINFOW whose string pointers
    // point into vectors alive for the whole call.
    unsafe { ShellExecuteExW(&mut info) }.map_err(to_io)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real call that fails without showing any dialog: the error comes back
    /// as an `io::Error`.
    #[test]
    fn missing_file_is_an_error_without_ui() {
        let missing = std::env::temp_dir().join(format!(
            "fx-missing-{}\\does-not-exist.fx-test",
            std::process::id()
        ));
        assert!(!missing.exists());
        let r = shell_execute(None, missing.as_os_str(), None, None, SEE_MASK_FLAG_NO_UI);
        assert!(r.is_err());
    }
}

/// Opens real Properties sheets (one item and two items), checks that each
/// window appears and closes it. Ignored: it shows windows on the desktop.
#[cfg(test)]
mod sheet_tests {
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
        WM_CLOSE,
    };
    use windows::core::BOOL;

    use crate::launch::Launch;

    /// Visible top-level windows of this process: (handle, title).
    fn own_windows() -> Vec<(HWND, String)> {
        unsafe extern "system" fn collect(h: HWND, l: LPARAM) -> BOOL {
            // SAFETY: `l` is the `Vec` passed below, alive during EnumWindows.
            let found = unsafe { &mut *(l.0 as *mut Vec<(HWND, String)>) };
            let mut pid = 0;
            // SAFETY: plain calls on a window handle given by EnumWindows.
            unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
            // SAFETY: as above.
            let visible = unsafe { IsWindowVisible(h) }.as_bool();
            if pid == std::process::id() && visible {
                let mut buf = [0u16; 256];
                // SAFETY: `buf` is a valid buffer of its length.
                let n = unsafe { GetWindowTextW(h, &mut buf) };
                found.push((h, String::from_utf16_lossy(&buf[..n.max(0) as usize])));
            }
            true.into()
        }
        let mut found: Vec<(HWND, String)> = Vec::new();
        // SAFETY: the callback only touches `found`, which outlives the call.
        let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut found as *mut _ as isize)) };
        found
    }

    fn wait_for_sheet(name: &str) -> HWND {
        let start = Instant::now();
        loop {
            if let Some((h, _)) = own_windows().into_iter().find(|(_, t)| t.contains(name)) {
                return h;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "no sheet for {name}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    #[ignore]
    fn properties_sheets_appear() {
        let windir = std::path::PathBuf::from(std::env::var("windir").unwrap());
        let one = crate::launch(Launch::Properties(vec![windir.join("notepad.exe")]));
        one.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
        let h = wait_for_sheet("notepad.exe");
        // SAFETY: plain call on a window of this process.
        unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)) }.unwrap();

        let two = vec![windir.join("notepad.exe"), windir.join("explorer.exe")];
        let rx = crate::launch(Launch::Properties(two));
        rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
        let h = wait_for_sheet("notepad.exe, ");
        // SAFETY: as above.
        unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)) }.unwrap();
    }
}
