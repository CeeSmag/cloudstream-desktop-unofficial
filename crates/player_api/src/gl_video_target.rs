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

/// A player that can draw into an OpenGL framebuffer owned by the app.
pub trait GlVideoTarget: Send + Sync {
    /// Creates the player's render context in the current GL context. Called once, when the
    /// context exists; until then the player has nowhere to draw.
    fn attach(&self, loader: GlLoader<'_>) -> Result<(), PlayerError>;

    /// Draws the newest picture into `framebuffer` (`width` x `height` pixels) when there is one
    /// the app has not shown; returns whether it drew.
    fn render(&self, framebuffer: u32, width: u32, height: u32) -> Result<bool, PlayerError>;

    /// Frees the render context; the GL context must still be current. Called when the GL context
    /// is about to go away.
    fn detach(&self);
}
