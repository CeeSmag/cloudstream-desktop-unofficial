use egui::RichText;
use player_api::{PlaybackState, PlaybackStatus};

use crate::actions::UiAction;
use crate::theme;
use crate::widgets::buttons;

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

/// Draws the playback controls as a floating glass pill; returns what the user pressed.
pub(crate) fn show(
    ui: &mut egui::Ui,
    title: &str,
    status: &PlaybackStatus,
    embedded: bool,
) -> Option<UiAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        action = controls(ui, status);
        if embedded && buttons::secondary(ui, "Open player").clicked() {
            action = Some(UiAction::OpenPlayer);
        }
        ui.vertical(|ui| {
            ui.add(egui::Label::new(RichText::new(title).strong().color(theme::TEXT)).truncate());
            ui.label(
                RichText::new(describe(status))
                    .size(12.0)
                    .color(theme::MUTED_TEXT),
            );
        });
    });
    if status.duration_ms > 0 {
        let fraction = status.position_ms as f32 / status.duration_ms as f32;
        ui.add(
            egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
                .desired_height(4.0)
                .fill(egui::Color32::WHITE),
        );
    }
    action
}

fn controls(ui: &mut egui::Ui, status: &PlaybackStatus) -> Option<UiAction> {
    let mut action = None;
    let paused = status.state == PlaybackState::Paused;
    if buttons::secondary(ui, "-10s").clicked() {
        action = Some(UiAction::SeekBy(-SKIP_MS));
    }
    if buttons::primary(ui, if paused { "Resume" } else { "Pause" }, true).clicked() {
        action = Some(UiAction::SetPaused(!paused));
    }
    if buttons::secondary(ui, "+10s").clicked() {
        action = Some(UiAction::SeekBy(SKIP_MS));
    }
    if buttons::secondary(ui, "Stop").clicked() {
        action = Some(UiAction::StopPlayback);
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
