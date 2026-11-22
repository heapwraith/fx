//! What the installer (task T4.3) needs from Windows: LZMS compression for
//! the package, shortcuts (`.lnk`), the entry in Settings › Apps and the
//! Start Menu folder. Registry paths are relative to `HKCU` (per-user
//! install, no administrator), so tests use `Software\fx-test-*`.

use std::ffi::{OsString, c_void};
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use windows::Win32::Storage::Compression::{
    COMPRESS_ALGORITHM_LZMS, COMPRESSOR_HANDLE, CloseCompressor, CloseDecompressor, Compress,
    CreateCompressor, CreateDecompressor, DECOMPRESSOR_HANDLE, Decompress,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, IPersistFile,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellLink,
};
use windows::core::{HSTRING, Interface};

use super::registry;

/// The app's entry in Settings › Apps (per user).
pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\fx";
/// The entry the old Inno Setup installer (Phase 2) left, adopted and removed.
pub const INNO_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{6E7C2B5A-3F1D-4C8E-9A2B-0F4D5E6A7B8C}_is1";

/// LZMS-compresses `data` (Windows Compression API, buffer mode).
pub fn compress(data: &[u8]) -> io::Result<Vec<u8>> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let mut h = COMPRESSOR_HANDLE::default();
    // SAFETY: `h` receives a handle closed below; buffers are sized by the
    // lengths passed; the first call only asks for the size.
    unsafe {
        CreateCompressor(COMPRESS_ALGORITHM_LZMS, None, &mut h)?;
        let mut needed = 0usize;
        let _ = Compress(
            h,
            Some(data.as_ptr().cast()),
            data.len(),
            None,
            0,
            &mut needed,
        );
        let mut out = vec![0u8; needed.max(64)];
        let mut written = 0usize;
        let r = Compress(
            h,
            Some(data.as_ptr().cast()),
            data.len(),
            Some(out.as_mut_ptr().cast()),
            out.len(),
            &mut written,
        );
        let _ = CloseCompressor(h);
        r?;
        out.truncate(written);
        Ok(out)
    }
}

/// Reverses [`compress`]; `size` is the original length.
pub fn decompress(data: &[u8], size: usize) -> io::Result<Vec<u8>> {
    if size == 0 {
        return Ok(Vec::new());
    }
    let mut h = DECOMPRESSOR_HANDLE::default();
    let mut out = vec![0u8; size];
    // SAFETY: as above; `out` has the original size.
    unsafe {
        CreateDecompressor(COMPRESS_ALGORITHM_LZMS, None, &mut h)?;
        let mut written = 0usize;
        let r = Decompress(
            h,
            Some(data.as_ptr().cast()),
            data.len(),
            Some(out.as_mut_ptr().cast::<c_void>()),
            out.len(),
            Some(&mut written),
        );
        let _ = CloseDecompressor(h);
        r?;
        if written != size {
            return Err(io::Error::other("unexpected decompressed size"));
        }
    }
    Ok(out)
}

/// The user's Start Menu "Programs" folder.
pub fn start_menu_programs() -> Option<PathBuf> {
    // SAFETY: valid GUID; the returned string is freed once below.
    let p = unsafe { SHGetKnownFolderPath(&FOLDERID_Programs, KF_FLAG_DEFAULT, None) }.ok()?;
    // SAFETY: NUL-terminated string allocated by the shell.
    let wide = unsafe { p.as_wide() }.to_vec();
    // SAFETY: allocated with CoTaskMemAlloc; freed once.
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    Some(PathBuf::from(OsString::from_wide(&wide)))
}

/// Writes a shortcut at `lnk` to `target` (working folder: the target's).
pub fn create_shortcut(lnk: &Path, target: &Path, description: &str) -> io::Result<()> {
    if let Some(dir) = lnk.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // SAFETY: COM on this thread (apartment); the interfaces are released
    // when dropped; every string outlives its call.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(target.as_os_str()))?;
        if let Some(dir) = target.parent() {
            link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
        }
        link.SetDescription(&HSTRING::from(description))?;
        link.SetIconLocation(&HSTRING::from(target.as_os_str()), 0)?;
        let file: IPersistFile = link.cast()?;
        file.Save(&HSTRING::from(lnk.as_os_str()), true)?;
    }
    Ok(())
}

/// What Settings › Apps shows and runs.
#[derive(Debug, Clone)]
pub struct UninstallEntry {
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_dir: PathBuf,
    pub icon: PathBuf,
    pub uninstall: String,
    pub quiet_uninstall: String,
    pub size_kb: u32,
    pub url: String,
}

pub fn register_uninstall(key: &str, e: &UninstallEntry) -> io::Result<()> {
    let s = |name: &str, value: &str| registry::set(key, name, value);
    s("DisplayName", &e.name)?;
    s("DisplayVersion", &e.version)?;
    s("Publisher", &e.publisher)?;
    s("InstallLocation", &e.install_dir.to_string_lossy())?;
    s("DisplayIcon", &e.icon.to_string_lossy())?;
    s("UninstallString", &e.uninstall)?;
    s("QuietUninstallString", &e.quiet_uninstall)?;
    s("URLInfoAbout", &e.url)?;
    registry::set_dword(key, "EstimatedSize", e.size_kb)?;
    registry::set_dword(key, "NoModify", 1)?;
    registry::set_dword(key, "NoRepair", 1)
}

pub fn unregister_uninstall(key: &str) -> io::Result<()> {
    registry::delete_tree(key)
}

/// The version an earlier install registered, if any.
pub fn registered_version(key: &str) -> Option<String> {
    registry::get(key, "DisplayVersion")
}

pub fn is_registered(key: &str) -> bool {
    registry::exists(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_round_trip() {
        let data: Vec<u8> = (0..200_000u32)
            .flat_map(|i| (i % 251).to_le_bytes())
            .collect();
        let c = compress(&data).unwrap();
        assert!(c.len() < data.len() / 4, "{} → {}", data.len(), c.len());
        assert_eq!(decompress(&c, data.len()).unwrap(), data);
        assert!(
            decompress(&c[..c.len() / 2], data.len()).is_err(),
            "truncated"
        );
        assert_eq!(decompress(&compress(b"").unwrap(), 0).unwrap(), b"");
    }

    #[test]
    fn uninstall_entry_under_a_test_key() {
        let key = format!(r"Software\fx-test-{}\Uninstall\fx", std::process::id());
        let e = UninstallEntry {
            name: "fx".into(),
            version: "1.2.3".into(),
            publisher: "fx".into(),
            install_dir: PathBuf::from(r"C:\x\fx"),
            icon: PathBuf::from(r"C:\x\fx\fx.exe"),
            uninstall: "u".into(),
            quiet_uninstall: "q".into(),
            size_kb: 42,
            url: "https://example.org".into(),
        };
        register_uninstall(&key, &e).unwrap();
        assert!(is_registered(&key));
        assert_eq!(registered_version(&key).as_deref(), Some("1.2.3"));
        unregister_uninstall(&key).unwrap();
        assert!(!is_registered(&key));
        let root = format!(r"Software\fx-test-{}", std::process::id());
        let _ = registry::delete_tree(&root);
    }

    #[test]
    fn shortcut_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let lnk = dir.path().join("menu").join("fx.lnk");
        let exe = std::env::current_exe().unwrap();
        create_shortcut(&lnk, &exe, "fx").unwrap();
        assert!(std::fs::metadata(&lnk).unwrap().len() > 0);
        assert!(start_menu_programs().is_some_and(|p| p.is_absolute()));
    }
}
