//! libmpv's render API in its software flavour: mpv draws frames into memory we own. The OpenGL
//! flavour (mpv draws into a texture of the app's GL context) is the child module `gl`.
#![allow(unsafe_code)]

mod gl;

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

type WakeCallback = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct SignalState {
    pending: bool,
    on_frame: Option<WakeCallback>,
}

/// Wakes the render thread (or UI redraw callback) when mpv has a new frame.
#[derive(Default)]
pub(crate) struct FrameSignal {
    state: Mutex<SignalState>,
    wake: Condvar,
}

impl FrameSignal {
    /// Registers a callback invoked on the `false -> true` pending edge (coalesced so rapid mpv
    /// notifications never flood the UI event loop). Fires once right away if a frame is already
    /// pending.
    pub(crate) fn set_wake(&self, callback: Box<dyn Fn() + Send + Sync>) {
        let callback: WakeCallback = Arc::from(callback);
        let fire_now = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            state.on_frame = Some(Arc::clone(&callback));
            state.pending
        };
        if fire_now {
            callback();
        }
    }

    /// Drops any registered wake callback (for example when the GL context tears down).
    pub(crate) fn clear_wake(&self) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .on_frame = None;
    }

    /// Marks a frame as pending and wakes any waiter or edge-triggered UI callback.
    pub(crate) fn notify(&self) {
        let callback = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            let was_pending = state.pending;
            state.pending = true;
            self.wake.notify_one();
            if was_pending {
                None
            } else {
                state.on_frame.clone()
            }
        };
        if let Some(callback) = callback {
            callback();
        }
    }

    /// Whether a frame is waiting, without taking the notification.
    pub(crate) fn is_pending(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pending
    }

    /// Forgets a waiting notification (the frame it announced is being drawn).
    pub(crate) fn clear(&self) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pending = false;
    }

    /// Waits up to `timeout` for a notification; true when one arrived.
    pub(crate) fn wait(&self, timeout: Duration) -> bool {
        let guard = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let (mut guard, _) = self
            .wake
            .wait_timeout_while(guard, timeout, |state| !state.pending)
            .unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut guard.pending)
    }
}

/// A render context (software or OpenGL) bound to one mpv instance.
pub(crate) struct MpvRenderContext {
    handle: Arc<MpvHandle>,
    raw: *mut c_void,
    signal: *const FrameSignal,
    /// Open DRM render node kept alive for libva zero-copy interop on Linux. Rust executes
    /// `Drop::drop` (which frees `raw` first) before dropping struct fields, so this fd stays open
    /// for the entire lifetime of `raw` and closes cleanly afterwards.
    #[cfg(target_os = "linux")]
    _drm_render_node: Option<std::fs::File>,
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
        Ok(Self::adopt(
            handle,
            raw,
            signal,
            #[cfg(target_os = "linux")]
            None,
        ))
    }

    /// Takes over a freshly created render context `raw` and registers the frame callback.
    fn adopt(
        handle: Arc<MpvHandle>,
        raw: *mut c_void,
        signal: Arc<FrameSignal>,
        #[cfg(target_os = "linux")] drm_render_node: Option<std::fs::File>,
    ) -> Self {
        let signal = Arc::into_raw(signal);
        if let Some(api) = handle.api().render_api() {
            // SAFETY: `raw` is a live render context; `signal` stays valid until `Drop` reclaims it.
            unsafe { (api.set_update_callback)(raw, Some(on_update), signal.cast_mut().cast()) };
        }
        Self {
            handle,
            raw,
            signal,
            #[cfg(target_os = "linux")]
            _drm_render_node: drm_render_node,
        }
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn frame_signal_coalesces_wakes_until_cleared_and_stops_after_clear_wake() {
        let signal = FrameSignal::default();
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&wakes);
        signal.set_wake(Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        }));

        signal.notify();
        signal.notify();
        assert_eq!(wakes.load(Ordering::SeqCst), 1, "coalesced while pending");
        assert!(signal.is_pending());

        signal.clear();
        signal.notify();
        assert_eq!(wakes.load(Ordering::SeqCst), 2, "re-armed after clear");

        signal.clear();
        signal.clear_wake();
        signal.notify();
        assert_eq!(wakes.load(Ordering::SeqCst), 2, "silent after clear_wake");
    }

    #[test]
    fn set_wake_fires_immediately_when_a_frame_is_already_pending() {
        let signal = FrameSignal::default();
        signal.notify();
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&wakes);
        signal.set_wake(Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stress_frame_signal_notify_never_deadlocks_when_callback_inspects_signal() {
        let signal = Arc::new(FrameSignal::default());
        let observed = Arc::new(AtomicUsize::new(0));
        {
            let sig = Arc::clone(&signal);
            let obs = Arc::clone(&observed);
            signal.set_wake(Box::new(move || {
                // Re-entering `is_pending` from inside the wake callback proves `notify` dropped
                // its mutex before invoking the callback.
                if sig.is_pending() {
                    obs.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }
        let notifier = {
            let sig = Arc::clone(&signal);
            std::thread::spawn(move || {
                for _ in 0..2_000 {
                    sig.notify();
                    std::thread::yield_now();
                }
            })
        };
        let clearer = {
            let sig = Arc::clone(&signal);
            std::thread::spawn(move || {
                for _ in 0..2_000 {
                    sig.clear();
                    std::thread::yield_now();
                }
            })
        };
        assert!(notifier.join().is_ok());
        assert!(clearer.join().is_ok());
        assert!(observed.load(Ordering::Relaxed) >= 1);
    }
}
