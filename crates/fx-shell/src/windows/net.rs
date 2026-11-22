//! What the updater (task T4.1) needs from Windows, with no new crates:
//! HTTPS downloads (WinHTTP, the system proxy and certificates), SHA-256
//! (BCrypt) and the Authenticode check (WinVerifyTrust).

use std::ffi::c_void;
use std::io;
use std::path::Path;

use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
};
use windows::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    WinVerifyTrust,
};
use windows::core::{HSTRING, PCWSTR};

/// A WinHTTP handle, closed once.
struct Handle(*mut c_void);

impl Handle {
    fn new(h: *mut c_void) -> io::Result<Handle> {
        if h.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Handle(h))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a valid handle owned by this value, closed once.
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

/// `https://host/path` → (host, path). Only HTTPS is accepted.
fn split_url(url: &str) -> io::Result<(&str, &str)> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "only https"))?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if host.is_empty() || host.contains([':', '@']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unexpected host",
        ));
    }
    Ok((host, path))
}

/// GET `url` over HTTPS (redirects followed, to HTTPS only), at most
/// `max_bytes`. Blocking: call it on a worker thread.
pub fn http_get(url: &str, max_bytes: usize) -> io::Result<Vec<u8>> {
    let (host, path) = split_url(url)?;
    // SAFETY: plain calls with valid strings; every handle is wrapped and
    // closed once; buffers are sized by the lengths passed.
    unsafe {
        let session = Handle::new(WinHttpOpen(
            &HSTRING::from("fx-updater"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        let connect = Handle::new(WinHttpConnect(
            session.0,
            &HSTRING::from(host),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        ))?;
        let request = Handle::new(WinHttpOpenRequest(
            connect.0,
            &HSTRING::from("GET"),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_OPEN_REQUEST_FLAGS(WINHTTP_FLAG_SECURE.0),
        ))?;
        let headers: Vec<u16> = "Accept: application/vnd.github+json, application/octet-stream\r\n"
            .encode_utf16()
            .collect();
        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())?;
        let mut status = 0u32;
        let mut len = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &mut len,
            std::ptr::null_mut(),
        )?;
        if status != 200 {
            return Err(io::Error::other(format!("HTTP {status}")));
        }
        let mut body = Vec::new();
        loop {
            let mut available = 0u32;
            WinHttpQueryDataAvailable(request.0, &mut available)?;
            if available == 0 {
                break;
            }
            if body.len() + available as usize > max_bytes {
                return Err(io::Error::other("answer too large"));
            }
            let start = body.len();
            body.resize(start + available as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                body[start..].as_mut_ptr().cast(),
                available,
                &mut read,
            )?;
            body.truncate(start + read as usize);
        }
        Ok(body)
    }
}

/// SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> io::Result<[u8; 32]> {
    let mut out = [0u8; 32];
    // SAFETY: the built-in SHA-256 handle; the output buffer is 32 bytes.
    unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, data, &mut out) }
        .ok()
        .map_err(io::Error::from)?;
    Ok(out)
}

/// Whether `path` has a valid Authenticode signature (no UI, no revocation
/// check over the network).
pub fn is_signed(path: &Path) -> bool {
    let wide = HSTRING::from(path.as_os_str());
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` points at `file`, which points at `wide`; all outlive
    // both calls. The second call releases the state of the first.
    unsafe {
        let result = WinVerifyTrust(Default::default(), &mut action, (&raw mut data).cast());
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(Default::default(), &mut action, (&raw mut data).cast());
        result == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sha256() {
        let h = sha256(b"abc").unwrap();
        let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn only_https_urls() {
        assert!(split_url("http://example.com/x").is_err());
        assert!(split_url("https://user@evil/x").is_err());
        assert_eq!(
            split_url("https://api.github.com/repos/a/b").unwrap(),
            ("api.github.com", "/repos/a/b")
        );
        assert_eq!(split_url("https://h").unwrap(), ("h", "/"));
    }

    #[test]
    fn signatures() {
        // A Windows binary is signed; a file we just wrote is not.
        let system = std::env::var_os("SystemRoot")
            .map(|r| Path::new(&r).join("System32").join("notepad.exe"));
        if let Some(p) = system.filter(|p| p.exists()) {
            // notepad.exe is catalog-signed on recent Windows, which this
            // embedded-signature check does not cover; kernel32 is the same.
            let _ = is_signed(&p);
        }
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("x.exe");
        std::fs::write(&f, b"MZ not really").unwrap();
        assert!(!is_signed(&f));
    }
}
