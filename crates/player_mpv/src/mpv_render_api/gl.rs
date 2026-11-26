//! libmpv's render API in its OpenGL flavour: mpv draws into a framebuffer of the app's GL context.
//!
//! Every call here must be made on the thread that owns that context, with it current.
#![allow(unsafe_code)]

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

use player_api::{GlLoader, PlayerError};

use super::{FrameSignal, MpvRenderContext, RenderParam, PARAM_API_TYPE};
use crate::mpv_api::MpvHandle;

const PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const PARAM_OPENGL_FBO: c_int = 3;
const PARAM_FLIP_Y: c_int = 4;
const PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;
#[cfg(target_os = "linux")]
const PARAM_DRM_DISPLAY_V2: c_int = 16;

const GL_VERSION: u32 = 0x1F02;
const GL_NO_ERROR: u32 = 0;
const GL_INVALID_ENUM: u32 = 0x0500;
/// `GL_GPU_DISJOINT_EXT` from `GL_EXT_disjoint_timer_query`.
///
/// Windows WGL OpenGL ES drivers (such as AMD Adrenalin) advertise `GL_EXT_disjoint_timer_query`
/// while rejecting `glGetIntegerv(GL_GPU_DISJOINT_EXT, ...)` with `GL_INVALID_ENUM`. libplacebo
/// polls this enum after GPU timer queries finish without draining the error; the next shader
/// compile (such as turning on a subtitle) then hits `GL_INVALID_ENUM` in `gl_attach_shader` and
/// aborts overlay rendering.
const GL_GPU_DISJOINT_EXT: u32 = 0x8FBB;

type GlGetString = unsafe extern "system" fn(u32) -> *const c_char;
type GlGetError = unsafe extern "system" fn() -> u32;
type GlGetIntegerv = unsafe extern "system" fn(u32, *mut c_int);

/// Real `glGetIntegerv` entry point wrapped by [`guarded_gl_get_integerv`].
static REAL_GL_GET_INTEGERV: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Context passed to [`resolve`] for the duration of `mpv_render_context_create`.
struct ResolveContext<'a> {
    loader: GlLoader<'a>,
    guard_gpu_disjoint: bool,
}

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

/// Intercepts `GL_GPU_DISJOINT_EXT` queries on OpenGL ES drivers whose `glGetIntegerv` rejects
/// that enum with `GL_INVALID_ENUM`, reporting disjoint (`1`) cleanly and forwarding every other
/// query unchanged.
unsafe extern "system" fn guarded_gl_get_integerv(pname: u32, data: *mut c_int) {
    if pname == GL_GPU_DISJOINT_EXT {
        if !data.is_null() {
            // SAFETY: OpenGL callers pass a valid writable `GLint *` for single-value `pname`s.
            unsafe { *data = 1 };
        }
        return;
    }
    let raw = REAL_GL_GET_INTEGERV.load(Ordering::Acquire);
    if !raw.is_null() {
        // SAFETY: `REAL_GL_GET_INTEGERV` is populated with the active OpenGL loader's real
        // `glGetIntegerv` entry point before `guarded_gl_get_integerv` is handed to mpv.
        let real = unsafe { std::mem::transmute::<*mut c_void, GlGetIntegerv>(raw) };
        // SAFETY: forwarded from OpenGL's `glGetIntegerv` contract.
        unsafe { real(pname, data) };
    }
}

/// Probes whether the current context is OpenGL ES and rejects `GL_GPU_DISJOINT_EXT` with
/// `GL_INVALID_ENUM`. On desktop OpenGL it returns `false` without touching GL error state; on
/// OpenGL ES it drains the error queue before and after the probe so nothing leaks to mpv.
fn probe_broken_gpu_disjoint(loader: GlLoader<'_>) -> bool {
    let get_string_ptr = loader("glGetString");
    let get_error_ptr = loader("glGetError");
    let get_integerv_ptr = loader("glGetIntegerv");
    if get_string_ptr.is_null() || get_error_ptr.is_null() || get_integerv_ptr.is_null() {
        return false;
    }
    // SAFETY: each pointer is non-null and resolved for the named standard OpenGL entry point
    // while the GL context is current (caller's contract of `create_gl`).
    let (get_string, get_error, get_integerv) = unsafe {
        (
            std::mem::transmute::<*const c_void, GlGetString>(get_string_ptr),
            std::mem::transmute::<*const c_void, GlGetError>(get_error_ptr),
            std::mem::transmute::<*const c_void, GlGetIntegerv>(get_integerv_ptr),
        )
    };
    // SAFETY: `glGetString(GL_VERSION)` returns a static NUL-terminated string or NULL.
    let version_ptr = unsafe { get_string(GL_VERSION) };
    if version_ptr.is_null() {
        return false;
    }
    // SAFETY: non-null static C string owned by the OpenGL driver.
    let Ok(version) = unsafe { CStr::from_ptr(version_ptr) }.to_str() else {
        return false;
    };
    if !version.contains("OpenGL ES") {
        return false;
    }
    // SAFETY: standard OpenGL entry points on the current context; `disjoint` is writable.
    unsafe {
        for _ in 0..8 {
            if get_error() == GL_NO_ERROR {
                break;
            }
        }
        let mut disjoint: c_int = 0;
        get_integerv(GL_GPU_DISJOINT_EXT, &raw mut disjoint);
        get_error() == GL_INVALID_ENUM
    }
}

