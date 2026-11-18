//! Reads the player's state and newest frame for the player screen.

use player_api::{MediaTrack, PlaybackState, TrackKind};
use slint::{Image, SharedPixelBuffer};

use crate::posters::string_model;
use crate::CloudStream;

use super::bridge_context::BridgeContext;

/// Copies position, tracks and the newest video frame into the player screen.
pub(crate) fn poll_player(ctx: &BridgeContext, ui: &CloudStream) {
    let Some(playback) = &ctx.services.playback else {
        return;
    };
    if !ui.get_player_open() {
        return;
    }
    let status = playback.tick();
    ui.set_position_ms(status.position_ms as f32);
    ui.set_duration_ms(status.duration_ms.max(1) as f32);
    ui.set_paused(status.state == PlaybackState::Paused);
    show_tracks(ui, &playback.tracks());
    let mut session = ctx.session.borrow_mut();
    if let Some(frame) = playback.latest_frame(session.last_frame) {
        session.last_frame = frame.serial;
        let buffer = SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
            &frame.rgba,
            frame.width,
            frame.height,
        );
        ui.set_video_frame(Image::from_rgba8(buffer));
    }
}

fn show_tracks(ui: &CloudStream, tracks: &[MediaTrack]) {
    let labels_of = |kind: TrackKind| {
        tracks
            .iter()
            .filter(move |track| track.kind == kind)
            .map(MediaTrack::label)
    };
    ui.set_audio_track_labels(string_model(labels_of(TrackKind::Audio)));
    let subtitles = std::iter::once("Off".to_owned()).chain(labels_of(TrackKind::Subtitle));
    ui.set_subtitle_track_labels(string_model(subtitles));
}
