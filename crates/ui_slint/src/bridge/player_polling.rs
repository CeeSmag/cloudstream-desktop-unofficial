//! Keeps the player screen in step with the player: status a few times a second, frames at the
//! display rate.
//!
//! Frames used to be copied only on the 200 ms UI tick, so video played at 5 frames per second.
//! They now have their own 16 ms timer and are copied only when a new one exists.

use player_api::{MediaTrack, PlaybackState, TrackKind};
use slint::{ComponentHandle, Image, SharedPixelBuffer};

use crate::posters::string_model;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::playback_reactions::react_to_status;

/// Updates position, times, pause, buffering, tracks and the video surface size.
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
    ui.set_position_text(format_time(status.position_ms).into());
    ui.set_duration_text(format_time(status.duration_ms).into());
    ui.set_paused(status.state == PlaybackState::Paused);
    ui.set_buffering(status.is_buffering || status.state == PlaybackState::Loading);
    ui.set_fullscreen(ui.window().is_fullscreen());
    show_tracks_if_changed(ctx, ui, &playback.tracks());
    follow_window_size(ctx, ui);
    react_to_status(ctx, ui, &status.state);
}

/// Shows the newest video frame, if there is one the screen has not shown yet.
pub(crate) fn show_latest_frame(ctx: &BridgeContext, ui: &CloudStream) {
    let Some(playback) = &ctx.services.playback else {
        return;
    };
    if !ui.get_player_open() {
        return;
    }
    let last = ctx.session.borrow().last_frame;
    if let Some(frame) = playback.latest_frame(last) {
        ctx.session.borrow_mut().last_frame = frame.serial;
        let buffer = SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
            &frame.rgba,
            frame.width,
            frame.height,
        );
        ui.set_video_frame(Image::from_rgba8(buffer));
    }
}

/// Rebuilding the track menus every tick reset them while open; now only when the tracks change.
fn show_tracks_if_changed(ctx: &BridgeContext, ui: &CloudStream, tracks: &[MediaTrack]) {
    let labels = track_labels(tracks);
    if ctx.session.borrow().shown_tracks == labels {
        return;
    }
    ui.set_audio_track_labels(string_model(labels.0.iter().cloned()));
    ui.set_subtitle_track_labels(string_model(labels.1.iter().cloned()));
    ctx.session.borrow_mut().shown_tracks = labels;
}

/// Audio labels, and subtitle labels starting with "Off".
pub(crate) fn track_labels(tracks: &[MediaTrack]) -> (Vec<String>, Vec<String>) {
    let of = |kind: TrackKind| -> Vec<String> {
        tracks
            .iter()
            .filter(|track| track.kind == kind)
            .map(MediaTrack::label)
            .collect()
    };
    let mut subtitles = vec!["Off".to_owned()];
    subtitles.extend(of(TrackKind::Subtitle));
    (of(TrackKind::Audio), subtitles)
}

/// The player renders at the window's size (it was never told, so it rendered at its default).
fn follow_window_size(ctx: &BridgeContext, ui: &CloudStream) {
    let Some(playback) = &ctx.services.playback else {
        return;
    };
    let size = ui.window().size();
    let wanted = (size.width, size.height);
    if ctx.session.borrow().surface_size != wanted && wanted.0 > 0 && wanted.1 > 0 {
        playback.set_surface_size(wanted.0, wanted.1);
        ctx.session.borrow_mut().surface_size = wanted;
    }
}

/// `m:ss`, or `h:mm:ss` from an hour on; negative times show as 0:00.
pub(crate) fn format_time(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (hours, minutes, seconds) = (total / 3600, (total / 60) % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_time_short_and_long_videos() {
        assert_eq!(format_time(0), "0:00");
        assert_eq!(format_time(-5), "0:00");
        assert_eq!(format_time(65_400), "1:05");
        assert_eq!(format_time(3_725_000), "1:02:05");
    }

    #[test]
    fn track_labels_subtitles_start_with_off() {
        let (audio, subtitles) = track_labels(&[]);
        assert!(audio.is_empty());
        assert_eq!(subtitles, ["Off"]);
    }
}
