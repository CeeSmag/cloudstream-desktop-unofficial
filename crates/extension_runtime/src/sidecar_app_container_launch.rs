//! Launches the sidecar JVM inside a Windows AppContainer (fail closed).
#![allow(unsafe_code)]

use std::ffi::OsStr;
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr;

use extension_api::GatewayError;

use crate::sidecar_app_container_acl::grant_modify;
use crate::sidecar_app_container_attrs::LaunchAttributes;
use crate::sidecar_app_container_cmdline::{
    data_dir_from_config, java_command_line, java_exe, java_home_dir, sidecar_home,
};
use crate::sidecar_app_container_ffi::{
    CloseHandle, ConvertStringSidToSidW, CreatePipe, CreateProcessW, GetLastError, LocalFree,
    ProcessInformation, ResumeThread, SecurityAttributes, SetHandleInformation, StartupInfoExW,
    StartupInfoW, TerminateProcess,
};
use crate::sidecar_app_container_mirror::ensure_readable;
use crate::sidecar_app_container_profile::{get_or_create_sidecar_container, AppContainerSid};
use crate::sidecar_job_object::SidecarJobObject;
use crate::SidecarLaunchConfig;

const EXTENDED_STARTUPINFO_PRESENT: u32 = 0x0008_0000;
const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_SUSPENDED: u32 = 0x0000_0004;
const STARTF_USESTDHANDLES: u32 = 0x0000_0100;
const STARTF_USESHOWWINDOW: u32 = 0x0000_0001;
const HANDLE_FLAG_INHERIT: u32 = 0x0000_0001;
const CAPABILITY_INTERNET_CLIENT: &str = "S-1-15-3-1";

/// A process started inside the AppContainer, with inherited stdio pipes.
pub(crate) struct AppContainerProcess {
    pub(crate) pid: u32,
    pub(crate) process: OwnedHandle,
    pub(crate) stdin: File,
    pub(crate) stdout: File,
    pub(crate) stderr: File,
    pub(crate) job: SidecarJobObject,
}

/// Prepares the AppContainer, ACLs, and starts `java.exe` for the sidecar config.
pub(crate) fn start_in_app_container(
    config: &SidecarLaunchConfig,
) -> Result<AppContainerProcess, GatewayError> {
    let sid = get_or_create_sidecar_container()?;
    let data_dir = data_dir_from_config(config)?;
    grant_sidecar_paths(&data_dir, &sid)?;
    let paths = prepare_readable_paths(config, &sid, &data_dir)?;
    let command_line = java_command_line(&paths.java, &paths.app_home, config)?;
    launch_command_line(&command_line, &sid, paths.java.parent())
}

/// Sidecar sandbox (`…/sidecar`) needs modify; host `.cs3` files live in sibling `…/extensions`.
fn grant_sidecar_paths(
    data_dir: &std::path::Path,
    sid: &AppContainerSid,
) -> Result<(), GatewayError> {
    use crate::sidecar_app_container_acl::grant_read_execute;
    grant_modify(data_dir, sid)?;
    let Some(app_data) = data_dir.parent() else {
        return Ok(());
    };
    let extensions = app_data.join("extensions");
    std::fs::create_dir_all(&extensions).map_err(|error| {
        GatewayError::Unavailable(format!(
            "cannot create extensions dir {}: {error}",
            extensions.display()
        ))
    })?;
    grant_read_execute(&extensions, sid)
}

struct ReadablePaths {
    java: std::path::PathBuf,
    app_home: std::path::PathBuf,
}

