use egui::{Align2, Color32, FontId, Rect, Sense, UiBuilder, Vec2};
use player_api::{MediaTrack, TrackKind};

use crate::actions::UiAction;
use crate::app_model::PlayerMenu;
use crate::theme;
use crate::widgets::glass;

use super::player_screen::PlayerScene;

/// Width of the pop-up menu.
const MENU_WIDTH: f32 = 360.0;
/// Height of one menu row.
const ROW_HEIGHT: f32 = 40.0;
/// Tallest the menu grows before it scrolls.
const MAX_HEIGHT: f32 = 340.0;

/// The screen rectangles the menu is placed relative to.
#[derive(Clone, Copy)]
pub(crate) struct Geometry {
    /// The whole video area.
    pub(crate) area: Rect,
    /// The bottom control panel; the menu opens above it.
    pub(crate) controls: Rect,
}

/// One selectable line in a menu.
struct MenuRow {
    label: String,
    selected: bool,
    action: UiAction,
}

/// The rows of `menu` for the current tracks and sources.
fn rows_for(menu: PlayerMenu, scene: &PlayerScene<'_>) -> Vec<MenuRow> {
    match menu {
        PlayerMenu::Audio => {
            track_rows(scene.tracks, TrackKind::Audio, "Off", UiAction::SelectAudio)
        }
        PlayerMenu::Subtitles => track_rows(
            scene.tracks,
            TrackKind::Subtitle,
            "Off",
            UiAction::SelectSubtitle,
        ),
        PlayerMenu::Sources => scene
            .sources
            .links
            .iter()
            .enumerate()
            .map(|(index, link)| MenuRow {
                label: app_services::source_label(link),
                selected: index == scene.sources.selected,
                action: UiAction::SwitchSource(index),
            })
            .collect(),
    }
}

/// `Off` plus one row per track of `kind`; "Off" is selected when none of them is.
fn track_rows(
    tracks: &[MediaTrack],
    kind: TrackKind,
    off: &str,
    select: fn(Option<i64>) -> UiAction,
) -> Vec<MenuRow> {
    let of_kind: Vec<&MediaTrack> = tracks.iter().filter(|track| track.kind == kind).collect();
    let mut rows = vec![MenuRow {
        label: off.to_owned(),
        selected: of_kind.iter().all(|track| !track.selected),
        action: select(None),
    }];
    rows.extend(of_kind.into_iter().map(|track| MenuRow {
        label: track_label(track),
        selected: track.selected,
        action: select(Some(track.id)),
    }));
    rows
}

fn track_label(track: &MediaTrack) -> String {
    if track.external && track.title.is_empty() && track.language.is_empty() {
        format!("Subtitle file {}", track.id)
    } else {
        track.label()
    }
}

/// Draws the open menu as a glass panel above the bottom controls and returns nothing; choices
/// are pushed as actions. Clicking outside both the menu and the control panel closes it.
pub(crate) fn show(
    ui: &mut egui::Ui,
    geometry: Geometry,
    scene: &PlayerScene<'_>,
    actions: &mut Vec<UiAction>,
) {
    let Geometry { area, controls } = geometry;
    let Some(menu) = scene.menu else { return };
    let rows = rows_for(menu, scene);
    let height = (rows.len() as f32 * ROW_HEIGHT + 20.0).min(MAX_HEIGHT);
    let rect = Rect::from_min_size(
        egui::pos2(
            controls.right() - MENU_WIDTH,
            controls.top() - 12.0 - height,
        ),
        Vec2::new(MENU_WIDTH, height),
    );
    let backdrop = super::player_screen::backdrop(ui, scene, area);
    glass::paint(ui, rect, 22, backdrop.as_ref());
    ui.scope_builder(UiBuilder::new().max_rect(rect.shrink(10.0)), |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for row in &rows {
                if row_button(ui, row) {
                    actions.push(row.action.clone());
                }
            }
        });
    });
    close_on_outside_click(ui, (rect, controls), menu, actions);
}

fn row_button(ui: &mut egui::Ui, row: &MenuRow) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), ROW_HEIGHT - 4.0),
        Sense::click(),
    );
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 12, Color32::from_white_alpha(28));
    }
    let color = if row.selected {
        Color32::WHITE
    } else {
        theme::TEXT
    };
    ui.painter().text(
        rect.left_center() + Vec2::new(12.0, 0.0),
        Align2::LEFT_CENTER,
        &row.label,
        FontId::proportional(15.0),
        color,
    );
    if row.selected {
        ui.painter().text(
            rect.right_center() - Vec2::new(12.0, 0.0),
            Align2::RIGHT_CENTER,
            "\u{2713}",
            FontId::proportional(16.0),
            theme::ACCENT,
        );
    }
    response.clicked()
}

fn close_on_outside_click(
    ui: &egui::Ui,
    rects: (Rect, Rect),
    open: PlayerMenu,
    actions: &mut Vec<UiAction>,
) {
    let (menu, controls) = rects;
    let pressed_at = ui.input(|input| {
        input
            .pointer
            .press_origin()
            .filter(|_| input.pointer.any_pressed())
    });
    if let Some(position) = pressed_at {
        if !menu.contains(position) && !controls.contains(position) {
            actions.push(UiAction::TogglePlayerMenu(open));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, kind: TrackKind, selected: bool) -> MediaTrack {
        MediaTrack {
            id,
            kind,
            language: "eng".into(),
            title: String::new(),
            selected,
            external: false,
        }
    }

    #[test]
    fn off_is_selected_when_no_subtitle_is_showing() {
        let tracks = [
            track(1, TrackKind::Subtitle, false),
            track(1, TrackKind::Audio, true),
        ];
        let rows = track_rows(
            &tracks,
            TrackKind::Subtitle,
            "Off",
            UiAction::SelectSubtitle,
        );
        assert_eq!(rows.len(), 2, "Off plus the one subtitle, audio excluded");
        assert!(rows[0].selected && !rows[1].selected);
    }

    #[test]
    fn off_is_not_selected_when_a_subtitle_is_showing() {
        let tracks = [track(3, TrackKind::Subtitle, true)];
        let rows = track_rows(
            &tracks,
            TrackKind::Subtitle,
            "Off",
            UiAction::SelectSubtitle,
        );
        assert!(!rows[0].selected && rows[1].selected);
        assert!(matches!(rows[1].action, UiAction::SelectSubtitle(Some(3))));
    }
}
