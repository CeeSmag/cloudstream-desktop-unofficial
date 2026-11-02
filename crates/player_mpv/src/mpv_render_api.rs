//! libmpv's render API in its software flavour: mpv draws frames into memory we own.
#![allow(unsafe_code)]

use std::ffi::{c_char, c_int, c_void, CString};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use libloading::Library;
use player_api::PlayerError;

use crate::mpv_api::{symbol, MpvHandle};

const PARAM_API_TYPE: c_int = 1;
const PARAM_SW_SIZE: c_int = 17;
const PARAM_SW_FORMAT: c_int = 18;
const PARAM_SW_STRIDE: c_int = 19;
const PARAM_SW_POINTER: c_int = 20;
/// Bit of `mpv_render_context_update`'s result that says a new frame is ready.
const UPDATE_FRAME: u64 = 1;

#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

type UpdateCallback = unsafe extern "C" fn(*mut c_void);
type CreateFn = unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut RenderParam) -> c_int;
type SetCallbackFn = unsafe extern "C" fn(*mut c_void, Option<UpdateCallback>, *mut c_void);
type UpdateFn = unsafe extern "C" fn(*mut c_void) -> u64;
type RenderFn = unsafe extern "C" fn(*mut c_void, *mut RenderParam) -> c_int;
type FreeFn = unsafe extern "C" fn(*mut c_void);

/// Resolved render-context entry points.
pub(crate) struct RenderApi {
    create: CreateFn,
    set_update_callback: SetCallbackFn,
    update: UpdateFn,
    render: RenderFn,
    free: FreeFn,
}

impl RenderApi {
    /// Resolves the functions from `render.h`; fails on libmpv builds without them.
    pub(crate) fn load(library: &Library) -> Result<Self, PlayerError> {
        // SAFETY: names and function types match libmpv's public `render.h`.
        unsafe {
            Ok(Self {
                create: symbol(library, b"mpv_render_context_create\0")?,
                set_update_callback: symbol(library, b"mpv_render_context_set_update_callback\0")?,
                update: symbol(library, b"mpv_render_context_update\0")?,
                render: symbol(library, b"mpv_render_context_render\0")?,
                free: symbol(library, b"mpv_render_context_free\0")?,
            })
        }
    }
}

/// Wakes the render thread when mpv has a new frame.
#[derive(Default)]
pub(crate) struct FrameSignal {
    pending: Mutex<bool>,
    wake: Condvar,
}

impl FrameSignal {
    /// Marks a frame as pending and wakes the waiter.
    pub(crate) fn notify(&self) {
        *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.wake.notify_one();
    }

    /// Waits up to `timeout` for a notification; true when one arrived.
    pub(crate) fn wait(&self, timeout: Duration) -> bool {
        let guard = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let (mut guard, _) = self
            .wake
            .wait_timeout_while(guard, timeout, |pending| !*pending)
            .unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *guard)
    }
}

/// A software render context bound to one mpv instance.
pub(crate) struct MpvRenderContext {
    handle: Arc<MpvHandle>,
    raw: *mut c_void,
    signal: *const FrameSignal,
}

// SAFETY: libmpv documents the render context as usable from one thread at a time; the app moves
// it into the single render thread and never shares it.
unsafe impl Send for MpvRenderContext {}

/// Called by mpv on arbitrary threads when a frame is ready; must not call back into mpv.
unsafe extern "C" fn on_update(data: *mut c_void) {
    // SAFETY: `data` is the `Arc<FrameSignal>` pointer registered in `create`; it stays alive until
    // `Drop` has cleared the callback.
    let signal = unsafe { &*data.cast::<FrameSignal>() };
    signal.notify();
}

