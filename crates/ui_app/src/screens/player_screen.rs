use egui::{Color32, Key, Rect, RichText, Sense, UiBuilder, Vec2};
use player_api::{MediaTrack, PlaybackState, PlaybackStatus, TrackKind};

use crate::actions::UiAction;
use crate::app_model::{PlayerMenu, PlayerView, SourceSet};
use crate::theme;
use crate::video_presenter::VideoPresenter;
use crate::widgets::buttons;
use crate::widgets::glass::{self, Backdrop};
use crate::widgets::now_playing_bar::format_time;

use super::player_menu;

/// Seconds without pointer activity before the controls fade out.
const CONTROLS_TIMEOUT: f64 = 3.0;
/// How far the skip buttons and arrow keys jump.
const SKIP_MS: i64 = 10_000;
/// Height of the bottom glass panel and the top title pill.
const PANEL_HEIGHT: f32 = 96.0;
const TOP_PILL_HEIGHT: f32 = 44.0;

/// Everything the player screen needs to draw.
pub(crate) struct PlayerScene<'a> {
    pub(crate) title: &'a str,
    pub(crate) status: &'a PlaybackStatus,
    pub(crate) presenter: &'a VideoPresenter,
    pub(crate) tracks: &'a [MediaTrack],
    pub(crate) sources: &'a SourceSet,
    pub(crate) menu: Option<PlayerMenu>,
}

/// Draws the embedded video with floating glass controls. Controls fade out when idle.
pub(crate) fn show(
    ui: &mut egui::Ui,
    view: &mut PlayerView,
    scene: &PlayerScene<'_>,
    actions: &mut Vec<UiAction>,
) {
    let area = ui.max_rect();
    ui.painter().rect_filled(area, 0.0, Color32::BLACK);
    draw_picture(ui, area, scene);
    // Registered before the controls so the buttons, drawn later, sit on top and get the clicks.
    let surface_double_clicked = double_clicked(ui, area);
    track_activity(ui, view, scene.status);
    handle_keys(ui, scene.menu, actions);
    if controls_visible(ui, view, scene.status) {
        top_pill(ui, area, scene, actions);
        bottom_panel(ui, area, scene, actions);
    } else {
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
    }
    if surface_double_clicked {
        actions.push(UiAction::ToggleFullscreen);
    }
    if scene.status.state == PlaybackState::Loading {
        ui.put(
            Rect::from_center_size(area.center(), Vec2::splat(48.0)),
            egui::Spinner::new().size(40.0),
        );
    }
}

fn draw_picture(ui: &egui::Ui, area: Rect, scene: &PlayerScene<'_>) {
    let Some(texture) = scene.presenter.picture() else {
        return;
    };
    // The player renders letterboxed to the surface size we gave it, so the texture fills the area.
    egui::Image::from_texture((texture.id(), texture.size_vec2())).paint_at(ui, area);
}

fn track_activity(ui: &egui::Ui, view: &mut PlayerView, status: &PlaybackStatus) {
    let now = ui.input(|input| input.time);
    let moved = ui.input(|input| {
        input.pointer.delta() != Vec2::ZERO
            || input.pointer.any_pressed()
            || !input.events.is_empty()
    });
    if view.just_opened || moved || status.state == PlaybackState::Paused {
        view.last_activity = now;
        view.just_opened = false;
    }
}

fn controls_visible(ui: &egui::Ui, view: &PlayerView, status: &PlaybackStatus) -> bool {
    let now = ui.input(|input| input.time);
    let idle = now - view.last_activity;
    if idle < CONTROLS_TIMEOUT {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
        return true;
    }
    matches!(
        status.state,
        PlaybackState::Failed(_) | PlaybackState::Ended
    )
}

fn double_clicked(ui: &egui::Ui, area: Rect) -> bool {
    let response = ui.interact(area, ui.auto_id_with("video-surface"), Sense::click());
    response.double_clicked()
}

