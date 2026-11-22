//! The named pipe of `fx-indexd` (T2.5): the server side (in the service,
//! as SYSTEM) and the client side (in the app, unelevated).
//!
//! The pipe only accepts local clients, and its ACL lets in SYSTEM and the
//! one user who installed the service. The server creates it with
//! `FILE_FLAG_FIRST_PIPE_INSTANCE`, so it refuses to start if someone else
//! already took the name. One request per connection: read it, answer, hang
//! up.

use std::ffi::c_void;
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::ManuallyDrop;
use std::os::windows::io::FromRawHandle;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAGS_AND_ATTRIBUTES, FlushFileBuffers, PIPE_ACCESS_DUPLEX,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT, WaitNamedPipeW,
};
use windows::core::HSTRING;

use crate::protocol::{Request, Response};

/// The pipe's name.
pub const PIPE_NAME: &str = r"\\.\pipe\fx-indexd";
const BUFFER: u32 = 64 * 1024;

/// SDDL of the pipe: full access for SYSTEM, read and write for `user_sid`,
/// nobody else (protected: nothing inherited).
pub fn pipe_sddl(user_sid: &str) -> String {
    format!("D:P(A;;GA;;;SY)(A;;GRGW;;;{user_sid})")
}

/// A security descriptor built from SDDL, freed on drop.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn from_sddl(sddl: &str) -> io::Result<Self> {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `HSTRING` keeps the NUL-terminated SDDL alive for the call;
        // `sd` receives a LocalAlloc'ed descriptor that `Drop` frees.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &HSTRING::from(sddl),
                SDDL_REVISION_1,
                &mut sd,
                None,
            )
        }
        .map_err(io::Error::from)?;
        Ok(Self(sd))
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: the descriptor came from LocalAlloc (SDDL conversion) and is
        // freed once.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.0.0))) };
    }
}

/// An owned pipe handle.
struct Pipe(HANDLE);

