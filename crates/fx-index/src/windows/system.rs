//! System facts the index service needs (T2.5): which volumes to index, who
//! the current user is, and where each user's profile lives.

use std::ffi::c_void;
use std::io;
use std::path::PathBuf;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegCloseKey,
    RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{HSTRING, PWSTR};

const DRIVE_FIXED: u32 = 3;

/// Returns free blocks of the process heap to the system: after reading a
/// volume's MFT (hundreds of MB, transient), the freed memory otherwise stays
/// committed to the service.
pub fn trim_heap() {
    use windows::Win32::System::Memory::{GetProcessHeap, HEAP_FLAGS, HeapCompact};
    // SAFETY: the process heap handle is always valid; compacting it only
    // decommits free blocks.
    if let Ok(heap) = unsafe { GetProcessHeap() } {
        // SAFETY: as above.
        let _ = unsafe { HeapCompact(heap, HEAP_FLAGS(0)) };
    }
}

/// Fixed NTFS volumes, as `"C:"`.
pub fn ntfs_volumes() -> Vec<String> {
    // SAFETY: plain call.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| format!("{}:", char::from(b'A' + i)))
        .filter(|v| {
            let root = HSTRING::from(format!(r"{v}\"));
            // SAFETY: NUL-terminated root kept alive by HSTRING.
            if unsafe { GetDriveTypeW(&root) } != DRIVE_FIXED {
                return false;
            }
            let mut fs = [0u16; 32];
            // SAFETY: `fs` is a writable buffer of its length; other outputs
            // are not requested.
            let ok = unsafe { GetVolumeInformationW(&root, None, None, None, None, Some(&mut fs)) };
            let len = fs.iter().position(|&c| c == 0).unwrap_or(fs.len());
            ok.is_ok() && String::from_utf16_lossy(&fs[..len]) == "NTFS"
        })
        .collect()
}

/// SID of the user running this process, as `S-1-5-21-...`.
pub fn current_user_sid() -> io::Result<String> {
    let mut token = HANDLE::default();
    // SAFETY: the pseudo-handle of this process is always valid; `token`
    // receives a handle closed below.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(io::Error::from)?;
    let result = token_user_sid(token);
    // SAFETY: we own the token handle and close it once.
    let _ = unsafe { CloseHandle(token) };
    result
}

fn token_user_sid(token: HANDLE) -> io::Result<String> {
    let mut needed = 0u32;
    // SAFETY: size query (no buffer); fails with "insufficient buffer" and sets
    // `needed`.
    let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut needed) };
    // u64 elements: aligned enough for TOKEN_USER.
    let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
    // SAFETY: `buf` has at least `needed` writable bytes.
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            needed,
            &mut needed,
        )
    }
    .map_err(io::Error::from)?;
    // SAFETY: the buffer now holds a TOKEN_USER (aligned, see above).
    let user = unsafe { &*buf.as_ptr().cast::<TOKEN_USER>() };
    let mut text = PWSTR::null();
    // SAFETY: the SID points into `buf`, alive for the call; `text` receives a
    // LocalAlloc'ed string freed below.
    unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) }.map_err(io::Error::from)?;
    // SAFETY: `text` is a valid NUL-terminated string from the conversion.
    let sid = unsafe { text.to_string() }.map_err(|_| io::Error::other("bad SID"));
    // SAFETY: allocated by ConvertSidToStringSidW, freed once.
    let _ = unsafe { LocalFree(Some(HLOCAL(text.0.cast()))) };
    sid
}

/// Each user's profile folder (`C:\Users\ana`), by SID, from the registry
/// (`ProfileList`). Read-only.
pub fn profiles() -> Vec<(String, PathBuf)> {
    const LIST: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList";
    let mut key = HKEY::default();
    // SAFETY: NUL-terminated subkey; `key` receives a handle closed below.
    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(LIST),
            None,
            KEY_READ,
            &mut key,
        )
    }
    .is_err()
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0.. {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        // SAFETY: `name` is a writable buffer of `len` units.
        let r = unsafe {
            RegEnumKeyExW(
                key,
                i,
                Some(PWSTR(name.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            )
        };
        if r.is_err() {
            break;
        }
        let sid = String::from_utf16_lossy(&name[..len as usize]);
        if let Some(path) = profile_path(key, &sid) {
            out.push((sid, path));
        }
    }
    // SAFETY: we own the key handle and close it once.
    let _ = unsafe { RegCloseKey(key) };
    out
}

fn profile_path(list: HKEY, sid: &str) -> Option<PathBuf> {
    let mut buf = [0u16; 1024];
    let mut size = std::mem::size_of_val(&buf) as u32;
    // SAFETY: `buf` is writable for `size` bytes; strings are NUL-terminated
    // (HSTRING). REG_EXPAND_SZ values are expanded by RegGetValueW.
    unsafe {
        RegGetValueW(
            list,
            &HSTRING::from(sid),
            &HSTRING::from("ProfileImagePath"),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    let units = (size as usize / 2).saturating_sub(1);
    use std::os::windows::ffi::OsStringExt;
    Some(PathBuf::from(std::ffi::OsString::from_wide(&buf[..units])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_machine_has_an_ntfs_system_volume_and_a_user() {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        assert!(
            ntfs_volumes().contains(&system.to_uppercase()),
            "{:?}",
            ntfs_volumes()
        );
        let sid = current_user_sid().unwrap();
        assert!(sid.starts_with("S-1-5-"), "{sid}");
        let mine = profiles().into_iter().find(|(s, _)| *s == sid);
        let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
        assert_eq!(
            mine.map(|(_, p)| p),
            home,
            "own profile found in ProfileList"
        );
    }
}