/// Grants RX on JDK / sidecar home, or mirrors them under `<data>/appcontainer` when DAC is denied.
fn prepare_readable_paths(
    config: &SidecarLaunchConfig,
    sid: &AppContainerSid,
    data_dir: &std::path::Path,
) -> Result<ReadablePaths, GatewayError> {
    let java = java_exe(config)?;
    let java_home = java_home_dir(&java)?;
    let app_home = sidecar_home(config)?;
    let stage = data_dir.join("appcontainer");
    let java_home = ensure_readable(&java_home, sid, &stage.join("jre"))?;
    let app_home = ensure_readable(&app_home, sid, &stage.join("sidecar"))?;
    let java = java_home.join("bin").join("java.exe");
    if !java.is_file() {
        return Err(GatewayError::Unavailable(format!(
            "java.exe missing after AppContainer path prep: {}",
            java.display()
        )));
    }
    Ok(ReadablePaths { java, app_home })
}

fn launch_command_line(
    command_line: &str,
    sid: &AppContainerSid,
    working_dir: Option<&std::path::Path>,
) -> Result<AppContainerProcess, GatewayError> {
    let mut app_sid = ptr::null_mut();
    let wide_sid = wide(sid.as_str());
    // SAFETY: `wide_sid` is NUL-terminated UTF-16 and `app_sid` is a valid out-pointer.
    if unsafe { ConvertStringSidToSidW(wide_sid.as_ptr(), &mut app_sid) } == 0 {
        return Err(launch_error("ConvertStringSidToSidW(container)"));
    }
    let mut cap_sid = ptr::null_mut();
    let wide_cap = wide(CAPABILITY_INTERNET_CLIENT);
    // SAFETY: `wide_cap` is NUL-terminated UTF-16; on failure `app_sid` is freed via `LocalFree`.
    if unsafe { ConvertStringSidToSidW(wide_cap.as_ptr(), &mut cap_sid) } == 0 {
        unsafe {
            LocalFree(app_sid);
        }
        return Err(launch_error("ConvertStringSidToSidW(capability)"));
    }
    // SAFETY: both SIDs came from `ConvertStringSidToSidW`, stay alive for `launch_with_sids`, and are freed right after.
    let outcome = unsafe { launch_with_sids(command_line, working_dir, app_sid, cap_sid) };
    unsafe {
        LocalFree(app_sid);
        LocalFree(cap_sid);
    }
    outcome
}

unsafe fn launch_with_sids(
    command_line: &str,
    working_dir: Option<&std::path::Path>,
    app_sid: *mut std::ffi::c_void,
    cap_sid: *mut std::ffi::c_void,
) -> Result<AppContainerProcess, GatewayError> {
    let (stdin_read, stdin_write) = create_pipe(true)?;
    let (stdout_read, stdout_write) = create_pipe(false)?;
    let (stderr_read, stderr_write) = create_pipe(false)?;
    let mut attrs =
        LaunchAttributes::new(app_sid, cap_sid, [stdin_read, stdout_write, stderr_write])?;
    let mut startup = startup_info(StartupHandles {
        attribute_list: attrs.list_ptr(),
        stdin_read,
        stdout_write,
        stderr_write,
    });
    let info = create_suspended(command_line, working_dir, &mut startup);
    drop(attrs);
    let info = info?;
    CloseHandle(stdin_read);
    CloseHandle(stdout_write);
    CloseHandle(stderr_write);
    finish_launch(info, stdin_write, stdout_read, stderr_read)
}

struct StartupHandles {
    attribute_list: *mut std::ffi::c_void,
    stdin_read: *mut std::ffi::c_void,
    stdout_write: *mut std::ffi::c_void,
    stderr_write: *mut std::ffi::c_void,
}

fn startup_info(handles: StartupHandles) -> StartupInfoExW {
    StartupInfoExW {
        startup_info: StartupInfoW {
            cb: std::mem::size_of::<StartupInfoExW>() as u32,
            reserved: ptr::null_mut(),
            desktop: ptr::null_mut(),
            title: ptr::null_mut(),
            dw_x: 0,
            dw_y: 0,
            dw_x_size: 0,
            dw_y_size: 0,
            dw_x_count_chars: 0,
            dw_y_count_chars: 0,
            dw_fill_attribute: 0,
            dw_flags: STARTF_USESTDHANDLES | STARTF_USESHOWWINDOW,
            w_show_window: 0,
            cb_reserved2: 0,
            lp_reserved2: ptr::null_mut(),
            h_std_input: handles.stdin_read,
            h_std_output: handles.stdout_write,
            h_std_error: handles.stderr_write,
        },
        attribute_list: handles.attribute_list,
    }
}