impl Drop for Pipe {
    fn drop(&mut self) {
        // SAFETY: we own the handle and close it once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Serves requests until `stop` is set (then call [`wake`] to unblock the
/// wait for the next client). `handler` answers each request.
pub fn serve(
    sddl: &str,
    stop: &AtomicBool,
    handler: &dyn Fn(Request) -> Response,
) -> io::Result<()> {
    serve_on(PIPE_NAME, sddl, stop, handler)
}

fn serve_on(
    name: &str,
    sddl: &str,
    stop: &AtomicBool,
    handler: &dyn Fn(Request) -> Response,
) -> io::Result<()> {
    let descriptor = Descriptor::from_sddl(sddl)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0.0.cast::<c_void>(),
        bInheritHandle: false.into(),
    };
    // SAFETY: the name is NUL-terminated (HSTRING) and `attributes` (with its
    // descriptor) outlives the call.
    let handle = unsafe {
        CreateNamedPipeW(
            &HSTRING::from(name),
            FILE_FLAGS_AND_ATTRIBUTES(PIPE_ACCESS_DUPLEX.0 | FILE_FLAG_FIRST_PIPE_INSTANCE.0),
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            BUFFER,
            BUFFER,
            0,
            Some(&attributes),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let pipe = Pipe(handle);
    while !stop.load(Ordering::SeqCst) {
        // SAFETY: `pipe.0` is a valid pipe handle owned by `pipe`.
        let connected = match unsafe { ConnectNamedPipe(pipe.0, None) } {
            Ok(()) => true,
            Err(e) => e.code() == ERROR_PIPE_CONNECTED.to_hresult(),
        };
        if connected && !stop.load(Ordering::SeqCst) {
            // Reads and writes through std, without closing the handle.
            // SAFETY: the handle stays valid while `stream` lives; ManuallyDrop
            // keeps File from closing it.
            let mut stream = ManuallyDrop::new(unsafe { File::from_raw_handle(pipe.0.0) });
            let _ = answer(&mut *stream, handler);
            // SAFETY: valid handle; flush lets the client read everything
            // before the disconnect.
            let _ = unsafe { FlushFileBuffers(pipe.0) };
        }
        // SAFETY: valid handle; ends this client's connection.
        let _ = unsafe { DisconnectNamedPipe(pipe.0) };
    }
    Ok(())
}

fn answer(
    stream: &mut (impl Read + Write),
    handler: &dyn Fn(Request) -> Response,
) -> io::Result<()> {
    let request = Request::read_from(stream)?;
    handler(request).write_to(stream)
}

/// Unblocks [`serve`] waiting for a client (used when stopping).
pub fn wake() {
    wake_on(PIPE_NAME);
}

fn wake_on(name: &str) {
    let _ = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(name);
}

/// Sends `request` to the service and waits for the answer. Fails quickly
/// when the service is not running (`NotFound`).
pub fn query(request: &Request, wait_ms: u32) -> io::Result<Response> {
    query_on(PIPE_NAME, request, wait_ms)
}

fn query_on(name: &str, request: &Request, wait_ms: u32) -> io::Result<Response> {
    let mut pipe = match open(name) {
        Ok(p) => p,
        Err(e) if e.raw_os_error() == Some(231) => {
            // ERROR_PIPE_BUSY: another client is being served.
            // SAFETY: NUL-terminated name kept alive by HSTRING.
            let _ = unsafe { WaitNamedPipeW(&HSTRING::from(name), wait_ms) };
            open(name)?
        }
        Err(e) => return Err(e),
    };
    request.write_to(&mut pipe)?;
    Response::read_from(&mut pipe)
}

fn open(name: &str) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    // SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION: a server pretending to
    // be fx-indexd cannot act as this user, only learn who it is.
    const SQOS: u32 = 0x0010_0000 | (1 << 16);
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .security_qos_flags(SQOS)
        .open(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{RemoteHit, Status};
    use std::sync::Arc;

    /// A real pipe in this process, with the ACL the service uses (this user).
    #[test]
    fn serves_requests_over_the_pipe_and_stops() {
        let name = format!(r"\\.\pipe\fx-indexd-test-{}", std::process::id());
        let sid = crate::windows::system::current_user_sid().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let server = {
            let (name, stop) = (name.clone(), stop.clone());
            std::thread::spawn(move || {
                serve_on(&name, &pipe_sddl(&sid), &stop, &|r: Request| Response {
                    status: Status::Ready,
                    items: 42,
                    hits: vec![RemoteHit {
                        path: format!(r"C:\{}", r.query).encode_utf16().collect(),
                        is_dir: false,
                        size: u64::from(r.limit),
                        modified: 0,
                    }],
                })
            })
        };
        let ask = |q: &str| {
            let request = Request {
                query: q.into(),
                limit: 3,
            };
            for _ in 0..100 {
                match query_on(&name, &request, 1000) {
                    Ok(r) => return r,
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
                }
            }
            panic!("no answer from the pipe");
        };
        for q in ["notas", "relatório"] {
            let r = ask(q);
            assert_eq!(r.items, 42);
            assert_eq!(
                String::from_utf16_lossy(&r.hits[0].path),
                format!(r"C:\{q}")
            );
            assert_eq!(r.hits[0].size, 3);
        }
        // A second server on the same name is refused (no squatting).
        let again = serve_on(
            &name,
            &pipe_sddl("S-1-5-18"),
            &AtomicBool::new(true),
            &|_| unreachable!(),
        );
        assert!(again.is_err());
        stop.store(true, Ordering::SeqCst);
        wake_on(&name);
        server.join().unwrap().unwrap();
        // Garbage instead of a request does not bring it down (checked above
        // by the protocol tests); here: nothing listens any more.
        assert!(
            query_on(
                &name,
                &Request {
                    query: "x".into(),
                    limit: 1
                },
                10
            )
            .is_err()
        );
    }

    #[test]
    fn the_acl_names_only_system_and_the_user() {
        assert_eq!(
            pipe_sddl("S-1-5-21-1-2-3-1001"),
            "D:P(A;;GA;;;SY)(A;;GRGW;;;S-1-5-21-1-2-3-1001)"
        );
        assert!(Descriptor::from_sddl(&pipe_sddl("S-1-5-21-1-2-3-1001")).is_ok());
    }
}