impl MpvRenderContext {
    /// Creates a software render context; `signal` is notified for every new frame.
    pub(crate) fn create(
        handle: Arc<MpvHandle>,
        signal: Arc<FrameSignal>,
    ) -> Result<Self, PlayerError> {
        let api = handle
            .api()
            .render_api()
            .ok_or_else(|| PlayerError::Unavailable("this libmpv has no render API".to_owned()))?;
        let kind =
            CString::new("sw").map_err(|_| PlayerError::Unavailable("bad api type".to_owned()))?;
        let mut params = [
            RenderParam {
                kind: PARAM_API_TYPE,
                data: kind.as_ptr().cast_mut().cast(),
            },
            RenderParam {
                kind: 0,
                data: std::ptr::null_mut(),
            },
        ];
        let mut raw: *mut c_void = std::ptr::null_mut();
        // SAFETY: `handle.raw()` is a live initialised mpv handle; `params` is a terminated array
        // whose pointers outlive the call.
        let code = unsafe { (api.create)(&raw mut raw, handle.raw(), params.as_mut_ptr()) };
        if code < 0 || raw.is_null() {
            return Err(PlayerError::Command {
                command: "render_context_create".to_owned(),
                reason: handle.describe(code),
            });
        }
        let signal = Arc::into_raw(signal);
        // SAFETY: `raw` is a live render context; `signal` stays valid until `Drop` reclaims it.
        unsafe { (api.set_update_callback)(raw, Some(on_update), signal.cast_mut().cast()) };
        Ok(Self {
            handle,
            raw,
            signal,
        })
    }

    /// Acknowledges mpv's update request; true when a new frame should be rendered.
    pub(crate) fn has_new_frame(&self) -> bool {
        let Some(api) = self.handle.api().render_api() else {
            return false;
        };
        // SAFETY: `raw` is a live render context owned by `self`.
        let flags = unsafe { (api.update)(self.raw) };
        flags & UPDATE_FRAME != 0
    }

    /// Renders the current frame as `rgb0` into `pixels` (`stride * height` bytes).
    pub(crate) fn render(
        &self,
        size: (u32, u32),
        stride: usize,
        pixels: &mut [u8],
    ) -> Result<(), PlayerError> {
        let api = self
            .handle
            .api()
            .render_api()
            .ok_or_else(|| PlayerError::Unavailable("render API vanished".to_owned()))?;
        check_buffer(size, stride, pixels.len())?;
        let mut dimensions: [c_int; 2] = [size.0 as c_int, size.1 as c_int];
        let format =
            CString::new("rgb0").map_err(|_| PlayerError::Unavailable("bad format".to_owned()))?;
        let mut stride_value: usize = stride;
        let mut params = software_params(&mut dimensions, &format, &mut stride_value, pixels);
        // SAFETY: `raw` is live; every parameter points at data valid for the call, and `pixels`
        // holds at least `stride * height` writable bytes (checked by `check_buffer`).
        let code = unsafe { (api.render)(self.raw, params.as_mut_ptr()) };
        if code < 0 {
            return Err(PlayerError::Command {
                command: "render".to_owned(),
                reason: self.handle.describe(code),
            });
        }
        Ok(())
    }
}

/// Refuses buffers that mpv could write past.
fn check_buffer(size: (u32, u32), stride: usize, length: usize) -> Result<(), PlayerError> {
    if length < stride * size.1 as usize || stride < size.0 as usize * 4 {
        return Err(PlayerError::Command {
            command: "render".to_owned(),
            reason: "buffer too small".to_owned(),
        });
    }
    Ok(())
}

/// The terminated parameter list for one software render call.
fn software_params(
    dimensions: &mut [c_int; 2],
    format: &CString,
    stride: &mut usize,
    pixels: &mut [u8],
) -> [RenderParam; 5] {
    [
        RenderParam {
            kind: PARAM_SW_SIZE,
            data: dimensions.as_mut_ptr().cast(),
        },
        RenderParam {
            kind: PARAM_SW_FORMAT,
            data: format.as_ptr().cast_mut().cast::<c_char>().cast(),
        },
        RenderParam {
            kind: PARAM_SW_STRIDE,
            data: (&raw mut *stride).cast(),
        },
        RenderParam {
            kind: PARAM_SW_POINTER,
            data: pixels.as_mut_ptr().cast(),
        },
        RenderParam {
            kind: 0,
            data: std::ptr::null_mut(),
        },
    ]
}

impl Drop for MpvRenderContext {
    fn drop(&mut self) {
        if let Some(api) = self.handle.api().render_api() {
            // SAFETY: `raw` is live; clearing the callback first guarantees `on_update` no longer
            // runs, then the context is freed (before the mpv handle, which `self.handle` keeps alive).
            unsafe {
                (api.set_update_callback)(self.raw, None, std::ptr::null_mut());
                (api.free)(self.raw);
            }
        }
        // SAFETY: `signal` came from `Arc::into_raw` in `create` and is reclaimed exactly once.
        drop(unsafe { Arc::from_raw(self.signal) });
    }
}
