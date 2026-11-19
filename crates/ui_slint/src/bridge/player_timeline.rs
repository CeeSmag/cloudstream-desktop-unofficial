//! The seek bar's extras on screen: the buffered stretch, chapter dividers and the hover text.

use slint::{ModelRc, SharedString, VecModel};

use crate::seek_bar::{buffered_fraction, chapter_notches, hover_text};
use crate::CloudStream;

use super::bridge_context::BridgeContext;

pub(crate) fn wire_timeline(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_describe_seek(move |ms| {
        let session = c.session.borrow();
        SharedString::from(hover_text(ms as i64, &session.chapters.0, None))
    });
}

/// Follows the player: how much is loaded and, once per stream length, its chapters.
pub(crate) fn show_timeline(
    ctx: &BridgeContext,
    ui: &CloudStream,
    status: &player_api::PlaybackStatus,
) {
    ui.set_buffered_fraction(buffered_fraction(
        status.buffered_ms,
        status.position_ms,
        status.duration_ms,
    ));
    if ctx.session.borrow().chapters.1 == status.duration_ms {
        return;
    }
    let Some(playback) = &ctx.services.playback else {
        return;
    };
    // The chapters come with the stream's length, so a new length means a new stream.
    let chapters = playback.chapters();
    let notches = chapter_notches(&chapters, status.duration_ms);
    ui.set_chapter_notches(ModelRc::new(VecModel::from(notches)));
    ctx.session.borrow_mut().chapters = (chapters, status.duration_ms);
}
