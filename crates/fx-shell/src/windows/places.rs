//! Windows implementation of `crate::places` (task T1.5). Every function here
//! can block (a network drive can take seconds to answer), so callers run them
//! on background threads.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures,
    FOLDERID_Profile, FOLDERID_Videos, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows::core::{GUID, PCWSTR};

use crate::places::{Drive, DriveKind, KnownFolder};

// GetDriveTypeW results (Win32_System_WindowsProgramming, not enabled here).
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;

/// How often the drive watcher looks at the drive letters (≤ 1 s to notice).
const POLL: Duration = Duration::from_millis(250);
/// Free space is refreshed this often even when no drive comes or goes.
const REFRESH: Duration = Duration::from_secs(30);

fn folder_id(f: KnownFolder) -> GUID {
    match f {
        KnownFolder::Home => FOLDERID_Profile,
        KnownFolder::Desktop => FOLDERID_Desktop,
        KnownFolder::Downloads => FOLDERID_Downloads,
        KnownFolder::Documents => FOLDERID_Documents,
        KnownFolder::Pictures => FOLDERID_Pictures,
        KnownFolder::Music => FOLDERID_Music,
        KnownFolder::Videos => FOLDERID_Videos,
    }
}

/// Path of a known folder (follows redirection, e.g. Documents in OneDrive).
pub fn known_folder(f: KnownFolder) -> Option<PathBuf> {
    let id = folder_id(f);
    // SAFETY: `id` is a valid GUID that outlives the call; no token (current user).
    let p = unsafe { SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None) }.ok()?;
    // SAFETY: on success `p` is a NUL-terminated string allocated by the shell.
    let wide = unsafe { p.as_wide() }.to_vec();
    // SAFETY: the shell allocated `p` with CoTaskMemAlloc; we free it once.
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    Some(PathBuf::from(OsString::from_wide(&wide)))
}

/// Every known folder that exists on this system.
pub fn known_folders() -> Vec<(KnownFolder, PathBuf)> {
    KnownFolder::ALL
        .iter()
        .filter_map(|&f| known_folder(f).map(|p| (f, p)))
        .collect()
}

/// Every drive letter in use, with label, kind and space.
pub fn drives() -> Vec<Drive> {
    // SAFETY: no arguments; returns a bitmask of drive letters.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|b| mask & (1 << b) != 0)
        .map(|b| drive(char::from(b'A' + b)))
        .collect()
}

fn drive(letter: char) -> Drive {
    let root = format!("{letter}:\\");
    let wide: Vec<u16> = std::ffi::OsStr::new(&root)
        .encode_wide()
        .chain([0])
        .collect();
    let name = PCWSTR(wide.as_ptr());
    // SAFETY: `name` points to a NUL-terminated string alive for the call.
    let kind = match unsafe { GetDriveTypeW(name) } {
        DRIVE_FIXED => DriveKind::Fixed,
        DRIVE_REMOVABLE => DriveKind::Removable,
        DRIVE_REMOTE => DriveKind::Network,
        DRIVE_CDROM => DriveKind::Optical,
        _ => DriveKind::Other,
    };
    let mut label = [0u16; 261];
    // SAFETY: `name` is NUL-terminated and `label` a writable buffer, both alive
    // for the call; the other outputs are not requested.
    let label =
        match unsafe { GetVolumeInformationW(name, Some(&mut label), None, None, None, None) } {
            Ok(()) => {
                let end = label.iter().position(|&c| c == 0).unwrap_or(label.len());
                String::from_utf16_lossy(&label[..end])
            }
            Err(_) => String::new(),
        };
    let (mut free, mut total) = (0u64, 0u64);
    // SAFETY: `name` is NUL-terminated; `free` and `total` are writable u64s.
    let space = unsafe { GetDiskFreeSpaceExW(name, Some(&mut free), Some(&mut total), None) }
        .ok()
        .map(|()| (free, total));
    Drive {
        root: PathBuf::from(root),
        label,
        kind,
        space,
    }
}

/// Sends the drive list now, again within ~250 ms of a drive letter appearing
/// or disappearing (USB stick, network mapping, `subst`), and every 30 s for
/// free space. Runs on its own thread until the receiver is dropped.
pub fn watch_drives(tx: Sender<Vec<Drive>>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("fx-drives".into())
        .spawn(move || {
            let mut last_mask = None;
            let mut last_send = Instant::now();
            loop {
                // SAFETY: no arguments; returns a bitmask of drive letters.
                let mask = unsafe { GetLogicalDrives() };
                if last_mask != Some(mask) || last_send.elapsed() >= REFRESH {
                    if tx.send(drives()).is_err() {
                        return;
                    }
                    last_mask = Some(mask);
                    last_send = Instant::now();
                }
                std::thread::sleep(POLL);
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn system_drive_and_profile_are_found() {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let all = drives();
        let c = all
            .iter()
            .find(|d| d.root.to_string_lossy().starts_with(&system))
            .expect("system drive listed");
        assert_eq!(c.kind, DriveKind::Fixed);
        let (free, total) = c.space.expect("system drive space");
        assert!(total > 0 && free <= total);
        let home = known_folder(KnownFolder::Home).expect("profile folder");
        assert!(home.is_dir());
        assert!(known_folders().len() >= 2);
    }

    #[test]
    fn watcher_sends_the_list_right_away() {
        let (tx, rx) = channel();
        watch_drives(tx).unwrap();
        let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!first.is_empty());
    }
}