fn handle_keys(ui: &egui::Ui, menu: Option<PlayerMenu>, actions: &mut Vec<UiAction>) {
    ui.input(|input| {
        if input.key_pressed(Key::Space) {
            actions.push(UiAction::TogglePause);
        }
        if input.key_pressed(Key::ArrowLeft) {
            actions.push(UiAction::SeekBy(-SKIP_MS));
        }
        if input.key_pressed(Key::ArrowRight) {
            actions.push(UiAction::SeekBy(SKIP_MS));
        }
        if input.key_pressed(Key::F) {
            actions.push(UiAction::ToggleFullscreen);
        }
        if input.key_pressed(Key::Escape) {
            actions.push(menu.map_or(UiAction::ClosePlayer, UiAction::TogglePlayerMenu));
        }
    });
}

fn backdrop<'a>(scene: &'a PlayerScene<'_>, area: Rect) -> Option<Backdrop<'a>> {
    scene
        .presenter
        .blurred()
        .map(|texture| Backdrop { texture, area })
}

fn top_pill(ui: &mut egui::Ui, area: Rect, scene: &PlayerScene<'_>, actions: &mut Vec<UiAction>) {
    let rect = Rect::from_min_size(
        area.min + Vec2::new(24.0, 20.0),
        Vec2::new((area.width() - 48.0).min(720.0), TOP_PILL_HEIGHT),
    );
    glass::paint(ui, rect, 22, backdrop(scene, area).as_ref());
    ui.scope_builder(
        UiBuilder::new().max_rect(rect.shrink2(Vec2::new(10.0, 4.0))),
        |ui| {
            ui.horizontal_centered(|ui| {
                if buttons::secondary(ui, "\u{2190} Back").clicked() {
                    actions.push(UiAction::ClosePlayer);
                }
                ui.add(
                    egui::Label::new(
                        RichText::new(scene.title)
                            .strong()
                            .size(16.0)
                            .color(theme::TEXT),
                    )
                    .truncate(),
                );
            });
        },
    );
}

/// The rectangle of the bottom glass panel.
fn bottom_rect(area: Rect) -> Rect {
    let width = (area.width() - 48.0).min(900.0);
    Rect::from_center_size(
        egui::pos2(area.center().x, area.bottom() - 24.0 - PANEL_HEIGHT / 2.0),
        Vec2::new(width, PANEL_HEIGHT),
    )
}

fn bottom_panel(
    ui: &mut egui::Ui,
    area: Rect,
    scene: &PlayerScene<'_>,
    actions: &mut Vec<UiAction>,
) {
    let rect = bottom_rect(area);
    glass::paint(ui, rect, 28, backdrop(scene, area).as_ref());
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
        paint_seek_track(ui, rect, status);
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

fn paint_seek_track(ui: &egui::Ui, rect: Rect, status: &PlaybackStatus) {
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 5.0));
    ui.painter()
        .rect_filled(track, 3, Color32::from_white_alpha(60));
    let fraction = if status.duration_ms > 0 {
        (status.position_ms as f32 / status.duration_ms as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = Rect::from_min_size(
        track.min,
        Vec2::new(track.width() * fraction, track.height()),
    );
    ui.painter().rect_filled(filled, 3, Color32::WHITE);
    ui.painter().circle_filled(
        egui::pos2(filled.right(), track.center().y),
        6.0,
        Color32::WHITE,
    );
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

/// Audio, Subtitles and Sources buttons; each only appears when there is something to choose.
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
    let text = RichText::new(label).strong().size(14.0).color(if open {
        Color32::BLACK
    } else {
        theme::TEXT
    });
    let fill = if open {
        Color32::WHITE
    } else {
        theme::GLASS_FILL
    };
    ui.add(
        egui::Button::new(text)
            .fill(fill)
            .corner_radius(theme::PILL_ROUNDING)
            .min_size(Vec2::new(44.0, 36.0)),
    )
}
