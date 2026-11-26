//! Zero-copy video: mpv draws into a texture of the app's OpenGL context, which the UI shows as it
//! is (plan step 2e, `docs/MEMORY.md`). No picture is ever copied into memory.
//!
//! Every method runs on the thread that owns the GL context, with it current (the contract of
//! [`GlVideoTarget`]); the mutex only makes the type `Sync`.
#![allow(unsafe_code)]

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use glow::HasContext;
use player_api::{GlLoader, GlPicture, GlVideoTarget, PlayerError};

use crate::mpv_api::MpvHandle;
use crate::mpv_render_api::{FrameSignal, MpvRenderContext};

/// The texture mpv draws into, and the framebuffer that wraps it.
struct Target {
    framebuffer: glow::Framebuffer,
    texture: glow::Texture,
    width: u32,
    height: u32,
}

/// What exists once the GL context does.
struct Attached {
    context: MpvRenderContext,
    gl: glow::Context,
    target: Option<Target>,
    /// Whether mpv has drawn at least one picture into the current texture.
    drawn: bool,
}

// SAFETY: `glow::Context` holds function pointers of one GL context. `Attached` is only touched
// under the mutex of `GlOutput`, and by contract only on the thread that owns that context.
unsafe impl Send for Attached {}

/// The GL side of one mpv player.
pub(crate) struct GlOutput {
    handle: Arc<MpvHandle>,
    signal: Arc<FrameSignal>,
    attached: Mutex<Option<Attached>>,
    /// True once `attach` failed; the player has fallen back to software rendering.
    failed: AtomicBool,
    on_attach_failed: Box<dyn Fn() + Send + Sync>,
}

impl GlOutput {
    pub(crate) fn new(
        handle: Arc<MpvHandle>,
        on_attach_failed: Box<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            handle,
            signal: Arc::new(FrameSignal::default()),
            attached: Mutex::new(None),
            failed: AtomicBool::new(false),
            on_attach_failed,
        }
    }

    /// True once attaching to the window's OpenGL context failed and software fallback took over.
    pub(crate) fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }
}

/// Whether the texture must be made again for `wanted` (a first picture, or a resized surface).
fn needs_new_target(current: Option<(u32, u32)>, wanted: (u32, u32)) -> bool {
    current != Some(wanted)
}

fn gl_error(what: &str, reason: String) -> PlayerError {
    PlayerError::Command {
        command: what.to_owned(),
        reason,
    }
}

/// Makes an RGBA texture of `width` x `height` and a framebuffer that draws into it, leaving the
/// app's GL bindings as they were.
fn create_target(gl: &glow::Context, width: u32, height: u32) -> Result<Target, PlayerError> {
    let too_large = || gl_error("gl target", "size too large".into());
    let w = i32::try_from(width).map_err(|_| too_large())?;
    let h = i32::try_from(height).map_err(|_| too_large())?;
    // SAFETY: the GL context `gl` belongs to is current on this thread (the trait's contract); the
    // objects built are owned by the returned `Target` and deleted by `delete_target`.
    let (framebuffer, texture) = unsafe {
        let previous_texture = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
        let previous_framebuffer = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
        let built = build_target(gl, w, h);
        restore_bindings(gl, previous_texture, previous_framebuffer);
        built?
    };
    Ok(Target {
        framebuffer,
        texture,
        width,
        height,
    })
}

/// The texture and the framebuffer that wraps it; leaves them bound.
///
/// # Safety
/// The GL context of `gl` must be current on this thread.
unsafe fn build_target(
    gl: &glow::Context,
    width: i32,
    height: i32,
) -> Result<(glow::Framebuffer, glow::Texture), PlayerError> {
    // SAFETY: forwarded from this function's contract.
    unsafe {
        let texture = make_texture(gl, width, height)?;
        let framebuffer = gl
            .create_framebuffer()
            .map_err(|e| gl_error("create_framebuffer", e))?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(texture),
            0,
        );
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        if status != glow::FRAMEBUFFER_COMPLETE {
            gl.delete_framebuffer(framebuffer);
            gl.delete_texture(texture);
            return Err(gl_error(
                "framebuffer",
                format!("incomplete (status {status:#x})"),
            ));
        }
        Ok((framebuffer, texture))
    }
}

/// An empty RGBA texture, left bound.
///
/// # Safety
/// The GL context of `gl` must be current on this thread.
unsafe fn make_texture(
    gl: &glow::Context,
    width: i32,
    height: i32,
) -> Result<glow::Texture, PlayerError> {
    // SAFETY: forwarded from this function's contract.
    unsafe {
        let texture = gl
            .create_texture()
            .map_err(|e| gl_error("create_texture", e))?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            width,
            height,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        for (name, value) in [
            (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
            (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, name, value as i32);
        }
        Ok(texture)
    }
}

/// Puts back the texture and framebuffer bindings the app had before we touched them.
///
/// # Safety
/// The GL context of `gl` must be current on this thread.
unsafe fn restore_bindings(gl: &glow::Context, texture: i32, framebuffer: i32) {
    let name = |id: i32| u32::try_from(id).ok().and_then(NonZeroU32::new);
    // SAFETY: forwarded from this function's contract.
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, name(texture).map(glow::NativeTexture));
        gl.bind_framebuffer(
            glow::FRAMEBUFFER,
            name(framebuffer).map(glow::NativeFramebuffer),
        );
    }
}

fn delete_target(gl: &glow::Context, target: &Target) {
    // SAFETY: the GL context is current (trait contract) and both objects were made by
    // `create_target` and are deleted exactly once, here.
    unsafe {
        gl.delete_framebuffer(target.framebuffer);
        gl.delete_texture(target.texture);
    }
}

