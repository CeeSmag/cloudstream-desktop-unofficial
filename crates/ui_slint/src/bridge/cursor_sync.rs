//! Keeps the mouse pointer in step with the player's controls.
//!
//! The controls hide on a timer inside the player screen and no mouse event follows, so Slint's
//! `mouse-cursor` never gets a chance to hide the pointer (see `cursor_policy`). This sets it on
//! the window directly, when the controls change and on every UI tick (the tick brings the
//! pointer back when the player closes while the controls were hidden).

use crate::CloudStream;

use super::bridge_context::BridgeContext;

/// Shows or hides the pointer on the window. `None`: the window is not backed by winit (nothing
/// to hide), so the caller keeps its state as it was.
#[cfg(feature = "pointer-hiding")]
fn set_pointer_hidden(ui: &CloudStream, hidden: bool) -> Option<()> {
    use slint::winit_030::WinitWindowAccessor;
    use slint::ComponentHandle;
    ui.window()
        .with_winit_window(|window| window.set_cursor_visible(!hidden))
}

/// Built without `pointer-hiding` (the lint pass without Wayland): there is no window to set it on.
#[cfg(not(feature = "pointer-hiding"))]
fn set_pointer_hidden(_ui: &CloudStream, _hidden: bool) -> Option<()> {
    None
}

/// Sets the pointer's visibility if it differs from what the player screen needs.
pub(crate) fn sync_cursor(ctx: &BridgeContext, ui: &CloudStream) {
    let change = ctx
        .session
        .borrow()
        .cursor
        .change_for(ui.get_player_open(), ui.get_player_chrome_visible());
    let Some(hidden) = change else {
        return;
    };
    if set_pointer_hidden(ui, hidden).is_some() {
        tracing::debug!(hidden, "mouse pointer visibility changed");
        ctx.session.borrow_mut().cursor.applied(hidden);
    }
}

/// Runs [`sync_cursor`] whenever the player's controls show or hide.
pub(crate) fn wire_cursor(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_chrome_visibility_changed(move || {
        if let Some(ui) = c.window() {
            sync_cursor(&c, &ui);
        }
    });
}
