//! Picture output straight into the app's OpenGL context (plan step 2e, `docs/MEMORY.md`).
//!
//! The software path renders each picture into memory and the UI copies it again: two buffers of
//! 3.5 MiB at 1280x720, 30-60 times a second. With a GL target the player renders into a texture
//! the UI shows as it is, and can decode on the GPU. All three calls must be made on the thread
//! that owns the GL context, with that context current.

use std::ffi::c_void;

use crate::PlayerError;

/// Finds an OpenGL function by name (what the windowing system's `get_proc_address` does).
pub type GlLoader<'a> = &'a dyn Fn(&str) -> *const c_void;

/// A picture the player drew into an OpenGL texture it owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlPicture {
    /// The texture's name in the app's GL context. Valid until the next `render` or `detach`.
    pub texture: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// True when this call drew a new picture (or the size changed): the app should show it. False
    /// when it is the picture shown before, so the app need not redraw.
    pub fresh: bool,
}

/// A player that can draw into an OpenGL texture it owns, for the app to show as it is.
pub trait GlVideoTarget: Send + Sync {
    /// Creates the player's render context in the current GL context. Called once, when the
    /// context exists; until then the player has nowhere to draw.
    fn attach(&self, loader: GlLoader<'_>) -> Result<(), PlayerError>;

    /// Whether the player has a picture the app has not drawn yet. Callable from any thread: the
    /// app asks it between frames to decide whether to request a redraw.
    fn frame_pending(&self) -> bool;

    /// Registers a callback invoked from mpv's thread whenever a new picture becomes ready to draw
    /// (coalesced while one is already pending). The callback must be fast and must not call back
    /// into the player.
    fn set_frame_wake(&self, wake: Box<dyn Fn() + Send + Sync>) {
        let _ = wake;
    }

    /// Draws the newest picture at `width` x `height`; `None` while there is none yet.
    fn render(&self, width: u32, height: u32) -> Result<Option<GlPicture>, PlayerError>;

    /// Frees the render context and the texture; the GL context must still be current. Called
    /// when the GL context is about to go away.
    fn detach(&self);
}
