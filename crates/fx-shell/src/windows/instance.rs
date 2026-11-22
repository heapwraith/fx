//! Single instance (D-013, task T1.2): the first fx process of a session owns a
//! named pipe; later ones send it the folder to open and exit, so a new window
//! costs ~25 ms instead of a new process (~400 ms).
//!
//! Pipe: `\\.\pipe\fx-instance-<session id>-<user SID>`, one instance,
//! message mode, local clients only, usable only by this user (explicit DACL).
//! The second process checks the pipe's server runs as the same user before
//! handing it a folder, so another user's process that took the name first
//! gets nothing (review, D-066). Message: the absolute path in UTF-16 LE.

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_WRITE, HANDLE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE,
    FlushFileBuffers, OPEN_EXISTING, PIPE_ACCESS_INBOUND, ReadFile, WriteFile,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId,
    PIPE_READMODE_MESSAGE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE, PIPE_WAIT,
    WaitNamedPipeW,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
use windows::core::PCWSTR;

use super::to_io;
use super::user::{Descriptor, current_user_sid, process_user_sid};

/// Longest path Windows accepts (32,767 UTF-16 units), in bytes.
const MAX_MESSAGE: usize = 32_767 * 2;
/// How long a second process waits for a busy pipe, in ms.
const BUSY_WAIT_MS: u32 = 2_000;

/// Result of [`claim_instance`].
#[derive(Debug)]
pub enum Instance {
    /// This is the first process: folders sent by later processes arrive here.
    Primary(Receiver<PathBuf>),
    /// Another process is running and received the folder; this one should exit.
    Forwarded,
}

/// Becomes the single fx instance of this session, or hands `folder` to the
/// one already running.
pub fn claim_instance(folder: &Path) -> io::Result<Instance> {
    claim(&pipe_name()?, folder)
}

fn pipe_name() -> io::Result<Vec<u16>> {
    let mut session = 0u32;
    // SAFETY: `session` is a valid, writable u32 on this stack frame.
    unsafe { ProcessIdToSessionId(std::process::id(), &mut session) }.map_err(to_io)?;
    let sid = current_user_sid()?;
    Ok(wide_z(&format!(r"\\.\pipe\fx-instance-{session}-{sid}")))
}

fn wide_z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn claim(name: &[u16], folder: &Path) -> io::Result<Instance> {
    match create_first(name) {
        Ok(pipe) => {
            let (tx, rx) = channel();
            std::thread::Builder::new()
                .name("fx-instance".into())
                .spawn(move || serve(&pipe, &tx))?;
            Ok(Instance::Primary(rx))
        }
        // The pipe exists: ACCESS_DENIED from FILE_FLAG_FIRST_PIPE_INSTANCE, or
        // PIPE_BUSY because its only instance is taken.
        Err(e)
            if [ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY]
                .iter()
                .any(|c| e.raw_os_error() == Some(c.0 as i32)) =>
        {
            send(name, folder)?;
            Ok(Instance::Forwarded)
        }
        Err(e) => Err(e),
    }
}

/// An owned kernel handle.
struct Pipe(HANDLE);

// SAFETY: a kernel handle is just a number valid in the whole process; pipe
// operations on it are thread-safe. Only one thread uses each `Pipe`.
unsafe impl Send for Pipe {}

impl Drop for Pipe {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a handle this struct owns and nobody else closes.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Creates the pipe; fails with `ERROR_ACCESS_DENIED` or `ERROR_PIPE_BUSY` if
/// it already exists.
fn create_first(name: &[u16]) -> io::Result<Pipe> {
    // Only this user, nothing inherited.
    let descriptor = Descriptor::from_sddl(&format!("D:P(A;;GA;;;{})", current_user_sid()?))?;
    let attributes = descriptor.attributes();
    // SAFETY: `name` is a NUL-terminated UTF-16 string and `attributes` (with
    // the descriptor it points at) outlives the call.
    let h = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            0,
            MAX_MESSAGE as u32,
            0,
            Some(&attributes),
        )
    };
    if h.is_invalid() {
        return Err(io::Error::last_os_error());
    }
    Ok(Pipe(h))
}

