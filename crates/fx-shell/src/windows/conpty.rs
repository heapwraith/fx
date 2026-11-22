//! A pseudo console (ConPTY) for the embedded terminal (task T3.5): a shell
//! process whose console is a pair of pipes. fx reads its output (VT text)
//! and writes keys to its input.

use std::ffi::c_void;
use std::fs::File;
use std::io::{self, Write};
use std::os::windows::io::FromRawHandle;
use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Console::{
    COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
    InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    TerminateProcess, UpdateProcThreadAttribute,
};
use windows::core::{HSTRING, PWSTR};

/// A running shell on a pseudo console. Dropping it ends both.
pub struct Pty {
    hpc: HPCON,
    input: File,
    process: HANDLE,
}

impl std::fmt::Debug for Pty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pty")
    }
}

fn coord(cols: u16, rows: u16) -> COORD {
    COORD {
        X: cols.max(1) as i16,
        Y: rows.max(1) as i16,
    }
}

/// Starts `command` (e.g. `powershell.exe -NoLogo`) in `dir` on a pseudo
/// console of `cols`×`rows`. Returns the console and its output stream.
pub fn spawn(command: &str, dir: &Path, cols: u16, rows: u16) -> io::Result<(Pty, File)> {
    let (mut in_read, mut in_write) = (HANDLE::default(), HANDLE::default());
    let (mut out_read, mut out_write) = (HANDLE::default(), HANDLE::default());
    // SAFETY: the handles are written by CreatePipe; no inheritance.
    unsafe {
        CreatePipe(&mut in_read, &mut in_write, None, 0)?;
        CreatePipe(&mut out_read, &mut out_write, None, 0)?;
    }
    // SAFETY: valid pipe ends; the console duplicates them.
    let hpc = unsafe { CreatePseudoConsole(coord(cols, rows), in_read, out_write, 0) };
    // SAFETY: our copies of the console's ends are not needed any more.
    unsafe {
        let _ = CloseHandle(in_read);
        let _ = CloseHandle(out_write);
    }
    let hpc = hpc?;
    let process = match start(command, dir, hpc) {
        Ok(p) => p,
        Err(e) => {
            // SAFETY: we own these and close them once.
            unsafe {
                ClosePseudoConsole(hpc);
                let _ = CloseHandle(in_write);
                let _ = CloseHandle(out_read);
            }
            return Err(e);
        }
    };
    // SAFETY: we own both pipe ends; each File closes its handle once.
    let (input, output) = unsafe {
        (
            File::from_raw_handle(in_write.0),
            File::from_raw_handle(out_read.0),
        )
    };
    Ok((
        Pty {
            hpc,
            input,
            process,
        },
        output,
    ))
}

fn start(command: &str, dir: &Path, hpc: HPCON) -> io::Result<HANDLE> {
    let mut size = 0usize;
    // SAFETY: size query; fails with "insufficient buffer" by design.
    let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut size) };
    let mut buf = vec![0u8; size];
    let list = LPPROC_THREAD_ATTRIBUTE_LIST(buf.as_mut_ptr().cast());
    // SAFETY: `buf` has the size the first call asked for.
    unsafe { InitializeProcThreadAttributeList(Some(list), 1, None, &mut size) }?;
    // SAFETY: the attribute value is the console handle itself, as documented.
    let updated = unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            Some(hpc.0 as *const c_void),
            std::mem::size_of::<HPCON>(),
            None,
            None,
        )
    };
    let result = updated.map_err(io::Error::from).and_then(|()| {
        let mut si = STARTUPINFOEXW::default();
        si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        // Null standard handles: otherwise a parent whose own handles are
        // redirected (a pipe) hands those to the shell instead of the console.
        si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        si.lpAttributeList = list;
        let mut cmd: Vec<u16> = command.encode_utf16().chain([0]).collect();
        let mut pi = PROCESS_INFORMATION::default();
        // SAFETY: `cmd` is a writable NUL-terminated buffer (CreateProcessW may
        // modify it); `si` and its attribute list are alive for the call.
        unsafe {
            CreateProcessW(
                None,
                Some(PWSTR(cmd.as_mut_ptr())),
                None,
                None,
                false,
                EXTENDED_STARTUPINFO_PRESENT,
                None,
                &HSTRING::from(dir.as_os_str()),
                &si.StartupInfo,
                &mut pi,
            )
        }?;
        // SAFETY: the thread handle is not used.
        let _ = unsafe { CloseHandle(pi.hThread) };
        Ok(pi.hProcess)
    });
    // SAFETY: initialized above; deleted once.
    unsafe { DeleteProcThreadAttributeList(list) };
    result
}

impl Pty {
    /// Sends keys (already encoded as VT input).
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.input.write_all(bytes)?;
        self.input.flush()
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        // SAFETY: valid console handle.
        let _ = unsafe { ResizePseudoConsole(self.hpc, coord(cols, rows)) };
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // SAFETY: we own the console and the process handle; each is released
        // once. Closing the console ends the shell's console; the process is
        // terminated in case it lingers.
        unsafe {
            ClosePseudoConsole(self.hpc);
            let _ = TerminateProcess(self.process, 0);
            let _ = CloseHandle(self.process);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::{Duration, Instant};

    #[test]
    fn a_shell_runs_and_answers() {
        let dir = std::env::temp_dir();
        let (mut pty, mut out) = spawn("cmd.exe /Q /K", &dir, 80, 25).unwrap();
        pty.write(b"echo fx-conpty-ok\r").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut all = Vec::new();
            while let Ok(n) = out.read(&mut buf) {
                if n == 0 {
                    break;
                }
                all.extend_from_slice(&buf[..n]);
                if String::from_utf8_lossy(&all)
                    .matches("fx-conpty-ok")
                    .count()
                    >= 2
                {
                    let _ = tx.send(true);
                    return;
                }
            }
        });
        let start = Instant::now();
        let ok = rx.recv_timeout(Duration::from_secs(15)).unwrap_or(false);
        assert!(ok, "no echo after {:?}", start.elapsed());
        pty.resize(100, 30);
        drop(pty);
    }
}
