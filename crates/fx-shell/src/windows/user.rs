//! Who a process runs as (its user SID), and security descriptors built from
//! SDDL: for pipes that only this user may use (review, D-066).

use std::ffi::c_void;
use std::io;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::{HSTRING, PWSTR};

/// SID of the user running this process, as `S-1-5-21-...`.
pub fn current_user_sid() -> io::Result<String> {
    // SAFETY: the pseudo-handle of this process is always valid and needs no
    // closing.
    token_sid_of(unsafe { GetCurrentProcess() })
}

/// SID of the user running process `pid`.
pub fn process_user_sid(pid: u32) -> io::Result<String> {
    // SAFETY: plain call; the handle it returns is closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }?;
    let sid = token_sid_of(process);
    // SAFETY: we own `process` and close it once.
    let _ = unsafe { CloseHandle(process) };
    sid
}

fn token_sid_of(process: HANDLE) -> io::Result<String> {
    let mut token = HANDLE::default();
    // SAFETY: `process` is a valid process handle; `token` receives a handle
    // closed below.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }?;
    let sid = token_user_sid(token);
    // SAFETY: we own the token handle and close it once.
    let _ = unsafe { CloseHandle(token) };
    sid
}

fn token_user_sid(token: HANDLE) -> io::Result<String> {
    let mut needed = 0u32;
    // SAFETY: size query (no buffer); fails with "insufficient buffer" and
    // sets `needed`.
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
    }?;
    // SAFETY: the buffer now holds a TOKEN_USER (aligned, see above).
    let user = unsafe { &*buf.as_ptr().cast::<TOKEN_USER>() };
    let mut text = PWSTR::null();
    // SAFETY: the SID points into `buf`, alive for the call; `text` receives a
    // LocalAlloc'ed string freed below.
    unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) }?;
    // SAFETY: `text` is a valid NUL-terminated string from the conversion.
    let sid = unsafe { text.to_string() }.map_err(|_| io::Error::other("bad SID"));
    // SAFETY: allocated by ConvertSidToStringSidW, freed once.
    let _ = unsafe { LocalFree(Some(HLOCAL(text.0.cast()))) };
    sid
}

/// A security descriptor built from SDDL, freed on drop.
pub(crate) struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    pub(crate) fn from_sddl(sddl: &str) -> io::Result<Self> {
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
        }?;
        Ok(Self(sd))
    }

    /// Attributes pointing at this descriptor; valid while `self` lives.
    pub(crate) fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.0,
            bInheritHandle: false.into(),
        }
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: the descriptor came from LocalAlloc (SDDL conversion) and is
        // freed once.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.0.0))) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_and_its_pid_have_the_same_user() {
        let me = current_user_sid().unwrap();
        assert!(me.starts_with("S-1-"), "{me}");
        assert_eq!(process_user_sid(std::process::id()).unwrap(), me);
        assert!(Descriptor::from_sddl(&format!("D:P(A;;GA;;;{me})")).is_ok());
        assert!(Descriptor::from_sddl("not sddl").is_err());
    }
}