/// Server loop (own thread): accepts one client at a time and forwards each
/// valid path. Ends when the receiver is dropped or the pipe breaks.
fn serve(pipe: &Pipe, tx: &Sender<PathBuf>) {
    let mut buf = vec![0u8; MAX_MESSAGE + 2];
    loop {
        // SAFETY: `pipe.0` is a valid pipe handle; synchronous (no OVERLAPPED).
        let connected = unsafe { ConnectNamedPipe(pipe.0, None) };
        if let Err(e) = connected
            && e.code() != ERROR_PIPE_CONNECTED.to_hresult()
        {
            return;
        }
        let mut read = 0u32;
        // SAFETY: `buf` is a writable buffer and `read` a writable u32, both
        // alive for the whole synchronous call.
        let ok = unsafe { ReadFile(pipe.0, Some(&mut buf), Some(&mut read), None) };
        let path = ok.ok().and_then(|()| decode(&buf[..read as usize]));
        // SAFETY: `pipe.0` is a valid pipe handle.
        let _ = unsafe { DisconnectNamedPipe(pipe.0) };
        if let Some(path) = path
            && tx.send(path).is_err()
        {
            return;
        }
    }
}

/// Second process: connects, lets the first one take the foreground (it
/// will open a window) and writes the absolute path.
fn send(name: &[u16], folder: &Path) -> io::Result<()> {
    let message = encode(&std::path::absolute(folder)?);
    let pipe = connect(name)?;
    let mut server = 0u32;
    // SAFETY: `pipe.0` is a valid pipe handle and `server` a writable u32.
    unsafe { GetNamedPipeServerProcessId(pipe.0, &mut server) }.map_err(to_io)?;
    // Only an fx of this same user gets the folder.
    if process_user_sid(server)? != current_user_sid()? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the instance pipe belongs to another user",
        ));
    }
    // SAFETY: plain call with a process id; failure only means the new
    // window may open behind the current one.
    let _ = unsafe { AllowSetForegroundWindow(server) };
    let mut written = 0u32;
    // SAFETY: `message` and `written` outlive the synchronous call.
    unsafe { WriteFile(pipe.0, Some(&message), Some(&mut written), None) }.map_err(to_io)?;
    if written as usize != message.len() {
        return Err(io::Error::new(io::ErrorKind::WriteZero, "short pipe write"));
    }
    // Waits until the server has read the message.
    // SAFETY: `pipe.0` is a valid pipe handle.
    let _ = unsafe { FlushFileBuffers(pipe.0) };
    Ok(())
}

fn connect(name: &[u16]) -> io::Result<Pipe> {
    let mut attempts = 0;
    loop {
        // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call.
        let opened = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        };
        match opened {
            Ok(h) => return Ok(Pipe(h)),
            Err(e) if e.code() == ERROR_PIPE_BUSY.to_hresult() && attempts < 3 => {
                attempts += 1;
                // SAFETY: `name` is a NUL-terminated UTF-16 string.
                let _ = unsafe { WaitNamedPipeW(PCWSTR(name.as_ptr()), BUSY_WAIT_MS) };
            }
            Err(e) => return Err(to_io(e)),
        }
    }
}

fn encode(path: &Path) -> Vec<u8> {
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// A message is accepted only if it is a whole, NUL-free, absolute path.
fn decode(bytes: &[u8]) -> Option<PathBuf> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(2) || bytes.len() > MAX_MESSAGE {
        return None;
    }
    let (pairs, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|&c| u16::from_le_bytes(c)).collect();
    if units.contains(&0) {
        return None;
    }
    let path = PathBuf::from(OsString::from_wide(&units));
    path.is_absolute().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn messages_round_trip_and_garbage_is_rejected() {
        let p = Path::new(r"C:\Pastas\relatório 日本");
        assert_eq!(decode(&encode(p)).as_deref(), Some(p));
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(b"C"), None, "odd length");
        assert_eq!(decode(&encode(Path::new("relative"))), None);
        assert_eq!(decode(b"C\0\0\0"), None, "embedded NUL");
    }

    #[test]
    fn second_claim_forwards_the_folder_to_the_first() {
        let name = wide_z(&format!(
            r"\\.\pipe\fx-instance-test-{}",
            std::process::id()
        ));
        let Instance::Primary(rx) = claim(&name, Path::new(r"C:\a")).unwrap() else {
            panic!("first claim must be primary");
        };
        for folder in [r"C:\one", r"D:\two\três"] {
            let second = claim(&name, Path::new(folder)).unwrap();
            assert!(matches!(second, Instance::Forwarded));
            let got = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(got, Path::new(folder));
        }
    }
}
