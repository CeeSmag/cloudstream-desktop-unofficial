//! The only module with `unsafe`: raw libmpv FFI, wrapped in a safe handle.
#![allow(unsafe_code)]

use std::ffi::{c_char, c_double, c_int, c_void, CStr, CString};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use libloading::Library;
use player_api::PlayerError;

use crate::mpv_event::{decode_event, MpvEvent};
use crate::mpv_log::{forward, LogApi};
use crate::mpv_render_api::RenderApi;

mod properties;

/// `mpv_event_id` of the end-of-file event, whose payload is an `mpv_event_end_file`.
const EVENT_END_FILE: c_int = 7;
/// `mpv_event_id` of a log message, whose payload is an `mpv_event_log_message`.
const EVENT_LOG_MESSAGE: c_int = 22;

#[repr(C)]
struct RawEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct RawEndFile {
    reason: c_int,
    error: c_int,
}

type CreateFn = unsafe extern "C" fn() -> *mut c_void;
type InitializeFn = unsafe extern "C" fn(*mut c_void) -> c_int;
type SetStringFn = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
type CommandFn = unsafe extern "C" fn(*mut c_void, *mut *const c_char) -> c_int;
type GetPropertyFn = unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int;
type WaitEventFn = unsafe extern "C" fn(*mut c_void, c_double) -> *mut RawEvent;
type DestroyFn = unsafe extern "C" fn(*mut c_void);
type ErrorStringFn = unsafe extern "C" fn(c_int) -> *const c_char;

/// Resolved libmpv entry points. The `Library` is kept so the pointers stay valid.
pub(crate) struct MpvApi {
    _library: Library,
    create: CreateFn,
    initialize: InitializeFn,
    set_option_string: SetStringFn,
    set_property_string: SetStringFn,
    command: CommandFn,
    get_property: GetPropertyFn,
    wait_event: WaitEventFn,
    terminate_destroy: DestroyFn,
    error_string: ErrorStringFn,
    free_memory: DestroyFn,
    render: Option<RenderApi>,
    log: Option<LogApi>,
}

fn unavailable(detail: impl std::fmt::Display) -> PlayerError {
    PlayerError::Unavailable(detail.to_string())
}

/// Copies one function pointer out of the library.
///
/// # Safety
/// `T` must be the exact `extern "C"` function type of the symbol `name` in libmpv's `client.h`.
pub(crate) unsafe fn symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T, PlayerError> {
    // SAFETY: guaranteed by the caller (see above); the pointer is copied out of the `Symbol`,
    // and `MpvApi` keeps the `Library` alive for as long as the pointer is used.
    unsafe { library.get::<T>(name) }
        .map(|found| *found)
        .map_err(unavailable)
}

impl MpvApi {
    /// Loads libmpv from `path` and resolves every function the app uses.
    pub(crate) fn load(path: &Path) -> Result<Self, PlayerError> {
        // SAFETY: loading a shared library runs its initialisers; libmpv's are safe to run.
        let library = unsafe { Library::new(path) }
            .map_err(|error| unavailable(format!("cannot load {}: {error}", path.display())))?;
        // SAFETY: each symbol name and function type below matches libmpv's public `client.h`.
        unsafe {
            Ok(Self {
                create: symbol(&library, b"mpv_create\0")?,
                initialize: symbol(&library, b"mpv_initialize\0")?,
                set_option_string: symbol(&library, b"mpv_set_option_string\0")?,
                set_property_string: symbol(&library, b"mpv_set_property_string\0")?,
                command: symbol(&library, b"mpv_command\0")?,
                get_property: symbol(&library, b"mpv_get_property\0")?,
                wait_event: symbol(&library, b"mpv_wait_event\0")?,
                terminate_destroy: symbol(&library, b"mpv_terminate_destroy\0")?,
                error_string: symbol(&library, b"mpv_error_string\0")?,
                free_memory: symbol(&library, b"mpv_free\0")?,
                render: RenderApi::load(&library).ok(),
                log: LogApi::load(&library).ok(),
                _library: library,
            })
        }
    }
}

impl MpvApi {
    /// The render-context entry points, when this libmpv build has them.
    pub(crate) fn render_api(&self) -> Option<&RenderApi> {
        self.render.as_ref()
    }
}

/// An owned mpv instance. Destroyed on drop.
pub(crate) struct MpvHandle {
    api: Arc<MpvApi>,
    raw: *mut c_void,
}

// SAFETY: libmpv's client API is documented as thread-safe for a single `mpv_handle`.
unsafe impl Send for MpvHandle {}
// SAFETY: see above; all access goes through libmpv's own locking.
unsafe impl Sync for MpvHandle {}

fn c_string(value: &str, what: &str) -> Result<CString, PlayerError> {
    CString::new(value).map_err(|_| PlayerError::Command {
        command: what.to_owned(),
        reason: "value contains a NUL byte".to_owned(),
    })
}

