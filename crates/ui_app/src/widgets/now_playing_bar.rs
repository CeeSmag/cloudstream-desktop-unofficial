use egui::RichText;
use player_api::{PlaybackState, PlaybackStatus};

use crate::actions::UiAction;
use crate::theme;

/// How far the skip buttons jump.
const SKIP_MS: i64 = 10_000;

/// Formats milliseconds as `m:ss` or `h:mm:ss`.
pub(crate) fn format_time(milliseconds: i64) -> String {
    let total = (milliseconds / 1000).max(0);
    let (hours, minutes, seconds) = (total / 3600, (total / 60) % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Draws the playback controls for the mpv window; returns what the user pressed.
pub(crate) fn show(ui: &mut egui::Ui, title: &str, status: &PlaybackStatus) -> Option<UiAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong().color(theme::TEXT));
        ui.label(RichText::new(describe(status)).color(theme::MUTED_TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("\u{25A0}").on_hover_text("Stop").clicked() {
                action = Some(UiAction::StopPlayback);
            }
            if ui.button("+10s").clicked() {
                action = Some(UiAction::SeekBy(SKIP_MS));
            }
            let paused = status.state == PlaybackState::Paused;
            if ui
                .button(if paused { "\u{25B6}" } else { "\u{23F8}" })
                .clicked()
            {
                action = Some(UiAction::SetPaused(!paused));
            }
            if ui.button("-10s").clicked() {
                action = Some(UiAction::SeekBy(-SKIP_MS));
            }
        });
    });
    if status.duration_ms > 0 {
        let fraction = status.position_ms as f32 / status.duration_ms as f32;
        ui.add(
            egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
                .desired_height(4.0)
                .fill(theme::PRIMARY),
        );
    }
    action
}

fn describe(status: &PlaybackStatus) -> String {
    match &status.state {
        PlaybackState::Idle => "Idle".to_owned(),
        PlaybackState::Loading => "Loading\u{2026}".to_owned(),
        PlaybackState::Playing | PlaybackState::Paused => {
            format!(
                "{} / {}",
                format_time(status.position_ms),
                format_time(status.duration_ms)
            )
        }
        PlaybackState::Ended => "Finished".to_owned(),
        PlaybackState::Failed(reason) => format!("Failed: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_time_uses_minutes_and_seconds_under_an_hour() {
        assert_eq!(format_time(0), "0:00");
        assert_eq!(format_time(65_000), "1:05");
    }

    #[test]
    fn format_time_adds_hours_and_clamps_negative_values() {
        assert_eq!(format_time(3_725_000), "1:02:05");
        assert_eq!(format_time(-5), "0:00");
    }
}
