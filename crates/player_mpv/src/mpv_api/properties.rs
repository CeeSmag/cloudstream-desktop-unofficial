//! Reading mpv properties: numbers, flags and text.

use std::ffi::{c_char, c_double, c_int, CStr, CString};

use super::MpvHandle;

/// `mpv_format` values used here.
const FORMAT_FLAG: c_int = 3;
const FORMAT_STRING: c_int = 1;
const FORMAT_DOUBLE: c_int = 5;

impl MpvHandle {
    /// Reads a numeric property; `None` while it is unavailable (for example before a file loads).
    pub(crate) fn get_double(&self, name: &str) -> Option<f64> {
        let name = CString::new(name).ok()?;
        let mut value: c_double = 0.0;
        // SAFETY: `raw` is live; `value` is a properly sized out-parameter for FORMAT_DOUBLE.
        let code = unsafe {
            (self.api.get_property)(
                self.raw,
                name.as_ptr(),
                FORMAT_DOUBLE,
                (&raw mut value).cast(),
            )
        };
        (code >= 0).then_some(value)
    }

    /// Reads a property as text; `None` while it is unavailable.
    pub(crate) fn get_string(&self, name: &str) -> Option<String> {
        let name = CString::new(name).ok()?;
        let mut text: *mut c_char = std::ptr::null_mut();
        // SAFETY: `raw` is live; for FORMAT_STRING mpv stores a newly allocated C string in `text`.
        let code = unsafe {
            (self.api.get_property)(
                self.raw,
                name.as_ptr(),
                FORMAT_STRING,
                (&raw mut text).cast(),
            )
        };
        if code < 0 || text.is_null() {
            return None;
        }
        // SAFETY: on success `text` is a valid NUL-terminated string that we own until freed.
        let value = unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: `text` was allocated by mpv and must be released with `mpv_free`, exactly once.
        unsafe { (self.api.free_memory)(text.cast()) };
        Some(value)
    }

    /// Reads a boolean property; `None` while it is unavailable.
    pub(crate) fn get_flag(&self, name: &str) -> Option<bool> {
        let name = CString::new(name).ok()?;
        let mut value: c_int = 0;
        // SAFETY: `raw` is live; `value` is a properly sized out-parameter for FORMAT_FLAG.
        let code = unsafe {
            (self.api.get_property)(
                self.raw,
                name.as_ptr(),
                FORMAT_FLAG,
                (&raw mut value).cast(),
            )
        };
        (code >= 0).then_some(value != 0)
    }
}
