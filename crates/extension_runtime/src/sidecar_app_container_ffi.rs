//! Win32 FFI types and imports for AppContainer process creation.
#![allow(unsafe_code)]

#[repr(C)]
pub(crate) struct SidAndAttributes {
    pub(crate) sid: *mut std::ffi::c_void,
    pub(crate) attributes: u32,
}

#[repr(C)]
pub(crate) struct SecurityCapabilities {
    pub(crate) app_container_sid: *mut std::ffi::c_void,
    pub(crate) capabilities: *mut SidAndAttributes,
    pub(crate) capability_count: u32,
    pub(crate) reserved: u32,
}

#[repr(C)]
pub(crate) struct SecurityAttributes {
    pub(crate) length: u32,
    pub(crate) descriptor: *mut std::ffi::c_void,
    pub(crate) inherit: i32,
}

#[repr(C)]
pub(crate) struct StartupInfoW {
    pub(crate) cb: u32,
    pub(crate) reserved: *mut u16,
    pub(crate) desktop: *mut u16,
    pub(crate) title: *mut u16,
    pub(crate) dw_x: u32,
    pub(crate) dw_y: u32,
    pub(crate) dw_x_size: u32,
    pub(crate) dw_y_size: u32,
    pub(crate) dw_x_count_chars: u32,
    pub(crate) dw_y_count_chars: u32,
    pub(crate) dw_fill_attribute: u32,
    pub(crate) dw_flags: u32,
    pub(crate) w_show_window: u16,
    pub(crate) cb_reserved2: u16,
    pub(crate) lp_reserved2: *mut u8,
    pub(crate) h_std_input: *mut std::ffi::c_void,
    pub(crate) h_std_output: *mut std::ffi::c_void,
    pub(crate) h_std_error: *mut std::ffi::c_void,
}

#[repr(C)]
pub(crate) struct StartupInfoExW {
    pub(crate) startup_info: StartupInfoW,
    pub(crate) attribute_list: *mut std::ffi::c_void,
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct ProcessInformation {
    pub(crate) process: *mut std::ffi::c_void,
    pub(crate) thread: *mut std::ffi::c_void,
    pub(crate) process_id: u32,
    pub(crate) thread_id: u32,
}

#[link(name = "advapi32")]
extern "system" {
    pub(crate) fn ConvertStringSidToSidW(
        string_sid: *const u16,
        sid: *mut *mut std::ffi::c_void,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    pub(crate) fn CreatePipe(
        read: *mut *mut std::ffi::c_void,
        write: *mut *mut std::ffi::c_void,
        attributes: *mut SecurityAttributes,
        size: u32,
    ) -> i32;
    pub(crate) fn SetHandleInformation(handle: *mut std::ffi::c_void, mask: u32, flags: u32)
        -> i32;
    pub(crate) fn InitializeProcThreadAttributeList(
        list: *mut std::ffi::c_void,
        count: u32,
        flags: u32,
        size: *mut usize,
    ) -> i32;
    pub(crate) fn UpdateProcThreadAttribute(
        list: *mut std::ffi::c_void,
        flags: u32,
        attribute: usize,
        value: *mut std::ffi::c_void,
        size: usize,
        previous: *mut std::ffi::c_void,
        return_size: *mut usize,
    ) -> i32;
    pub(crate) fn DeleteProcThreadAttributeList(list: *mut std::ffi::c_void);
    pub(crate) fn CreateProcessW(
        application: *const u16,
        command_line: *mut u16,
        process_attributes: *mut std::ffi::c_void,
        thread_attributes: *mut std::ffi::c_void,
        inherit_handles: i32,
        creation_flags: u32,
        environment: *mut std::ffi::c_void,
        current_directory: *const u16,
        startup_info: *mut std::ffi::c_void,
        process_information: *mut ProcessInformation,
    ) -> i32;
    pub(crate) fn ResumeThread(thread: *mut std::ffi::c_void) -> u32;
    pub(crate) fn TerminateProcess(process: *mut std::ffi::c_void, exit_code: u32) -> i32;
    pub(crate) fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    pub(crate) fn LocalFree(mem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    pub(crate) fn GetLastError() -> u32;
}
