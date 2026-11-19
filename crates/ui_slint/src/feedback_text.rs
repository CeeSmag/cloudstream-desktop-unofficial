//! The words of the player's pop-ups: what the viewer just did, and why the picture waits.
//!
//! Ported from the old app (`feedbackText.ts`, `BufferingIndicator.tsx`).

use player_api::{PlaybackState, PlaybackStatus};

/// Longest text a pop-up shows before it is cut with an ellipsis.
const MAX_POPUP_CHARS: usize = 60;

/// `+10 s` or `−10 s` for a jump forward or back.
pub(crate) fn seek_text(delta_ms: i64) -> String {
    let seconds = (delta_ms.unsigned_abs() + 500) / 1000;
    let sign = if delta_ms < 0 { '−' } else { '+' };
    format!("{sign}{seconds} s")
}

/// `Volume 55%`, or `Muted` at zero.
pub(crate) fn volume_text(percent: f32) -> String {
    if percent <= 0.0 {
        "Muted".to_owned()
    } else {
        format!("Volume {}%", percent.round())
    }
}

/// The pop-up for a chosen subtitle track; None when subtitles were switched off.
pub(crate) fn subtitle_text(label: Option<&str>) -> String {
    match label {
        Some(label) => shorten(&format!("Subtitles: {label}")),
        None => "Subtitles off".to_owned(),
    }
}

/// The pop-up for a chosen audio track.
pub(crate) fn audio_text(label: &str) -> String {
    shorten(&format!("Audio: {label}"))
}

/// The pop-up for a chosen stream source.
pub(crate) fn source_text(label: &str) -> String {
    shorten(&format!("Source: {label}"))
}

/// Cuts a text to the longest length a pop-up shows.
pub(crate) fn shorten(text: &str) -> String {
    if text.chars().count() <= MAX_POPUP_CHARS {
        return text.to_owned();
    }
    let kept: String = text.chars().take(MAX_POPUP_CHARS - 1).collect();
    format!("{kept}…")
}

/// What the top of the player says while the picture waits; empty when it does not.
pub(crate) fn waiting_text(status: &PlaybackStatus) -> String {
    match status.state {
        PlaybackState::Loading => "Loading stream…".to_owned(),
        PlaybackState::Failed(_) => String::new(),
        _ if status.is_buffering => match status.buffer_percent {
            Some(percent) if percent <= 100 => format!("Buffering {percent}%"),
            _ => "Buffering…".to_owned(),
        },
        _ => String::new(),
    }
}

/// True when a playing stream has sound but no picture.
pub(crate) fn audio_only(status: &PlaybackStatus) -> bool {
    !status.has_video && matches!(status.state, PlaybackState::Playing | PlaybackState::Paused)
}

/// Wheel distance (logical pixels; Slint makes one mouse-wheel notch 60) per percent of volume:
/// a notch is 5 %.
pub(crate) const WHEEL_PX_PER_PERCENT: f32 = 12.0;

/// Turns a wheel movement into a volume change in whole percent (up raises it). `carry` is the
/// part of earlier movements that did not make a whole percent yet, so the small steps of a
/// touchpad are not lost; the new carry comes back with the change (from the old wheelVolume.ts).
pub(crate) fn wheel_volume_delta(delta_y: f32, carry: f32) -> (f32, f32) {
    let total = carry + delta_y;
    let delta = (total / WHEEL_PX_PER_PERCENT).trunc();
    (delta, total - delta * WHEEL_PX_PER_PERCENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_volume_delta_notches_and_touchpad_steps() {
        assert_eq!(
            wheel_volume_delta(60.0, 0.0),
            (5.0, 0.0),
            "one notch up is 5 %"
        );
        assert_eq!(wheel_volume_delta(-120.0, 0.0), (-10.0, 0.0));
        let (first, carry) = wheel_volume_delta(7.0, 0.0);
        assert_eq!(first, 0.0, "a small touchpad step waits");
        let (second, carry) = wheel_volume_delta(7.0, carry);
        assert_eq!(second, 1.0, "until steps add up to a percent");
        assert!((carry - 2.0).abs() < 1e-6);
    }

    #[test]
    fn seek_text_signs_and_rounds() {
        assert_eq!(seek_text(10_000), "+10 s");
        assert_eq!(seek_text(-10_000), "−10 s");
        assert_eq!(seek_text(4_600), "+5 s");
    }

    #[test]
    fn volume_text_says_muted_at_zero() {
        assert_eq!(volume_text(0.0), "Muted");
        assert_eq!(volume_text(54.6), "Volume 55%");
    }

    #[test]
    fn track_texts_and_long_labels_are_cut() {
        assert_eq!(subtitle_text(None), "Subtitles off");
        assert_eq!(subtitle_text(Some("English")), "Subtitles: English");
        assert_eq!(audio_text("Japanese"), "Audio: Japanese");
        let long = source_text(&"x".repeat(100));
        assert_eq!(long.chars().count(), MAX_POPUP_CHARS);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn waiting_text_loading_buffering_with_and_without_percent() {
        let mut status = PlaybackStatus {
            state: PlaybackState::Loading,
            ..PlaybackStatus::default()
        };
        assert_eq!(waiting_text(&status), "Loading stream…");
        status.state = PlaybackState::Playing;
        assert_eq!(waiting_text(&status), "");
        status.is_buffering = true;
        assert_eq!(waiting_text(&status), "Buffering…");
        status.buffer_percent = Some(45);
        assert_eq!(waiting_text(&status), "Buffering 45%");
        status.state = PlaybackState::Failed("x".into());
        assert_eq!(
            waiting_text(&status),
            "",
            "a failed stream shows its error instead"
        );
    }

    #[test]
    fn audio_only_needs_a_playing_stream_without_picture() {
        let mut status = PlaybackStatus {
            state: PlaybackState::Playing,
            ..PlaybackStatus::default()
        };
        assert!(audio_only(&status));
        status.has_video = true;
        assert!(!audio_only(&status));
        status.has_video = false;
        status.state = PlaybackState::Loading;
        assert!(!audio_only(&status));
    }
}
