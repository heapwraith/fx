//! Windows launching (task T1.9): `ShellExecuteExW` on a COM STA thread (the
//! thread `crate::launch::spawn_launch` creates for each launch).

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::mpsc::Receiver;

use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_INVOKEIDLIST, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
    SHMultiFileProperties, ShellExecuteExW,
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

/// The shell's Properties sheet: the "properties" verb for one item, the
/// multi-item sheet for several (it sums sizes, like Explorer's).
fn properties(items: &[std::path::PathBuf]) -> io::Result<()> {
    match items {
        [] => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nada selecionado",
        )),
        [one] => shell_execute(
            Some("properties"),
            one.as_os_str(),
            None,
            None,
            SEE_MASK_INVOKEIDLIST,
        ),
        many => {
            let data = super::dragdrop::data_object(many)?;
            // SAFETY: `data` is the shell's own data object for `many`; the
            // sheet keeps its own reference.
            unsafe { SHMultiFileProperties(&data, 0) }.map_err(to_io)
        }
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