unsafe fn create_suspended(
    command_line: &str,
    working_dir: Option<&std::path::Path>,
    startup: &mut StartupInfoExW,
) -> Result<ProcessInformation, GatewayError> {
    let mut cmdline = wide(command_line);
    let cwd = working_dir.map(|path| wide(&path.to_string_lossy()));
    let mut info = ProcessInformation::default();
    let created = CreateProcessW(
        ptr::null(),
        cmdline.as_mut_ptr(),
        ptr::null_mut(),
        ptr::null_mut(),
        1,
        EXTENDED_STARTUPINFO_PRESENT
            | CREATE_UNICODE_ENVIRONMENT
            | CREATE_NO_WINDOW
            | CREATE_SUSPENDED,
        ptr::null_mut(),
        cwd.as_ref().map_or(ptr::null(), Vec::as_ptr),
        std::ptr::addr_of_mut!(*startup).cast(),
        &mut info,
    );
    if created == 0 {
        let code = GetLastError();
        return Err(GatewayError::Unavailable(format!(
            "cannot start the sidecar inside an AppContainer (CreateProcessW(AppContainer) failed, Win32 {code})"
        )));
    }
    Ok(info)
}

unsafe fn finish_launch(
    info: ProcessInformation,
    stdin_write: *mut std::ffi::c_void,
    stdout_read: *mut std::ffi::c_void,
    stderr_read: *mut std::ffi::c_void,
) -> Result<AppContainerProcess, GatewayError> {
    let process = OwnedHandle::from_raw_handle(info.process as RawHandle);
    let thread = OwnedHandle::from_raw_handle(info.thread as RawHandle);
    let job = match SidecarJobObject::constrain_raw_handle(process.as_raw_handle()) {
        Ok(job) => job,
        Err(error) => {
            TerminateProcess(info.process, 1);
            return Err(error);
        }
    };
    if ResumeThread(info.thread) == u32::MAX {
        return Err(launch_error("ResumeThread"));
    }
    drop(thread);
    tracing::info!(pid = info.process_id, "sidecar started inside AppContainer");
    Ok(AppContainerProcess {
        pid: info.process_id,
        process,
        stdin: File::from_raw_handle(stdin_write as RawHandle),
        stdout: File::from_raw_handle(stdout_read as RawHandle),
        stderr: File::from_raw_handle(stderr_read as RawHandle),
        job,
    })
}

fn create_pipe(
    parent_writes: bool,
) -> Result<(*mut std::ffi::c_void, *mut std::ffi::c_void), GatewayError> {
    let mut read = ptr::null_mut();
    let mut write = ptr::null_mut();
    let mut attributes = SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>() as u32,
        descriptor: ptr::null_mut(),
        inherit: 1,
    };
    // SAFETY: `read`, `write`, and `attributes` are valid pointers initialized for `CreatePipe`.
    if unsafe { CreatePipe(&mut read, &mut write, &mut attributes, 0) } == 0 {
        return Err(launch_error("CreatePipe"));
    }
    let parent = if parent_writes { write } else { read };
    // SAFETY: `parent` is a valid pipe handle from `CreatePipe`; both handles are closed on error.
    if unsafe { SetHandleInformation(parent, HANDLE_FLAG_INHERIT, 0) } == 0 {
        unsafe {
            CloseHandle(read);
            CloseHandle(write);
        }
        return Err(launch_error("SetHandleInformation"));
    }
    Ok((read, write))
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn launch_error(api: &str) -> GatewayError {
    // SAFETY: `GetLastError` reads the calling thread's last Win32 error code with no side effects.
    let code = unsafe { GetLastError() };
    GatewayError::Unavailable(format!(
        "cannot start the sidecar inside an AppContainer ({api} failed, Win32 {code})"
    ))
}
