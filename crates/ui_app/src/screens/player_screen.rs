use egui::{Color32, Key, Rect, RichText, Sense, UiBuilder, Vec2};
use player_api::{MediaTrack, PlaybackState, PlaybackStatus};

use crate::actions::UiAction;
use crate::ambient_backdrop;
use crate::app_model::{PlayerMenu, PlayerView, SourceSet};
use crate::theme;
use crate::video_presenter::VideoPresenter;
use crate::widgets::buttons;
use crate::widgets::glass::{self, Backdrop};

use super::player_controls;

/// Seconds without pointer activity before the controls fade out.
const CONTROLS_TIMEOUT: f64 = 3.0;
/// Height of the top title pill.
const TOP_PILL_HEIGHT: f32 = 44.0;

/// Everything the player screen needs to draw.
pub(crate) struct PlayerScene<'a> {
    pub(crate) title: &'a str,
    pub(crate) status: &'a PlaybackStatus,
    pub(crate) presenter: &'a VideoPresenter,
    pub(crate) tracks: &'a [MediaTrack],
    pub(crate) sources: &'a SourceSet,
    pub(crate) menu: Option<PlayerMenu>,
    /// The stream has no picture (an audio-only source); show artwork instead of a stale frame.
    pub(crate) audio_only: bool,
    /// Poster of what is playing, used for the audio-only view.
    pub(crate) poster_url: Option<&'a str>,
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
        player_controls::show_bottom_panel(ui, area, scene, actions);
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
    if scene.audio_only {
        draw_audio_only(ui, area, scene);
        return;
    }
    let Some(texture) = scene.presenter.picture() else {
        return;
    };
    // The player renders letterboxed to the surface size we gave it, so the texture fills the area.
    egui::Image::from_texture((texture.id(), texture.size_vec2())).paint_at(ui, area);
}

/// Artwork over a blurred wash of its colours, with an "Audio only" caption.
fn draw_audio_only(ui: &egui::Ui, area: Rect, scene: &PlayerScene<'_>) {
    if let Some(url) = scene.poster_url {
        if let Some(texture) = ambient_backdrop::texture_for(ui.ctx(), url) {
            egui::Image::from_texture((texture.id(), texture.size_vec2())).paint_at(ui, area);
        }
        ui.painter()
            .rect_filled(area, 0.0, Color32::from_black_alpha(110));
        let height = (area.height() * 0.5).min(360.0);
        let size = Vec2::new(height * 2.0 / 3.0, height);
        let poster = Rect::from_center_size(area.center() - Vec2::new(0.0, 24.0), size);
        egui::Image::new(url)
            .fit_to_exact_size(size)
            .corner_radius(theme::ROUNDING)
            .paint_at(ui, poster);
    }
    ui.painter().text(
        area.center() + Vec2::new(0.0, area.height() * 0.25 + 16.0),
        egui::Align2::CENTER_CENTER,
        "Audio only",
        egui::FontId::proportional(18.0),
        theme::MUTED_TEXT,
    );
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
            actions.push(UiAction::SeekBy(-player_controls::SKIP_MS));
        }
        if input.key_pressed(Key::ArrowRight) {
            actions.push(UiAction::SeekBy(player_controls::SKIP_MS));
        }
        if input.key_pressed(Key::F) {
            actions.push(UiAction::ToggleFullscreen);
        }
        if input.key_pressed(Key::Escape) {
            actions.push(menu.map_or(UiAction::ClosePlayer, UiAction::TogglePlayerMenu));
        }
    });
}

/// What the glass panels blur: the current picture, or the artwork for audio-only sources.
pub(super) fn backdrop(ui: &egui::Ui, scene: &PlayerScene<'_>, area: Rect) -> Option<Backdrop> {
    let texture = if scene.audio_only {
        scene
            .poster_url
            .and_then(|url| ambient_backdrop::texture_for(ui.ctx(), url))
    } else {
        scene.presenter.blurred().cloned()
    };
    texture.map(|texture| Backdrop { texture, area })
}

fn top_pill(ui: &mut egui::Ui, area: Rect, scene: &PlayerScene<'_>, actions: &mut Vec<UiAction>) {
    let rect = Rect::from_min_size(
        area.min + Vec2::new(24.0, 20.0),
        Vec2::new((area.width() - 48.0).min(720.0), TOP_PILL_HEIGHT),
    );
    glass::paint(ui, rect, 22, backdrop(ui, scene, area).as_ref());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_pill_height_is_standard() {
        assert_eq!(TOP_PILL_HEIGHT, 44.0);
    }
}