impl GlOutput {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Attached>> {
        self.attached.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The texture for `width` x `height`, made again when the size changed. True when it is new.
    fn ready_target(attached: &mut Attached, width: u32, height: u32) -> Result<bool, PlayerError> {
        let current = attached
            .target
            .as_ref()
            .map(|target| (target.width, target.height));
        if !needs_new_target(current, (width, height)) {
            return Ok(false);
        }
        if let Some(old) = attached.target.take() {
            delete_target(&attached.gl, &old);
        }
        attached.target = Some(create_target(&attached.gl, width, height)?);
        attached.drawn = false;
        Ok(true)
    }
}

/// Creates mpv's OpenGL render context and `glow::Context`, rejecting null loaders cleanly so
/// `glow` never panics on `glGetString`.
fn setup_gl_context(
    handle: &Arc<MpvHandle>,
    signal: &Arc<FrameSignal>,
    loader: GlLoader<'_>,
) -> Result<(MpvRenderContext, glow::Context), PlayerError> {
    if loader("glGetString").is_null() {
        return Err(gl_error(
            "gl loader",
            "OpenGL loader did not provide glGetString".into(),
        ));
    }
    let context = MpvRenderContext::create_gl(Arc::clone(handle), Arc::clone(signal), loader)?;
    // SAFETY: `loader("glGetString")` is non-null and the GL context is current on this thread
    // (the trait's contract), so `glow` resolves real entry points.
    let gl = unsafe { glow::Context::from_loader_function(|name| loader(name)) };
    Ok((context, gl))
}

/// Draws into `framebuffer` while preserving the caller's `FRAMEBUFFER_BINDING` (required by
/// Slint's renderer on both Windows WGL and Linux EGL).
fn draw_preserving_fbo(
    attached: &Attached,
    framebuffer: u32,
    width: u32,
    height: u32,
) -> Result<(), PlayerError> {
    // SAFETY: the GL context is current (trait contract); read the caller's FBO binding.
    let previous = unsafe { attached.gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) };
    let drawn = attached.context.render_gl(framebuffer, width, height);
    let restore = u32::try_from(previous)
        .ok()
        .and_then(NonZeroU32::new)
        .map(glow::NativeFramebuffer);
    // SAFETY: the GL context is current (trait contract); restores the prior FBO binding.
    unsafe { attached.gl.bind_framebuffer(glow::FRAMEBUFFER, restore) };
    drawn
}

impl GlVideoTarget for GlOutput {
    fn attach(&self, loader: GlLoader<'_>) -> Result<(), PlayerError> {
        let mut attached = self.lock();
        if attached.is_some() {
            return Ok(());
        }
        let (context, gl) = match setup_gl_context(&self.handle, &self.signal, loader) {
            Ok(pair) => pair,
            Err(error) => {
                drop(attached);
                if !self.failed.swap(true, Ordering::AcqRel) {
                    tracing::warn!(
                        %error,
                        "OpenGL zero-copy video could not attach; switching to software fallback"
                    );
                    (self.on_attach_failed)();
                }
                return Err(error);
            }
        };
        tracing::info!("mpv now draws into the app's OpenGL context (zero-copy video)");
        *attached = Some(Attached {
            context,
            gl,
            target: None,
            drawn: false,
        });
        Ok(())
    }

    fn frame_pending(&self) -> bool {
        self.signal.is_pending()
    }

    fn set_frame_wake(&self, wake: Box<dyn Fn() + Send + Sync>) {
        self.signal.set_wake(wake);
    }

    fn render(&self, width: u32, height: u32) -> Result<Option<GlPicture>, PlayerError> {
        let mut guard = self.lock();
        let Some(attached) = guard.as_mut() else {
            return Ok(None);
        };
        let resized = Self::ready_target(attached, width, height)?;
        // Clear pending *before* `has_new_frame()` so an `on_update` arriving during render
        // re-arms the signal instead of being erased.
        self.signal.clear();
        let new_frame = attached.context.has_new_frame();
        let Some(target) = attached.target.as_ref() else {
            return Ok(None);
        };
        let fresh = new_frame || (resized && attached.drawn);
        if new_frame || resized {
            draw_preserving_fbo(attached, target.framebuffer.0.get(), width, height)?;
            attached.drawn = true;
        }
        if !attached.drawn {
            return Ok(None); // nothing has been played yet
        }
        Ok(Some(GlPicture {
            texture: target.texture.0.get(),
            width,
            height,
            fresh: fresh || resized,
        }))
    }

    fn detach(&self) {
        self.signal.clear_wake();
        if let Some(attached) = self.lock().take() {
            if let Some(target) = &attached.target {
                delete_target(&attached.gl, target);
            }
            tracing::info!("mpv's OpenGL render context freed");
            drop(attached); // frees the render context while the GL context is still current
        }
    }
}

impl Drop for GlOutput {
    fn drop(&mut self) {
        if let Some(attached) = self.lock().take() {
            // The GL context may already be gone, and freeing GL objects then is undefined:
            // leave them (the process is ending or the window is already closed).
            tracing::warn!(
                "the GL video output was dropped while attached; leaking its GL objects"
            );
            std::mem::forget(attached);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::needs_new_target;

    #[test]
    fn a_target_is_made_for_the_first_picture_and_again_when_the_size_changes() {
        assert!(needs_new_target(None, (1280, 720)));
        assert!(!needs_new_target(Some((1280, 720)), (1280, 720)));
        assert!(needs_new_target(Some((1280, 720)), (1920, 1080)));
    }
}
