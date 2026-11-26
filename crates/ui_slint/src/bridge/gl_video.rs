//! Zero-copy video in the window: the player draws into a texture of the window's OpenGL context
//! and the picture is shown as that texture, never copied (plan step 2e, `docs/MEMORY.md`).
//!
//! Slint calls the rendering notifier on the thread that owns the GL context, with it current, so
//! this is where the player's render context is attached, asked for pictures and detached. If the
//! window has no OpenGL, the shared `gl_video` flag is cleared before the first video builds the
//! player, which then uses the software path as before.
#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::ffi::{c_void, CString};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use app_services::UiServices;
use player_api::{GlPicture, GlVideoTarget};
use slint::{
    BorrowedOpenGLTextureBuilder, BorrowedOpenGLTextureOrigin, ComponentHandle, GraphicsAPI, Image,
    RenderingState,
};

use crate::CloudStream;

use super::bridge_context::BridgeContext;

/// What the notifier and the frame timer share (both run on the UI thread).
#[derive(Default)]
pub(crate) struct GlVideoState {
    /// The player's GL target once attached.
    target: RefCell<Option<Arc<dyn GlVideoTarget>>>,
    /// Attaching failed: do not try again every frame.
    failed: Cell<bool>,
    /// The picture last handed to the window: texture, width, height.
    shown: Cell<Option<(u32, u32, u32)>>,
}

impl GlVideoState {
    /// Whether the player's render context is attached.
    pub(crate) fn is_attached(&self) -> bool {
        self.target.borrow().is_some()
    }

    /// Forgets the picture shown, so the next one is handed over again.
    pub(crate) fn forget_picture(&self) {
        self.shown.set(None);
    }
}

/// What the rendering notifier needs; it lives as long as the window's renderer.
struct Notifier {
    state: Rc<GlVideoState>,
    services: Rc<UiServices>,
    weak: slint::Weak<CloudStream>,
}

/// Installs the rendering notifier when zero-copy video was asked for.
pub(crate) fn install(ctx: &BridgeContext, ui: &CloudStream) {
    if !ctx.services.gl_video.load(Ordering::SeqCst) {
        return;
    }
    let notifier = Notifier {
        state: Rc::clone(&ctx.gl),
        services: Rc::clone(&ctx.services),
        weak: ui.as_weak(),
    };
    let installed = ui
        .window()
        .set_rendering_notifier(move |rendering, api| notifier.on_rendering(rendering, api));
    match installed {
        Ok(()) => {
            tracing::info!("zero-copy video requested: waiting for the window's OpenGL context")
        }
        Err(error) => {
            tracing::warn!(%error, "this window cannot hand out its OpenGL context; using software video");
            ctx.services.gl_video.store(false, Ordering::SeqCst);
        }
    }
}

impl Notifier {
    fn on_rendering(&self, rendering: RenderingState, api: &GraphicsAPI<'_>) {
        match rendering {
            RenderingState::RenderingSetup => {
                if matches!(api, GraphicsAPI::NativeOpenGL { .. }) {
                    tracing::info!("the window renders with OpenGL: zero-copy video is available");
                } else {
                    tracing::warn!("the window does not render with OpenGL; using software video");
                    self.services.gl_video.store(false, Ordering::SeqCst);
                }
            }
            RenderingState::BeforeRendering => self.before_rendering(api),
            RenderingState::RenderingTeardown => {
                if let Some(target) = self.state.target.borrow_mut().take() {
                    target.detach();
                }
                self.state.shown.set(None);
            }
            _ => {}
        }
    }

    fn before_rendering(&self, api: &GraphicsAPI<'_>) {
        let GraphicsAPI::NativeOpenGL { get_proc_address } = api else {
            return;
        };
        let Some(ui) = self.weak.upgrade() else {
            return;
        };
        let state = &self.state;
        if !state.is_attached() && !attach(state, &self.services, &self.weak, get_proc_address) {
            return;
        }
        if !ui.get_player_open() {
            return;
        }
        let size = ui.window().size();
        let target = state.target.borrow().clone();
        let Some(target) = target else {
            return;
        };
        match target.render(size.width.max(2), size.height.max(2)) {
            Ok(Some(picture)) => show(state, &ui, &picture),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "zero-copy video could not draw a picture"),
        }
    }
}

/// Attaches the player's render context once the player exists; false while there is nothing to do.
fn attach(
    state: &GlVideoState,
    services: &UiServices,
    weak: &slint::Weak<CloudStream>,
    get_proc_address: &dyn Fn(&std::ffi::CStr) -> *const c_void,
) -> bool {
    if state.failed.get() {
        return false;
    }
    let Some(target) = services
        .playback
        .as_ref()
        .and_then(|playback| playback.gl_target())
    else {
        return false;
    };
    let loader = |name: &str| -> *const c_void {
        CString::new(name).map_or(std::ptr::null(), |name| get_proc_address(&name))
    };
    match target.attach(&loader) {
        Ok(()) => {
            // Wake Slint on every new mpv frame instead of waiting for the 16 ms polling timer,
            // avoiding 60 fps cadence drops and unnecessary wakeups on 24 fps streams.
            let weak = weak.clone();
            target.set_frame_wake(Box::new(move || {
                let weak = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        if ui.get_player_open() {
                            ui.window().request_redraw();
                        }
                    }
                });
            }));
            *state.target.borrow_mut() = Some(target);
            true
        }
        Err(error) => {
            tracing::error!(%error, "zero-copy video could not attach to the OpenGL context; using software fallback");
            services.gl_video.store(false, Ordering::SeqCst);
            state.failed.set(true);
            false
        }
    }
}

/// Hands the picture to the window, when it is new or its texture changed.
fn show(state: &GlVideoState, ui: &CloudStream, picture: &GlPicture) {
    let key = (picture.texture, picture.width, picture.height);
    if !picture.fresh && state.shown.get() == Some(key) {
        return;
    }
    let Some(image) = texture_image(picture) else {
        return;
    };
    ui.set_video_frame(image);
    state.shown.set(Some(key));
}

/// The player's texture as an image Slint draws without copying it.
fn texture_image(picture: &GlPicture) -> Option<Image> {
    let texture = NonZeroU32::new(picture.texture)?;
    let size = (picture.width, picture.height).into();
    // SAFETY: `texture` names a live 2D RGBA texture of the GL context Slint renders with: the
    // player made it in that same context and keeps it until its next `render` (which this code
    // calls before Slint draws) or `detach` (called at teardown, after which the image is
    // forgotten), so Slint never draws a deleted texture. mpv's picture comes out with its first row
    // at the top of the texture (seen upside down when declared bottom-left), which is declared here.
    let image = unsafe { BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(texture, size) }
        .origin(BorrowedOpenGLTextureOrigin::TopLeft)
        .build();
    Some(image)
}
