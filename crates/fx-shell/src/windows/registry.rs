//! Small helpers over the registry, under `HKCU` only (shared by the
//! default file manager, T3.1, and the installer, T4.3). Paths are relative
//! to `HKCU`, so tests can work under `Software\fx-test-*`.

use std::io;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
    RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegGetValueW,
    RegOpenKeyExW, RegSetValueExW,
};
use windows::core::{HSTRING, PCWSTR};

pub(crate) fn check(e: WIN32_ERROR) -> io::Result<()> {
    if e.is_ok() {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(e.0 as i32))
    }
}

pub(crate) struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: we own the handle and close it once.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

pub(crate) fn create(path: &str) -> io::Result<Key> {
    let mut key = HKEY::default();
    // SAFETY: NUL-terminated path (HSTRING); `key` receives a handle we close.
    check(unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE | KEY_READ,
            None,
            &mut key,
            None,
        )
    })?;
    Ok(Key(key))
}

pub(crate) fn exists(path: &str) -> bool {
    let mut key = HKEY::default();
    // SAFETY: NUL-terminated path; the handle is closed by `Key`.
    let ok = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            None,
            KEY_READ,
            &mut key,
        )
    }
    .is_ok();
    if ok {
        drop(Key(key));
    }
    ok
}

pub(crate) fn get(path: &str, name: &str) -> Option<String> {
    let mut buf = vec![0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: NUL-terminated strings; `buf` is writable for `size` bytes.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            &HSTRING::from(name),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if r.is_err() {
        return None;
    }
    let units = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..units]))
}

pub(crate) fn set(path: &str, name: &str, value: &str) -> io::Result<()> {
    let key = create(path)?;
    let data: Vec<u8> = value
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    // SAFETY: valid key; `data` is a NUL-terminated UTF-16 string.
    check(unsafe { RegSetValueExW(key.0, &HSTRING::from(name), None, REG_SZ, Some(&data)) })
}

pub(crate) fn delete_value(path: &str, name: &str) -> io::Result<()> {
    let key = create(path)?;
    // SAFETY: valid key; NUL-terminated name.
    let r = unsafe { RegDeleteValueW(key.0, &HSTRING::from(name)) };
    if r == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        check(r)
    }
}

pub(crate) fn delete_tree(path: &str) -> io::Result<()> {
    // SAFETY: NUL-terminated path under HKCU.
    let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(path)) };
    if r == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        check(r)
    }
}

/// A DWORD value.
pub(crate) fn set_dword(path: &str, name: &str, value: u32) -> io::Result<()> {
    let key = create(path)?;
    let data = value.to_le_bytes();
    // SAFETY: valid key; 4 bytes of DWORD data.
    check(unsafe { RegSetValueExW(key.0, &HSTRING::from(name), None, REG_DWORD, Some(&data)) })
}
