use egui::{Color32, Rect, RichText, Sense, Stroke, UiBuilder, Vec2};
use player_api::{PlaybackState, PlaybackStatus, TrackKind};

use crate::actions::UiAction;
use crate::app_model::PlayerMenu;
use crate::theme;
use crate::widgets::buttons;
use crate::widgets::glass;
use crate::widgets::now_playing_bar::format_time;

use super::player_menu;
use super::player_screen::{self, PlayerScene};

/// Height of the bottom glass panel.
pub(crate) const PANEL_HEIGHT: f32 = 96.0;
/// How far the skip buttons jump.
pub(crate) const SKIP_MS: i64 = 10_000;

/// The rectangle of the bottom glass panel.
pub(crate) fn bottom_rect(area: Rect) -> Rect {
    let width = (area.width() - 48.0).min(900.0);
    Rect::from_center_size(
        egui::pos2(area.center().x, area.bottom() - 24.0 - PANEL_HEIGHT / 2.0),
        Vec2::new(width, PANEL_HEIGHT),
    )
}

/// Renders the bottom transport controls and track menus.
pub(crate) fn show_bottom_panel(
    ui: &mut egui::Ui,
    area: Rect,
    scene: &PlayerScene<'_>,
    actions: &mut Vec<UiAction>,
) {
    let rect = bottom_rect(area);
    glass::paint(
        ui,
        rect,
        28,
        player_screen::backdrop(ui, scene, area).as_ref(),
    );
    ui.scope_builder(
        UiBuilder::new().max_rect(rect.shrink2(Vec2::new(22.0, 12.0))),
        |ui| {
            seek_bar(ui, scene.status, actions);
            ui.add_space(4.0);
            transport(ui, scene, actions);
        },
    );
    player_menu::show(
        ui,
        player_menu::Geometry {
            area,
            controls: rect,
        },
        scene,
        actions,
    );
}

fn seek_bar(ui: &mut egui::Ui, status: &PlaybackStatus, actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format_time(status.position_ms))
                .size(12.0)
                .color(theme::MUTED_TEXT),
        );
        let width = (ui.available_width() - 60.0).max(40.0);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::click_and_drag());
        paint_seek_track(ui, rect, status, response.hovered() || response.dragged());
        if let (true, Some(position)) = (
            response.clicked() || response.dragged(),
            response.interact_pointer_pos(),
        ) {
            let target = ((position.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            actions.push(UiAction::SeekTo(
                (target * status.duration_ms as f32) as i64,
            ));
        }
        ui.label(
            RichText::new(format_time(status.duration_ms))
                .size(12.0)
                .color(theme::MUTED_TEXT),
        );
    });
}

fn paint_seek_track(ui: &egui::Ui, rect: Rect, status: &PlaybackStatus, active: bool) {
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.5));
    ui.painter()
        .rect_filled(track, 2.5, Color32::from_white_alpha(50));
    let fraction = if status.duration_ms > 0 {
        (status.position_ms as f32 / status.duration_ms as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = Rect::from_min_size(
        track.min,
        Vec2::new(track.width() * fraction, track.height()),
    );
    ui.painter().rect_filled(filled, 2.5, theme::ACCENT);
    let thumb_radius = if active { 7.0 } else { 5.5 };
    let thumb_center = egui::pos2(filled.right(), track.center().y);
    if active {
        ui.painter()
            .circle_filled(thumb_center, thumb_radius + 3.0, theme::ACCENT_SOFT);
    }
    ui.painter()
        .circle_filled(thumb_center, thumb_radius, Color32::WHITE);
}

fn transport(ui: &mut egui::Ui, scene: &PlayerScene<'_>, actions: &mut Vec<UiAction>) {
    ui.columns(3, |columns| {
        if let [left, centre, right] = columns {
            left.horizontal(|ui| menu_buttons(ui, scene, actions));
            centre.horizontal(|ui| playback_buttons(ui, scene.status, actions));
            right.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if buttons::secondary(ui, "Stop").clicked() {
                    actions.push(UiAction::StopPlayback);
                }
                if buttons::secondary(ui, "Fullscreen").clicked() {
                    actions.push(UiAction::ToggleFullscreen);
                }
            });
        }
    });
}

fn playback_buttons(ui: &mut egui::Ui, status: &PlaybackStatus, actions: &mut Vec<UiAction>) {
    let paused = status.state == PlaybackState::Paused;
    ui.add_space((ui.available_width() / 2.0 - 120.0).max(0.0));
    if buttons::secondary(ui, "-10s").clicked() {
        actions.push(UiAction::SeekBy(-SKIP_MS));
    }
    if buttons::primary(ui, if paused { "Resume" } else { "Pause" }, true).clicked() {
        actions.push(UiAction::TogglePause);
    }
    if buttons::secondary(ui, "+10s").clicked() {
        actions.push(UiAction::SeekBy(SKIP_MS));
    }
}

fn menu_buttons(ui: &mut egui::Ui, scene: &PlayerScene<'_>, actions: &mut Vec<UiAction>) {
    let count = |kind| {
        scene
            .tracks
            .iter()
            .filter(|track| track.kind == kind)
            .count()
    };
    let entries = [
        (PlayerMenu::Audio, "Audio", count(TrackKind::Audio) > 1),
        (
            PlayerMenu::Subtitles,
            "Subtitles",
            count(TrackKind::Subtitle) > 0,
        ),
        (
            PlayerMenu::Sources,
            "Sources",
            scene.sources.links.len() > 1,
        ),
    ];
    for (menu, label, available) in entries {
        if available && menu_button(ui, label, scene.menu == Some(menu)).clicked() {
            actions.push(UiAction::TogglePlayerMenu(menu));
        }
    }
}

fn menu_button(ui: &mut egui::Ui, label: &str, open: bool) -> egui::Response {
    let text = RichText::new(label).strong().size(13.5).color(if open {
        Color32::from_rgb(0x0c, 0x0c, 0x16)
    } else {
        theme::TEXT
    });
    let fill = if open {
        Color32::WHITE
    } else {
        theme::GLASS_FILL
    };
    let button = egui::Button::new(text)
        .fill(fill)
        .stroke(Stroke::new(1.0, theme::BORDER_SUBTLE))
        .corner_radius(theme::PILL_ROUNDING)
        .min_size(Vec2::new(44.0, 34.0));
    ui.add(button)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_rect_is_centered_and_bounded() {
        let area = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1200.0, 800.0));
        let rect = bottom_rect(area);
        assert!(rect.width() <= 900.0);
        assert_eq!(rect.height(), PANEL_HEIGHT);
    }
}
