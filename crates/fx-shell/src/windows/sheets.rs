//! Host thread for the multi-item Properties sheet (T1.21).
//!
//! `SHMultiFileProperties` shows the sheet on a shell thread, but the sheet
//! keeps calling the data object in the apartment that created it. If that
//! thread ends (like the short-lived launch threads), the sheet never
//! appears. So one STA thread lives for the whole process: it waits in
//! `GetMessageW` (no cost while idle), creates each data object and sheet,
//! and keeps pumping messages for them.

use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::SHMultiFileProperties;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    TranslateMessage, WM_APP,
};

use super::to_io;

/// Wakes the host thread: requests are waiting in its channel.
const WM_SHEET: u32 = WM_APP + 1;
/// How long a caller waits for the sheet to be created.
const CREATE_TIMEOUT: Duration = Duration::from_secs(10);

type Request = (Vec<PathBuf>, Sender<io::Result<()>>);

struct Host {
    thread_id: u32,
    requests: Sender<Request>,
}

fn host() -> io::Result<&'static Mutex<Host>> {
    static HOST: OnceLock<Mutex<Host>> = OnceLock::new();
    if let Some(h) = HOST.get() {
        return Ok(h);
    }
    let (tx, rx) = channel::<Request>();
    let (ready_tx, ready_rx) = channel();
    std::thread::Builder::new()
        .name("fx-sheets".into())
        .spawn(move || run(&rx, &ready_tx))?;
    let thread_id = ready_rx
        .recv_timeout(CREATE_TIMEOUT)
        .map_err(|_| io::Error::other("a thread das Propriedades não iniciou"))?;
    // A race with another first caller only leaves an idle thread behind.
    Ok(HOST.get_or_init(|| {
        Mutex::new(Host {
            thread_id,
            requests: tx,
        })
    }))
}

fn run(requests: &Receiver<Request>, ready: &Sender<u32>) {
    // SAFETY: first COM call of this thread, which lives as long as the process.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let mut msg = MSG::default();
    // SAFETY: creates this thread's message queue so PostThreadMessageW
    // cannot fail once the id is known; `msg` is a valid out pointer.
    let _ = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE) };
    // SAFETY: plain call.
    let _ = ready.send(unsafe { GetCurrentThreadId() });
    // SAFETY: standard message loop on this thread's own queue.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        if msg.message == WM_SHEET {
            while let Ok((items, reply)) = requests.try_recv() {
                let _ = reply.send(show(&items));
            }
            continue;
        }
        // SAFETY: `msg` came from GetMessageW on this thread.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn show(items: &[PathBuf]) -> io::Result<()> {
    let data = super::dragdrop::data_object(items)?;
    // SAFETY: `data` is the shell's data object for `items`, created on this
    // thread, whose message loop keeps serving the sheet.
    unsafe { SHMultiFileProperties(&data, 0) }.map_err(to_io)
}

/// Shows the multi-item sheet for `items`; returns once it was created.
pub(super) fn multi(items: Vec<PathBuf>) -> io::Result<()> {
    let (reply_tx, reply_rx) = channel();
    let thread_id = {
        let host = host()?
            .lock()
            .map_err(|_| io::Error::other("Propriedades indisponíveis"))?;
        host.requests
            .send((items, reply_tx))
            .map_err(|_| io::Error::other("Propriedades indisponíveis"))?;
        host.thread_id
    };
    // SAFETY: plain call; the host thread's queue exists (created before its
    // id was published).
    unsafe { PostThreadMessageW(thread_id, WM_SHEET, WPARAM(0), LPARAM(0)) }.map_err(to_io)?;
    reply_rx
        .recv_timeout(CREATE_TIMEOUT)
        .map_err(|_| io::Error::other("as Propriedades não responderam"))?
}