impl MpvHandle {
    /// Creates an mpv instance (not yet initialised, so options can still be set).
    pub(crate) fn create(api: Arc<MpvApi>) -> Result<Self, PlayerError> {
        // SAFETY: `mpv_create` has no preconditions.
        let raw = unsafe { (api.create)() };
        if raw.is_null() {
            return Err(unavailable("mpv_create returned null"));
        }
        Ok(Self { api, raw })
    }

    pub(crate) fn describe(&self, code: c_int) -> String {
        // SAFETY: `mpv_error_string` returns a static NUL-terminated string for any code.
        let text = unsafe { (self.api.error_string)(code) };
        if text.is_null() {
            return format!("error {code}");
        }
        // SAFETY: non-null, static, NUL-terminated (see above).
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    }

    fn check(&self, code: c_int, command: &str) -> Result<(), PlayerError> {
        if code >= 0 {
            return Ok(());
        }
        Err(PlayerError::Command {
            command: command.to_owned(),
            reason: self.describe(code),
        })
    }

    /// The raw `mpv_handle*`, for the render module.
    pub(crate) fn raw(&self) -> *mut c_void {
        self.raw
    }

    /// The loaded API, for the render module.
    pub(crate) fn api(&self) -> &MpvApi {
        &self.api
    }

    /// Finishes configuration; options set afterwards go through `set_property`.
    pub(crate) fn initialize(&self) -> Result<(), PlayerError> {
        // SAFETY: `raw` is a live handle owned by `self`.
        self.check(unsafe { (self.api.initialize)(self.raw) }, "initialize")
    }

    /// Sets an option before `initialize`.
    pub(crate) fn set_option(&self, name: &str, value: &str) -> Result<(), PlayerError> {
        let (n, v) = (c_string(name, name)?, c_string(value, name)?);
        // SAFETY: `raw` is live; both pointers are valid NUL-terminated strings for the call.
        self.check(
            unsafe { (self.api.set_option_string)(self.raw, n.as_ptr(), v.as_ptr()) },
            name,
        )
    }

    /// Sets a property on a running instance.
    pub(crate) fn set_property(&self, name: &str, value: &str) -> Result<(), PlayerError> {
        let (n, v) = (c_string(name, name)?, c_string(value, name)?);
        // SAFETY: `raw` is live; both pointers are valid NUL-terminated strings for the call.
        self.check(
            unsafe { (self.api.set_property_string)(self.raw, n.as_ptr(), v.as_ptr()) },
            name,
        )
    }

    /// Has mpv deliver its warnings and errors as events; [`MpvHandle::wait_event`] logs them.
    pub(crate) fn forward_mpv_log(&self) {
        match &self.api.log {
            Some(log) => {
                let code = log.request(self.raw, "warn");
                if code < 0 {
                    tracing::warn!(reason = %self.describe(code), "could not ask mpv for its log");
                }
            }
            None => tracing::warn!("this libmpv cannot forward its log"),
        }
    }

    /// Runs a command such as `["loadfile", url, "replace"]`.
    pub(crate) fn command(&self, arguments: &[&str]) -> Result<(), PlayerError> {
        let name = arguments.first().copied().unwrap_or("command");
        let owned = arguments
            .iter()
            .map(|a| c_string(a, name))
            .collect::<Result<Vec<_>, _>>()?;
        let mut pointers: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        pointers.push(std::ptr::null());
        // SAFETY: `raw` is live; `pointers` is a NULL-terminated array of valid C strings
        // that outlive the call.
        self.check(
            unsafe { (self.api.command)(self.raw, pointers.as_mut_ptr()) },
            name,
        )
    }

    /// Waits up to `timeout` for the next event.
    pub(crate) fn wait_event(&self, timeout: Duration) -> MpvEvent {
        // SAFETY: `raw` is live; the returned event is valid until the next call on this handle,
        // and only this crate's single event thread calls `wait_event`.
        let event = unsafe { (self.api.wait_event)(self.raw, timeout.as_secs_f64()) };
        if event.is_null() {
            return MpvEvent::Nothing;
        }
        // SAFETY: non-null event pointer returned by mpv (see above).
        let (event_id, data) = unsafe { ((*event).event_id, (*event).data) };
        if event_id == EVENT_LOG_MESSAGE {
            if !data.is_null() {
                // SAFETY: for LOG_MESSAGE events `data` points to an `mpv_event_log_message`.
                unsafe { forward(data) };
            }
            return MpvEvent::Other;
        }
        let end_file = (event_id == EVENT_END_FILE && !data.is_null()).then(|| {
            // SAFETY: for END_FILE events `data` points to an `mpv_event_end_file`.
            let details = unsafe { &*data.cast::<RawEndFile>() };
            (details.reason, details.error)
        });
        decode_event(event_id, end_file)
    }
}

impl Drop for MpvHandle {
    fn drop(&mut self) {
        // SAFETY: `raw` is live and never used again after this call.
        unsafe { (self.api.terminate_destroy)(self.raw) };
    }
}
