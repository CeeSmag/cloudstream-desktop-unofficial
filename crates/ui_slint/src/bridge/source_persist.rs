//! Saves the open title's checked sources to disk when leaving or closing the player.

use playback_session::save_open_kept;

use super::bridge_context::BridgeContext;

/// Writes the live source list for the kept title (no-op when nothing was opened).
pub(crate) fn persist_playing_sources(ctx: &BridgeContext) {
    let (kept, opened) = {
        let session = ctx.session.borrow();
        let Some(kept) = session.kept_sources.clone() else {
            return;
        };
        let opened = session.live.snapshot();
        (kept, opened)
    };
    if opened.links.is_empty() {
        return;
    }
    save_open_kept(&ctx.services.title_cache, &kept, &opened);
}
