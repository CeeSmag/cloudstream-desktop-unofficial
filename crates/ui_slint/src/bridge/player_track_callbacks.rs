//! Audio and subtitle track pickers of the player.

use player_api::TrackKind;
use slint::SharedString;

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::player_callbacks::report;

pub(crate) fn wire_player_tracks(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_select_audio_track(move |label| {
        select_track(&c, TrackKind::Audio, &label);
        if let Some(ui) = c.window() {
            ui.set_audio_track_value(label);
        }
    });
    let c = ctx.clone();
    ui.on_select_subtitle_track(move |label| {
        select_track(&c, TrackKind::Subtitle, &label);
        if let Some(ui) = c.window() {
            ui.set_subtitle_track_value(label);
        }
    });
}

/// Switches to the track of `kind` labelled `label`; "Off" turns subtitles off.
fn select_track(ctx: &BridgeContext, kind: TrackKind, label: &SharedString) {
    let Some(playback) = &ctx.services.playback else {
        return;
    };
    if kind == TrackKind::Subtitle && label == "Off" {
        report("subtitles off", playback.select_track(kind, None));
        return;
    }
    let track = playback
        .tracks()
        .into_iter()
        .find(|track| track.kind == kind && track.label() == label.as_str());
    match track {
        Some(track) => report("select track", playback.select_track(kind, Some(track.id))),
        None => tracing::debug!(%label, "the picked track is gone"),
    }
}