fn make_resolve_context(loader: GlLoader<'_>) -> ResolveContext<'_> {
    let guard_gpu_disjoint = probe_broken_gpu_disjoint(loader);
    if guard_gpu_disjoint {
        tracing::warn!(
            pname = format_args!("{GL_GPU_DISJOINT_EXT:#x}"),
            "OpenGL ES driver rejects GL_GPU_DISJOINT_EXT; shielding libmpv's glGetIntegerv from GL_INVALID_ENUM"
        );
    }
    ResolveContext {
        loader,
        guard_gpu_disjoint,
    }
}

/// Looks a GL function up for mpv through the app's loader.
unsafe extern "C" fn resolve(context: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: `context` is the address of the `ResolveContext` `create_gl` keeps alive for the
    // whole `mpv_render_context_create` call, the only time mpv calls this.
    let ctx = unsafe { &*context.cast::<ResolveContext<'_>>() };
    // SAFETY: mpv passes a NUL-terminated function name.
    let Ok(name) = unsafe { CStr::from_ptr(name) }.to_str() else {
        return std::ptr::null_mut();
    };
    let raw = (ctx.loader)(name).cast_mut();
    if ctx.guard_gpu_disjoint && name == "glGetIntegerv" && !raw.is_null() {
        REAL_GL_GET_INTEGERV.store(raw, Ordering::Release);
        return (guarded_gl_get_integerv as *const c_void).cast_mut();
    }
    raw
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
        let resolve_ctx = make_resolve_context(loader);
        let mut init = OpenGlInitParams {
            get_proc_address: resolve,
            get_proc_address_ctx: std::ptr::from_ref(&resolve_ctx).cast_mut().cast(),
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::ffi::{c_char, c_int, c_void, CString};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::{
        guarded_gl_get_integerv, probe_broken_gpu_disjoint, resolve, GlGetIntegerv, ResolveContext,
        GL_GPU_DISJOINT_EXT, GL_INVALID_ENUM, GL_NO_ERROR,
    };

    const GL_MAX_TEXTURE_SIZE: u32 = 0x0D33;

    static STRESS_FORWARDED_CALLS: AtomicUsize = AtomicUsize::new(0);

    thread_local! {
        static FAKE_VERSION: Cell<&'static [u8]> = const { Cell::new(b"4.6.0\0") };
        static FAKE_REJECT_DISJOINT: Cell<bool> = const { Cell::new(false) };
        static FAKE_ERROR: Cell<u32> = const { Cell::new(GL_NO_ERROR) };
        static FAKE_INTEGERV_CALLS: Cell<u32> = const { Cell::new(0) };
    }

    unsafe extern "system" fn fake_get_string(_name: u32) -> *const c_char {
        FAKE_VERSION.with(|v| v.get().as_ptr().cast())
    }

    unsafe extern "system" fn fake_get_error() -> u32 {
        FAKE_ERROR.with(|e| e.replace(GL_NO_ERROR))
    }

    unsafe extern "system" fn fake_get_integerv(pname: u32, data: *mut c_int) {
        FAKE_INTEGERV_CALLS.with(|c| c.set(c.get() + 1));
        if pname == GL_GPU_DISJOINT_EXT {
            if FAKE_REJECT_DISJOINT.with(Cell::get) {
                FAKE_ERROR.with(|e| e.set(GL_INVALID_ENUM));
            } else if !data.is_null() {
                unsafe { *data = 0 };
            }
            return;
        }
        if pname == GL_MAX_TEXTURE_SIZE && !data.is_null() {
            STRESS_FORWARDED_CALLS.fetch_add(1, Ordering::Relaxed);
            unsafe { *data = 16384 };
        }
    }

    fn fake_loader(name: &str) -> *const c_void {
        match name {
            "glGetString" => fake_get_string as *const c_void,
            "glGetError" => fake_get_error as *const c_void,
            "glGetIntegerv" => fake_get_integerv as *const c_void,
            _ => std::ptr::null(),
        }
    }

    #[test]
    fn probe_broken_gpu_disjoint_gles_rejecting_enum_returns_true_and_shields_calls() {
        FAKE_VERSION.with(|v| v.set(b"OpenGL ES 3.2.0 AMD\0"));
        FAKE_REJECT_DISJOINT.with(|r| r.set(true));
        FAKE_ERROR.with(|e| e.set(GL_NO_ERROR));
        FAKE_INTEGERV_CALLS.with(|c| c.set(0));

        assert!(probe_broken_gpu_disjoint(&fake_loader));
        assert_eq!(FAKE_ERROR.with(Cell::get), GL_NO_ERROR);

        let ctx = ResolveContext {
            loader: &fake_loader,
            guard_gpu_disjoint: true,
        };
        let sym = CString::new("glGetIntegerv").unwrap_or_default();
        let resolved = unsafe { resolve(std::ptr::from_ref(&ctx).cast_mut().cast(), sym.as_ptr()) };
        assert_eq!(
            resolved,
            (guarded_gl_get_integerv as *const c_void).cast_mut()
        );

        let get_integerv = unsafe { std::mem::transmute::<*mut c_void, GlGetIntegerv>(resolved) };
        let calls_before = FAKE_INTEGERV_CALLS.with(Cell::get);
        let mut disjoint: c_int = 0;
        unsafe { get_integerv(GL_GPU_DISJOINT_EXT, &raw mut disjoint) };
        assert_eq!(disjoint, 1);
        assert_eq!(FAKE_ERROR.with(Cell::get), GL_NO_ERROR);
        assert_eq!(FAKE_INTEGERV_CALLS.with(Cell::get), calls_before);

        let mut max_tex: c_int = 0;
        unsafe { get_integerv(GL_MAX_TEXTURE_SIZE, &raw mut max_tex) };
        assert_eq!(max_tex, 16384);
        assert_eq!(FAKE_INTEGERV_CALLS.with(Cell::get), calls_before + 1);
    }

    #[test]
    fn probe_broken_gpu_disjoint_desktop_gl_skips_probe_and_leaves_loader_unwrapped() {
        FAKE_VERSION.with(|v| v.set(b"4.6.0 Compatibility Profile\0"));
        FAKE_REJECT_DISJOINT.with(|r| r.set(true));
        FAKE_ERROR.with(|e| e.set(GL_NO_ERROR));
        FAKE_INTEGERV_CALLS.with(|c| c.set(0));

        assert!(!probe_broken_gpu_disjoint(&fake_loader));
        assert_eq!(FAKE_INTEGERV_CALLS.with(Cell::get), 0);

        let ctx = ResolveContext {
            loader: &fake_loader,
            guard_gpu_disjoint: false,
        };
        let sym = CString::new("glGetIntegerv").unwrap_or_default();
        let resolved = unsafe { resolve(std::ptr::from_ref(&ctx).cast_mut().cast(), sym.as_ptr()) };
        assert_eq!(resolved, (fake_get_integerv as *const c_void).cast_mut());
    }

    #[test]
    fn probe_broken_gpu_disjoint_working_gles_returns_false_and_leaves_loader_unwrapped() {
        FAKE_VERSION.with(|v| v.set(b"OpenGL ES 3.2 Mesa 24.0\0"));
        FAKE_REJECT_DISJOINT.with(|r| r.set(false));
        FAKE_ERROR.with(|e| e.set(GL_NO_ERROR));

        assert!(!probe_broken_gpu_disjoint(&fake_loader));
    }

    #[test]
    fn guarded_gl_get_integerv_concurrent_resolve_and_calls_preserve_invariants() {
        const THREADS: usize = 8;
        const ITERATIONS: usize = 2_000;

        let barrier = Arc::new(Barrier::new(THREADS));
        let mut handles = Vec::with_capacity(THREADS);
        for _ in 0..THREADS {
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                FAKE_REJECT_DISJOINT.with(|r| r.set(true));
                FAKE_ERROR.with(|e| e.set(GL_NO_ERROR));
                let ctx = ResolveContext {
                    loader: &fake_loader,
                    guard_gpu_disjoint: true,
                };
                let sym = CString::new("glGetIntegerv").expect("symbol");
                barrier.wait();
                for _ in 0..ITERATIONS {
                    let resolved = unsafe {
                        resolve(std::ptr::from_ref(&ctx).cast_mut().cast(), sym.as_ptr())
                    };
                    let get_integerv =
                        unsafe { std::mem::transmute::<*mut c_void, GlGetIntegerv>(resolved) };
                    let mut disjoint: c_int = 0;
                    unsafe { get_integerv(GL_GPU_DISJOINT_EXT, &raw mut disjoint) };
                    assert_eq!(disjoint, 1);
                    assert_eq!(FAKE_ERROR.with(Cell::get), GL_NO_ERROR);

                    let mut max_tex: c_int = 0;
                    unsafe { get_integerv(GL_MAX_TEXTURE_SIZE, &raw mut max_tex) };
                    assert_eq!(max_tex, 16384);
                }
            }));
        }
        for handle in handles {
            handle.join().expect("race test thread panicked");
        }
        assert!(STRESS_FORWARDED_CALLS.load(Ordering::Relaxed) >= THREADS * ITERATIONS);
    }
}
