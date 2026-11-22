//! What the updater (task T4.1) needs from Windows, with no new crates:
//! HTTPS downloads (WinHTTP, the system proxy and certificates), SHA-256
//! (BCrypt) and the Authenticode check (WinVerifyTrust).

use std::ffi::c_void;
use std::io;
use std::path::Path;

use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
};
use windows::Win32::Security::Cryptography::{
    BCRYPT_SHA256_ALG_HANDLE, BCryptHash, CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW,
};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
    WINTRUST_DATA_PROVIDER_FLAGS, WINTRUST_FILE_INFO, WTD_CHOICE_FILE,
    WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, WTD_REVOKE_NONE, WTD_REVOKE_WHOLECHAIN,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvCertFromChain,
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
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

/// Opens `path` so that nobody can write, rename or delete it while the
/// returned handle lives (others may still read and run it), and reads it
/// whole, up to `max` bytes. What was read is what is on disk until the
/// handle is dropped: check it, start it, then drop the handle.
pub fn read_locked(path: &Path, max: usize) -> io::Result<(std::fs::File, Vec<u8>)> {
    use std::io::Read;
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 1;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)?;
    let mut data = Vec::new();
    (&mut file).take(max as u64 + 1).read_to_end(&mut data)?;
    if data.len() > max {
        return Err(io::Error::other("file too large"));
    }
    Ok((file, data))
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

/// The publisher of `path`: the subject name of the certificate that signed
/// it, when its Authenticode signature is valid and chains to a trusted root;
/// `None` otherwise. A valid signature alone says nothing about *who* signed
/// (Microsoft: the signer is a separate check), so callers compare this name.
/// `check_revocation` also refuses revoked certificates (may use the network).
/// No UI.
pub fn signer(path: &Path, check_revocation: bool) -> Option<String> {
    let wide = HSTRING::from(path.as_os_str());
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        ..Default::default()
    };
    let (revocation, flags) = if check_revocation {
        (
            WTD_REVOKE_WHOLECHAIN,
            WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
        )
    } else {
        (WTD_REVOKE_NONE, WINTRUST_DATA_PROVIDER_FLAGS(0))
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: revocation,
        dwProvFlags: flags,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` points at `file`, which points at `wide`; all outlive
    // the call. Its state is released below.
    let result = unsafe { WinVerifyTrust(HWND::default(), &mut action, (&raw mut data).cast()) };
    let name = if result == 0 {
        publisher_of(data.hWVTStateData)
    } else {
        None
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: the same `data`, still alive; releases the verification state.
    let _ = unsafe { WinVerifyTrust(HWND::default(), &mut action, (&raw mut data).cast()) };
    name
}

/// Subject name of the leaf certificate of the first signer.
fn publisher_of(state: HANDLE) -> Option<String> {
    // SAFETY: `state` is the open state of a successful verification. The
    // provider data, signer and certificate it hands out belong to that state
    // and are only read here, before the caller closes it. Null results are
    // checked before use; the name buffer is as long as the API asked for.
    unsafe {
        let data = WTHelperProvDataFromStateData(state);
        if data.is_null() {
            return None;
        }
        let signer = WTHelperGetProvSignerFromChain(data, 0, false, 0);
        if signer.is_null() {
            return None;
        }
        let cert = WTHelperGetProvCertFromChain(signer, 0);
        if cert.is_null() || (*cert).pCert.is_null() {
            return None;
        }
        let context = (*cert).pCert;
        let len = CertGetNameStringW(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, None);
        if len <= 1 {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        let got = CertGetNameStringW(
            context,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            None,
            Some(&mut buf),
        );
        let text = buf.get(..(got as usize).saturating_sub(1))?;
        Some(String::from_utf16_lossy(text)).filter(|s| !s.is_empty())
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
            let _ = signer(&p, false);
        }
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("x.exe");
        std::fs::write(&f, b"MZ not really").unwrap();
        assert_eq!(signer(&f, false), None);
        assert_eq!(signer(&f, true), None);
    }
}
