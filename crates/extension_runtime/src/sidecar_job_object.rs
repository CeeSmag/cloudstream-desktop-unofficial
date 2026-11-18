//! Windows Job Object that caps the sidecar process tree and kills it when the job handle closes.
#![allow(unsafe_code)]

use std::os::windows::io::AsRawHandle;
use std::process::Child;
use std::ptr;

use extension_api::GatewayError;

/// Soft match for the sidecar heap (`-Xmx512m`) plus JVM / native overhead.
const JOB_MEMORY_LIMIT_BYTES: u64 = 768 * 1024 * 1024;

/// Enough for `cs-sidecar.bat` → `cmd.exe` → `java.exe` and a few helpers.
const ACTIVE_PROCESS_LIMIT: u32 = 16;

const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
const JOB_OBJECT_LIMIT_JOB_MEMORY: u32 = 0x0000_0200;
const JOB_OBJECT_LIMIT_ACTIVE_PROCESS: u32 = 0x0000_0008;
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;

/// Owns a Job Object handle; dropping it ends every process still assigned (kill-on-job-close).
pub(crate) struct SidecarJobObject {
    handle: *mut std::ffi::c_void,
}

// SAFETY: the handle is used only on the thread that owns the sidecar slot.
unsafe impl Send for SidecarJobObject {}

impl SidecarJobObject {
    /// Creates a job with memory / process caps and assigns `child` to it.
    /// Fails closed: if the OS refuses, the caller must not keep an unconstrained sidecar.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn constrain(child: &Child) -> Result<Self, GatewayError> {
        Self::constrain_raw_handle(child.as_raw_handle())
    }

    /// Same as [`Self::constrain`], but for a raw process handle (AppContainer launch).
    pub(crate) fn constrain_raw_handle(
        process: *mut std::ffi::c_void,
    ) -> Result<Self, GatewayError> {
        let job = create_job()?;
        set_limits(job)?;
        assign_raw_process(job, process)?;
        tracing::info!(
            memory_mb = JOB_MEMORY_LIMIT_BYTES / (1024 * 1024),
            active_process_limit = ACTIVE_PROCESS_LIMIT,
            "sidecar assigned to Windows Job Object"
        );
        Ok(Self { handle: job })
    }
}

impl Drop for SidecarJobObject {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: handle was created by CreateJobObjectW and not closed yet.
            unsafe {
                CloseHandle(self.handle);
            }
            self.handle = ptr::null_mut();
        }
    }
}

fn create_job() -> Result<*mut std::ffi::c_void, GatewayError> {
    // SAFETY: CreateJobObjectW with null attributes returns an owned handle or null.
    let handle = unsafe { CreateJobObjectW(ptr::null_mut(), ptr::null()) };
    if handle.is_null() {
        return Err(job_error("CreateJobObjectW"));
    }
    Ok(handle)
}

fn set_limits(job: *mut std::ffi::c_void) -> Result<(), GatewayError> {
    let mut info = JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation {
            per_process_user_time_limit: 0,
            per_job_user_time_limit: 0,
            limit_flags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_JOB_MEMORY
                | JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
            minimum_working_set_size: 0,
            maximum_working_set_size: 0,
            active_process_limit: ACTIVE_PROCESS_LIMIT,
            affinity: 0,
            priority_class: 0,
            scheduling_class: 0,
        },
        io_info: IoCounters {
            read_operation_count: 0,
            write_operation_count: 0,
            other_operation_count: 0,
            read_transfer_count: 0,
            write_transfer_count: 0,
            other_transfer_count: 0,
        },
        process_memory_limit: 0,
        job_memory_limit: JOB_MEMORY_LIMIT_BYTES as usize,
        peak_process_memory_used: 0,
        peak_job_memory_used: 0,
    };
    // SAFETY: info is a valid JOBOBJECT_EXTENDED_LIMIT_INFORMATION for this job handle.
    let ok = unsafe {
        SetInformationJobObject(
            job,
            JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            std::ptr::addr_of_mut!(info).cast(),
            std::mem::size_of_val(&info) as u32,
        )
    };
    if ok == 0 {
        return Err(job_error("SetInformationJobObject"));
    }
    Ok(())
}

fn assign_raw_process(
    job: *mut std::ffi::c_void,
    process: *mut std::ffi::c_void,
) -> Result<(), GatewayError> {
    // SAFETY: process is a live process handle; job is a valid job object.
    let ok = unsafe { AssignProcessToJobObject(job, process) };
    if ok == 0 {
        return Err(job_error("AssignProcessToJobObject"));
    }
    Ok(())
}

fn job_error(api: &str) -> GatewayError {
    let code = unsafe { GetLastError() };
    GatewayError::Unavailable(format!(
        "cannot constrain the extension sidecar with a Windows Job Object ({api} failed, Win32 {code})"
    ))
}

#[repr(C)]
struct JobObjectBasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[repr(C)]
struct IoCounters {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[repr(C)]
struct JobObjectExtendedLimitInformation {
    basic_limit_information: JobObjectBasicLimitInformation,
    io_info: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(
        job_attributes: *mut std::ffi::c_void,
        name: *const u16,
    ) -> *mut std::ffi::c_void;
    fn SetInformationJobObject(
        job: *mut std::ffi::c_void,
        info_class: u32,
        info: *mut std::ffi::c_void,
        info_length: u32,
    ) -> i32;
    fn AssignProcessToJobObject(job: *mut std::ffi::c_void, process: *mut std::ffi::c_void) -> i32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    fn GetLastError() -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn constrain_assigns_a_short_lived_child_to_a_job_object() {
        let mut child = Command::new("cmd")
            .args(["/C", "ping", "-n", "2", "127.0.0.1", ">", "nul"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn");
        let job = SidecarJobObject::constrain(&child).expect("job");
        drop(job);
        let _ = child.wait();
    }
}
