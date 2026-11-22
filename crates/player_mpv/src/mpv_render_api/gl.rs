//! libmpv's render API in its OpenGL flavour: mpv draws into a framebuffer of the app's GL context.
//!
//! Every call here must be made on the thread that owns that context, with it current.
#![allow(unsafe_code)]

use std::ffi::{c_char, c_int, c_void, CStr, CString};

use player_api::{GlLoader, PlayerError};

use super::{FrameSignal, MpvRenderContext, RenderParam, PARAM_API_TYPE};
use crate::mpv_api::MpvHandle;
use std::sync::Arc;

const PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const PARAM_OPENGL_FBO: c_int = 3;
const PARAM_FLIP_Y: c_int = 4;

/// `mpv_opengl_init_params` from `render_gl.h`.
#[repr(C)]
struct OpenGlInitParams {
    get_proc_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    get_proc_address_ctx: *mut c_void,
}

/// `mpv_opengl_fbo` from `render_gl.h`.
#[repr(C)]
struct OpenGlFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

/// Looks a GL function up for mpv through the app's loader.
unsafe extern "C" fn resolve(context: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: `context` is the address of the `GlLoader` reference `create_gl` keeps alive for the
    // whole `mpv_render_context_create` call, the only time mpv calls this.
    let loader = unsafe { &*context.cast::<GlLoader<'_>>() };
    // SAFETY: mpv passes a NUL-terminated function name.
    let Ok(name) = unsafe { CStr::from_ptr(name) }.to_str() else {
        return std::ptr::null_mut();
    };
    loader(name).cast_mut()
}

impl MpvRenderContext {
    /// Creates an OpenGL render context in the current GL context; `signal` is notified for every
    /// new frame.
    pub(crate) fn create_gl(
        handle: Arc<MpvHandle>,
        signal: Arc<FrameSignal>,
        loader: GlLoader<'_>,
    ) -> Result<Self, PlayerError> {
        let api = handle
            .api()
            .render_api()
            .ok_or_else(|| PlayerError::Unavailable("this libmpv has no render API".to_owned()))?;
        let kind = CString::new("opengl")
            .map_err(|_| PlayerError::Unavailable("bad api type".to_owned()))?;
        let mut init = OpenGlInitParams {
            get_proc_address: resolve,
            get_proc_address_ctx: std::ptr::from_ref(&loader).cast_mut().cast(),
        };
        let mut params = [
            RenderParam {
                kind: PARAM_API_TYPE,
                data: kind.as_ptr().cast_mut().cast(),
            },
            RenderParam {
                kind: PARAM_OPENGL_INIT_PARAMS,
                data: (&raw mut init).cast(),
            },
            RenderParam {
                kind: 0,
                data: std::ptr::null_mut(),
            },
        ];
        let mut raw: *mut c_void = std::ptr::null_mut();
        // SAFETY: `handle.raw()` is a live initialised mpv handle; `params` is a terminated array
        // whose pointers (`kind`, `init`, and the loader behind `get_proc_address_ctx`) outlive the
        // call; the GL context is current on this thread (the caller's contract).
        let code = unsafe { (api.create)(&raw mut raw, handle.raw(), params.as_mut_ptr()) };
        if code < 0 || raw.is_null() {
            return Err(PlayerError::Command {
                command: "render_context_create (opengl)".to_owned(),
                reason: handle.describe(code),
            });
        }
        Ok(Self::adopt(handle, raw, signal))
    }

    /// Draws the current frame into framebuffer `fbo` (`width` x `height`).
    pub(crate) fn render_gl(&self, fbo: u32, width: u32, height: u32) -> Result<(), PlayerError> {
        let api = self
            .handle
            .api()
            .render_api()
            .ok_or_else(|| PlayerError::Unavailable("render API vanished".to_owned()))?;
        let too_large = |value: u32| PlayerError::Command {
            command: "render (opengl)".to_owned(),
            reason: format!("{value} does not fit a GL size"),
        };
        let mut target = OpenGlFbo {
            fbo: c_int::try_from(fbo).map_err(|_| too_large(fbo))?,
            w: c_int::try_from(width).map_err(|_| too_large(width))?,
            h: c_int::try_from(height).map_err(|_| too_large(height))?,
            internal_format: 0, // unknown: mpv assumes a usable format
        };
        // Textures are drawn with their origin at the bottom left, as GL expects: no flip.
        let mut flip: c_int = 0;
        let mut params = [
            RenderParam {
                kind: PARAM_OPENGL_FBO,
                data: (&raw mut target).cast(),
            },
            RenderParam {
                kind: PARAM_FLIP_Y,
                data: (&raw mut flip).cast(),
            },
            RenderParam {
                kind: 0,
                data: std::ptr::null_mut(),
            },
        ];
        // SAFETY: `raw` is a live render context owned by `self`; both parameters point at data
        // valid for the call; the GL context is current on this thread (the caller's contract).
        let code = unsafe { (api.render)(self.raw, params.as_mut_ptr()) };
        if code < 0 {
            return Err(PlayerError::Command {
                command: "render (opengl)".to_owned(),
                reason: self.handle.describe(code),
            });
        }
        Ok(())
    }
}
