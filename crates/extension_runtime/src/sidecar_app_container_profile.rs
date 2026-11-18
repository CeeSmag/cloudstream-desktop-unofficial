//! Creates or derives the Windows AppContainer profile used for the plugin sidecar.
#![allow(unsafe_code)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr;

use extension_api::GatewayError;

/// Stable AppContainer name for the CloudStream plugin sidecar (max 64 chars).
pub(crate) const SIDECAR_CONTAINER_NAME: &str = "CloudStream.PluginSidecar.v1";

/// Capability SID: outbound internet client (S-1-15-3-1).
const CAPABILITY_INTERNET_CLIENT: &str = "S-1-15-3-1";

const S_OK: i32 = 0;
/// HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)
const ERROR_ALREADY_EXISTS_HRESULT: i32 = -2147024713;
const SE_GROUP_ENABLED: u32 = 0x0000_0004;

/// AppContainer SID string (e.g. `S-1-15-2-...`) for the sidecar profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppContainerSid(String);

impl AppContainerSid {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Creates the sidecar AppContainer profile or returns its existing SID.
pub(crate) fn get_or_create_sidecar_container() -> Result<AppContainerSid, GatewayError> {
    get_or_create_container(
        SIDECAR_CONTAINER_NAME,
        "CloudStream plugin sidecar",
        "Isolates the CloudStream JVM extension sidecar",
    )
}

fn get_or_create_container(
    name: &str,
    display: &str,
    description: &str,
) -> Result<AppContainerSid, GatewayError> {
    let (hr, profile_sid) = create_profile(name, display, description)?;
    if hr == S_OK {
        let sid = sid_to_string(profile_sid)?;
        // SAFETY: profile SID from CreateAppContainerProfile is freed with FreeSid.
        unsafe {
            FreeSid(profile_sid);
        }
        tracing::info!(container = name, %sid, "created AppContainer profile");
        return Ok(AppContainerSid(sid));
    }
    recover_existing_profile(name, hr)
}

fn create_profile(
    name: &str,
    display: &str,
    description: &str,
) -> Result<(i32, *mut std::ffi::c_void), GatewayError> {
    let mut capability_sid = ptr::null_mut();
    let wide_cap = wide(CAPABILITY_INTERNET_CLIENT);
    // SAFETY: ConvertStringSidToSidW allocates a SID the caller must LocalFree.
    let converted = unsafe { ConvertStringSidToSidW(wide_cap.as_ptr(), &mut capability_sid) };
    if converted == 0 || capability_sid.is_null() {
        return Err(ac_error("ConvertStringSidToSidW(internetClient)"));
    }
    let mut attributes = SidAndAttributes {
        sid: capability_sid,
        attributes: SE_GROUP_ENABLED,
    };
    let wide_name = wide(name);
    let wide_display = wide(display);
    let wide_description = wide(description);
    let mut profile_sid = ptr::null_mut();
    // SAFETY: wide strings are null-terminated; capabilities points at one SID.
    let hr = unsafe {
        CreateAppContainerProfile(
            wide_name.as_ptr(),
            wide_display.as_ptr(),
            wide_description.as_ptr(),
            &mut attributes,
            1,
            &mut profile_sid,
        )
    };
    // SAFETY: capability SID was allocated by ConvertStringSidToSidW.
    unsafe {
        LocalFree(capability_sid);
    }
    Ok((hr, profile_sid))
}

fn recover_existing_profile(name: &str, hr: i32) -> Result<AppContainerSid, GatewayError> {
    if hr == ERROR_ALREADY_EXISTS_HRESULT {
        return derive_container_sid(name);
    }
    match derive_container_sid(name) {
        Ok(sid) => {
            tracing::warn!(
                container = name,
                hr = format_args!("0x{hr:08X}"),
                "CreateAppContainerProfile failed; reused derived SID"
            );
            Ok(sid)
        }
        Err(_) => Err(GatewayError::Unavailable(format!(
            "CreateAppContainerProfile failed for '{name}' (HRESULT 0x{hr:08X})"
        ))),
    }
}

fn derive_container_sid(name: &str) -> Result<AppContainerSid, GatewayError> {
    let wide_name = wide(name);
    let mut sid = ptr::null_mut();
    // SAFETY: name is a null-terminated wide string.
    let hr = unsafe { DeriveAppContainerSidFromAppContainerName(wide_name.as_ptr(), &mut sid) };
    if hr != S_OK || sid.is_null() {
        return Err(GatewayError::Unavailable(format!(
            "DeriveAppContainerSidFromAppContainerName failed for '{name}' (HRESULT 0x{hr:08X})"
        )));
    }
    let text = sid_to_string(sid)?;
    // SAFETY: SID from DeriveAppContainerSidFromAppContainerName.
    unsafe {
        FreeSid(sid);
    }
    tracing::info!(container = name, sid = %text, "reused AppContainer profile");
    Ok(AppContainerSid(text))
}

fn sid_to_string(sid: *mut std::ffi::c_void) -> Result<String, GatewayError> {
    let mut wide_ptr = ptr::null_mut();
    // SAFETY: sid is a valid PSID; ConvertSidToStringSidW allocates a LocalFree string.
    let ok = unsafe { ConvertSidToStringSidW(sid, &mut wide_ptr) };
    if ok == 0 || wide_ptr.is_null() {
        return Err(ac_error("ConvertSidToStringSidW"));
    }
    let text = unsafe { wide_ptr_to_string(wide_ptr) };
    unsafe {
        LocalFree(wide_ptr.cast());
    }
    Ok(text)
}

unsafe fn wide_ptr_to_string(ptr: *mut u16) -> String {
    let mut len = 0usize;
    // SAFETY: ptr is a null-terminated wide string from the Win32 allocator.
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf16_lossy(slice)
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn ac_error(api: &str) -> GatewayError {
    let code = unsafe { GetLastError() };
    GatewayError::Unavailable(format!(
        "cannot prepare the AppContainer ({api} failed, Win32 {code})"
    ))
}

#[repr(C)]
struct SidAndAttributes {
    sid: *mut std::ffi::c_void,
    attributes: u32,
}

#[link(name = "userenv")]
extern "system" {
    fn CreateAppContainerProfile(
        name: *const u16,
        display_name: *const u16,
        description: *const u16,
        capabilities: *mut SidAndAttributes,
        capability_count: u32,
        app_container_sid: *mut *mut std::ffi::c_void,
    ) -> i32;
    fn DeriveAppContainerSidFromAppContainerName(
        name: *const u16,
        app_container_sid: *mut *mut std::ffi::c_void,
    ) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn ConvertStringSidToSidW(string_sid: *const u16, sid: *mut *mut std::ffi::c_void) -> i32;
    fn ConvertSidToStringSidW(sid: *mut std::ffi::c_void, string_sid: *mut *mut u16) -> i32;
    fn FreeSid(sid: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(mem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn GetLastError() -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_or_create_sidecar_container_returns_a_sid_string() {
        let sid = get_or_create_sidecar_container().expect("profile");
        assert!(
            sid.as_str().starts_with("S-1-15-2-"),
            "unexpected sid {}",
            sid.as_str()
        );
        let again = get_or_create_sidecar_container().expect("derive");
        assert_eq!(sid, again);
    }
}
