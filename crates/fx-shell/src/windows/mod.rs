//! Windows implementation.

mod icons;
mod instance;
mod launch;
mod places;
mod thumbs;

pub use icons::start_icon_service;
pub use instance::{Instance, claim_instance};
pub use launch::launch;
pub use places::{drives, known_folder, known_folders, watch_drives};
pub use thumbs::start_thumb_service;

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
