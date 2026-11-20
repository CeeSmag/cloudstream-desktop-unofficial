//! Builds a `PROC_THREAD_ATTRIBUTE_LIST` that stays valid until CreateProcess returns.
#![allow(unsafe_code)]

use std::ptr;

use extension_api::GatewayError;

use crate::sidecar_app_container_ffi::{
    DeleteProcThreadAttributeList, GetLastError, InitializeProcThreadAttributeList,
    SecurityCapabilities, SidAndAttributes, UpdateProcThreadAttribute,
};

const PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES: usize = 0x0002_0009;
const PROC_THREAD_ATTRIBUTE_HANDLE_LIST: usize = 0x0002_0002;
const SE_GROUP_ENABLED: u32 = 0x0000_0004;
const ATTRIBUTE_COUNT: u32 = 2;

/// Heap-backed security + handle list so pointers stay valid if this struct moves.
pub(crate) struct LaunchAttributes {
    _cap: Box<SidAndAttributes>,
    _security: Box<SecurityCapabilities>,
    _handles: Box<[*mut std::ffi::c_void; 3]>,
    attr_buf: Vec<u8>,
}

impl LaunchAttributes {
    /// `child_stdio`: stdin read, stdout write, stderr write (inheritable child ends).
    pub(crate) unsafe fn new(
        app_sid: *mut std::ffi::c_void,
        cap_sid: *mut std::ffi::c_void,
        child_stdio: [*mut std::ffi::c_void; 3],
    ) -> Result<Self, GatewayError> {
        let mut cap = Box::new(SidAndAttributes {
            sid: cap_sid,
            attributes: SE_GROUP_ENABLED,
        });
        let cap_ptr = cap.as_mut() as *mut SidAndAttributes;
        let mut security = Box::new(SecurityCapabilities {
            app_container_sid: app_sid,
            capabilities: cap_ptr,
            capability_count: 1,
            reserved: 0,
        });
        let mut handles = Box::new(child_stdio);
        let mut attr_buf = attribute_buffer()?;
        add_attribute(AttributeUpdate {
            attr_buf: &mut attr_buf,
            attribute: PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
            value: security.as_mut() as *mut SecurityCapabilities as *mut std::ffi::c_void,
            size: std::mem::size_of::<SecurityCapabilities>(),
            label: "UpdateProcThreadAttribute(security)",
        })?;
        add_attribute(AttributeUpdate {
            attr_buf: &mut attr_buf,
            attribute: PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            value: handles.as_mut() as *mut _ as *mut std::ffi::c_void,
            size: std::mem::size_of_val(handles.as_ref()),
            label: "UpdateProcThreadAttribute(handles)",
        })?;
        Ok(Self {
            _cap: cap,
            _security: security,
            _handles: handles,
            attr_buf,
        })
    }

    pub(crate) fn list_ptr(&mut self) -> *mut std::ffi::c_void {
        self.attr_buf.as_mut_ptr().cast()
    }
}

impl Drop for LaunchAttributes {
    fn drop(&mut self) {
        if self.attr_buf.is_empty() {
            return;
        }
        // SAFETY: `attr_buf` was initialized by `InitializeProcThreadAttributeList` and has not yet been deleted.
        unsafe {
            DeleteProcThreadAttributeList(self.attr_buf.as_mut_ptr().cast());
        }
        self.attr_buf.clear();
    }
}

struct AttributeUpdate<'a> {
    attr_buf: &'a mut Vec<u8>,
    attribute: usize,
    value: *mut std::ffi::c_void,
    size: usize,
    label: &'a str,
}

unsafe fn attribute_buffer() -> Result<Vec<u8>, GatewayError> {
    let mut attr_size = 0usize;
    InitializeProcThreadAttributeList(ptr::null_mut(), ATTRIBUTE_COUNT, 0, &mut attr_size);
    let mut attr_buf = vec![0u8; attr_size];
    if InitializeProcThreadAttributeList(
        attr_buf.as_mut_ptr().cast(),
        ATTRIBUTE_COUNT,
        0,
        &mut attr_size,
    ) == 0
    {
        return Err(attr_error("InitializeProcThreadAttributeList"));
    }
    Ok(attr_buf)
}

unsafe fn add_attribute(update: AttributeUpdate<'_>) -> Result<(), GatewayError> {
    if UpdateProcThreadAttribute(
        update.attr_buf.as_mut_ptr().cast(),
        0,
        update.attribute,
        update.value,
        update.size,
        ptr::null_mut(),
        ptr::null_mut(),
    ) == 0
    {
        DeleteProcThreadAttributeList(update.attr_buf.as_mut_ptr().cast());
        update.attr_buf.clear();
        return Err(attr_error(update.label));
    }
    Ok(())
}

fn attr_error(api: &str) -> GatewayError {
    // SAFETY: `GetLastError` reads the calling thread's last Win32 error code with no side effects.
    let code = unsafe { GetLastError() };
    GatewayError::Unavailable(format!(
        "cannot start the sidecar inside an AppContainer ({api} failed, Win32 {code})"
    ))
}
