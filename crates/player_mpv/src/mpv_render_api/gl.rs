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
const PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;
#[cfg(target_os = "linux")]
const PARAM_DRM_DISPLAY_V2: c_int = 16;

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

/// `mpv_opengl_drm_params_v2` from `render_gl.h`: passes a DRM render node fd so libmpv can open
/// VA-API directly for zero-copy EGL dmabuf interop on Linux (Wayland and X11 alike).
#[cfg(target_os = "linux")]
#[repr(C)]
struct OpenGlDrmParamsV2 {
    fd: c_int,
    crtc_id: c_int,
    connector_id: c_int,
    atomic_request_ptr: *mut c_void,
    render_fd: c_int,
}

/// Opens the Linux DRM render node (`O_RDWR`, required by libva ioctls) for zero-copy VA-API
/// interop; returns `None` when unavailable (e.g. proprietary NVIDIA or sandboxed environments),
/// in which case libmpv falls back cleanly to its other hwdec paths.
#[cfg(target_os = "linux")]
fn open_drm_render_node() -> Option<std::fs::File> {
    let path = std::env::var_os("CS_VAAPI_DEVICE")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/dev/dri/renderD128"));
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
    {
        Ok(file) => Some(file),
        Err(error) => {
            tracing::debug!(path = %path.display(), %error, "DRM render node unavailable for VA-API zero-copy");
            None
        }
    }
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

#[cfg(target_os = "linux")]
fn drm_params_for(node: Option<&std::fs::File>) -> OpenGlDrmParamsV2 {
    use std::os::fd::AsRawFd;
    OpenGlDrmParamsV2 {
        fd: -1,
        crtc_id: 0,
        connector_id: 0,
        atomic_request_ptr: std::ptr::null_mut(),
        render_fd: node.map_or(-1, AsRawFd::as_raw_fd),
    }
}

fn gl_render_params(
    target: &mut OpenGlFbo,
    flip: &mut c_int,
    block_for_target_time: &mut c_int,
) -> [RenderParam; 4] {
    [
        RenderParam {
            kind: PARAM_OPENGL_FBO,
            data: std::ptr::from_mut(target).cast(),
        },
        RenderParam {
            kind: PARAM_FLIP_Y,
            data: std::ptr::from_mut(flip).cast(),
        },
        RenderParam {
            kind: PARAM_BLOCK_FOR_TARGET_TIME,
            data: std::ptr::from_mut(block_for_target_time).cast(),
        },
        RenderParam {
            kind: 0,
            data: std::ptr::null_mut(),
        },
    ]
}

fn gl_init_params(
    kind: &CString,
    init: &mut OpenGlInitParams,
    #[cfg(target_os = "linux")] drm: &mut OpenGlDrmParamsV2,
) -> Vec<RenderParam> {
    let mut params = vec![
        RenderParam {
            kind: PARAM_API_TYPE,
            data: kind.as_ptr().cast_mut().cast(),
        },
        RenderParam {
            kind: PARAM_OPENGL_INIT_PARAMS,
            data: std::ptr::from_mut(init).cast(),
        },
    ];
    #[cfg(target_os = "linux")]
    if drm.render_fd >= 0 {
        params.push(RenderParam {
            kind: PARAM_DRM_DISPLAY_V2,
            data: std::ptr::from_mut(drm).cast(),
        });
    }
    params.push(RenderParam {
        kind: 0,
        data: std::ptr::null_mut(),
    });
    params
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
        #[cfg(target_os = "linux")]
        let drm_render_node = open_drm_render_node();
        #[cfg(target_os = "linux")]
        let mut drm_params = drm_params_for(drm_render_node.as_ref());
        let mut params = gl_init_params(
            &kind,
            &mut init,
            #[cfg(target_os = "linux")]
            &mut drm_params,
        );
        let mut raw: *mut c_void = std::ptr::null_mut();
        // SAFETY: `handle.raw()` is a live initialised mpv handle; `params` is a terminated array
        // whose pointers (`kind`, `init`, `drm_params`, and the loader behind
        // `get_proc_address_ctx`) outlive the call; the GL context is current on this thread (the
        // caller's contract).
        let code = unsafe { (api.create)(&raw mut raw, handle.raw(), params.as_mut_ptr()) };
        if code < 0 || raw.is_null() {
            return Err(PlayerError::Command {
                command: "render_context_create (opengl)".to_owned(),
                reason: handle.describe(code),
            });
        }
        Ok(Self::adopt(
            handle,
            raw,
            signal,
            #[cfg(target_os = "linux")]
            drm_render_node,
        ))
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
            // Matches `make_texture` (`glow::RGBA8`) so mpv skips format probing on every draw.
            internal_format: glow::RGBA8 as c_int,
        };
        // No flip here: the picture lands top-row-first in the texture, and the UI declares its
        // origin as top-left (seen upside down the other way round, on a real run).
        let mut flip: c_int = 0;
        // Never sleep waiting for a presentation timestamp on the UI / GL thread.
        let mut block_for_target_time: c_int = 0;
        let mut params = gl_render_params(&mut target, &mut flip, &mut block_for_target_time);
        // SAFETY: `raw` is a live render context owned by `self`; every parameter points at data
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
