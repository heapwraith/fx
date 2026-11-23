//! Windows implementation.

mod a11y;
mod clipboard;
pub mod conpty;
pub mod contrast;
pub mod default_manager;
mod dragdrop;
mod elevation;
mod icons;
pub mod images;
pub mod install;
mod instance;
mod keys;
mod launch;
mod menu;
pub mod net;
mod places;
pub mod preview_host;
mod process;
mod properties;
mod registry;
mod sheets;
mod thumbs;
mod time;
mod user;

pub use a11y::{A11y, LatestTree};
pub use clipboard::{clear_clipboard, clipboard_files, set_clipboard_files};
pub use dragdrop::{DragResult, drag_files, drop_keys, window_id};
pub use elevation::is_elevated;
pub use icons::start_icon_service;
pub use instance::{Instance, claim_instance};
pub use keys::shifted_digit;
pub use launch::launch;
pub use menu::show_context_menu;
pub use places::{drives, known_folder, known_folders, watch_drives};
pub use process::run_hidden;
pub use properties::{item_properties, photo_taken, property_values};
pub use thumbs::{preview_image, start_thumb_service};
pub use time::utc_offset_at;

use std::time::{Duration, SystemTime};

use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

/// Windows build number (e.g. 26200), or `None` if Windows refuses to say.
/// Uses `RtlGetVersion`, which (unlike `GetVersionExW`) does not depend on the
/// executable's compatibility manifest.
pub fn windows_build() -> Option<u32> {
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a valid, writable OSVERSIONINFOW with its size field set,
    // as the API requires.
    unsafe { RtlGetVersion(&mut info) }.ok().ok()?;
    Some(info.dwBuildNumber)
}

/// Whether "Animation effects" is on in Windows (Accessibility > Visual
/// effects), for fx's own transitions (T4.2).
pub fn animations_enabled() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut on = windows::core::BOOL(1);
    // SAFETY: `on` is a writable BOOL, as this action documents.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut on).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_err() || on.as_bool()
}

/// Whether the Windows display language is Portuguese (any country), for
/// the interface language (T3.7).
pub fn system_language_is_portuguese() -> bool {
    // SAFETY: plain call, no arguments.
    let lang = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    lang & 0x3FF == 0x16 // LANG_PORTUGUESE
}

/// Mica (`DWMSBT_MAINWINDOW`) exists from Windows 11 22H2 (build 22621).
pub fn supports_mica() -> bool {
    windows_build().is_some_and(|b| b >= 22_621)
}

/// When the current process was created (for "time since process start"
/// measurements, docs/PERFORMANCE.md). `None` if Windows refuses to say.
pub fn process_start_time() -> Option<SystemTime> {
    let (mut creation, mut exit, mut kernel, mut user) = Default::default();
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing; the
    // four out-pointers are valid, writable FILETIMEs on this stack frame.
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    }
    .ok()?;
    Some(filetime_to_system_time(creation))
}

fn filetime_to_system_time(ft: FILETIME) -> SystemTime {
    const TICKS_1601_TO_1970: u64 = 116_444_736_000_000_000;
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    let since_1970 = ticks.saturating_sub(TICKS_1601_TO_1970);
    SystemTime::UNIX_EPOCH + Duration::from_nanos(since_1970.saturating_mul(100))
}

/// `windows::core::Error` as `io::Error`, keeping Win32 error codes.
fn to_io(e: windows::core::Error) -> std::io::Error {
    let code = e.code().0 as u32;
    if code & 0xFFFF_0000 == 0x8007_0000 {
        std::io::Error::from_raw_os_error((code & 0xFFFF) as i32)
    } else {
        std::io::Error::from_raw_os_error(code as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_build_is_plausible() {
        let build = windows_build().unwrap();
        assert!(build >= 10_240, "{build}");
        assert_eq!(supports_mica(), build >= 22_621);
    }

    #[test]
    fn process_started_recently_and_in_the_past() {
        let start = process_start_time().unwrap();
        let age = SystemTime::now().duration_since(start).unwrap();
        assert!(age < Duration::from_secs(600), "{age:?}");
    }
}
