//! Forwards libmpv's own warnings and errors into the app's log, so a failure such as a subtitle
//! file that cannot be opened is explained in the player's own words.
#![allow(unsafe_code)]

use std::ffi::{c_char, c_int, c_void, CStr, CString};

use libloading::Library;
use player_api::PlayerError;

use crate::mpv_api::symbol;

type RequestLogFn = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;

/// `mpv_event_log_message`.
#[repr(C)]
struct RawLogMessage {
    prefix: *const c_char,
    level: *const c_char,
    text: *const c_char,
    log_level: c_int,
}

/// The mpv log level to ask for: warnings and errors, or while debug logging is on everything mpv
/// says at `verbose` and worse.
pub(crate) fn log_level(verbose: bool) -> &'static str {
    if verbose {
        "v"
    } else {
        "warn"
    }
}

/// The entry point that asks mpv to deliver its log messages as events.
pub(crate) struct LogApi {
    request: RequestLogFn,
}

impl LogApi {
    pub(crate) fn load(library: &Library) -> Result<Self, PlayerError> {
        // SAFETY: the name and type match `mpv_request_log_messages` in libmpv's `client.h`.
        let request = unsafe { symbol(library, b"mpv_request_log_messages\0")? };
        Ok(Self { request })
    }

    /// Asks for messages of `level` and worse (`warn`, `error`, ...); returns mpv's result code.
    pub(crate) fn request(&self, raw: *mut c_void, level: &str) -> c_int {
        let Ok(level) = CString::new(level) else {
            return -1;
        };
        // SAFETY: `raw` is a live handle and `level` a valid C string for the call.
        unsafe { (self.request)(raw, level.as_ptr()) }
    }
}

fn text_of(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    // SAFETY: mpv passes NUL-terminated strings that stay valid until the next event call.
    unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .trim_end()
        .to_owned()
}

/// Writes one mpv log event into the app's log at the matching level.
///
/// # Safety
/// `data` must be the payload of an `MPV_EVENT_LOG_MESSAGE` event, valid for this call.
pub(crate) unsafe fn forward(data: *mut c_void) {
    // SAFETY: guaranteed by the caller (see above).
    let message = unsafe { &*data.cast::<RawLogMessage>() };
    let (prefix, level, text) = (
        text_of(message.prefix),
        text_of(message.level),
        text_of(message.text),
    );
    match level.as_str() {
        "fatal" | "error" => tracing::error!(source = %prefix, "mpv: {text}"),
        "warn" => tracing::warn!(source = %prefix, "mpv: {text}"),
        _ => tracing::debug!(source = %prefix, %level, "mpv: {text}"),
    }
}
