//! Audio and subtitle track pickers of the player.

use player_api::TrackKind;

use crate::feedback_text::{audio_text, subtitle_text};
use crate::track_menu::OFF_ID;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::player_callbacks::report;
use super::player_feedback::show_feedback;

pub(crate) fn wire_player_tracks(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_select_audio_track(move |id| select_track(&c, TrackKind::Audio, id));
    let c = ctx.clone();
    ui.on_select_subtitle_track(move |id| select_track(&c, TrackKind::Subtitle, id));
}

/// Switches to the track of `kind` with the player's id `id`; [`OFF_ID`] turns subtitles off.
fn select_track(ctx: &BridgeContext, kind: TrackKind, id: i32) {
    let Some(playback) = &ctx.services.playback else {
        return;
    };
    let id = i64::from(id);
    if kind == TrackKind::Subtitle && id == OFF_ID {
        report("subtitles off", playback.select_track(kind, None));
        tell(ctx, subtitle_text(None));
        return;
    }
    let picked = playback
        .tracks()
        .into_iter()
        .find(|track| track.kind == kind && track.id == id);
    let Some(track) = picked else {
        tracing::debug!(id, "the picked track is gone");
        return;
    };
    report("select track", playback.select_track(kind, Some(id)));
    tell(
        ctx,
        match kind {
            TrackKind::Subtitle => subtitle_text(Some(&track.label())),
            TrackKind::Audio => audio_text(&track.label()),
        },
    );
}

fn tell(ctx: &BridgeContext, text: String) {
    if let Some(ui) = ctx.window() {
        show_feedback(&ui, text, None);
    }
}
