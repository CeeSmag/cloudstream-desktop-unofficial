use egui::{Align2, FontId, Sense, Vec2};

use crate::navigation::Tab;
use crate::theme;

/// Height of the bottom navigation bar.
pub(crate) const HEIGHT: f32 = 56.0;

/// Draws the bottom navigation; returns the tab the user clicked.
pub(crate) fn show(ui: &mut egui::Ui, current: Tab) -> Option<Tab> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        let width = ui.available_width() / Tab::ALL.len() as f32;
        for tab in Tab::ALL {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, HEIGHT), Sense::click());
            let color = if tab == current {
                theme::PRIMARY
            } else {
                theme::MUTED_TEXT
            };
            ui.painter().text(
                rect.center() - Vec2::new(0.0, 8.0),
                Align2::CENTER_CENTER,
                tab.glyph(),
                FontId::proportional(20.0),
                color,
            );
            ui.painter().text(
                rect.center() + Vec2::new(0.0, 14.0),
                Align2::CENTER_CENTER,
                tab.label(),
                FontId::proportional(11.0),
                color,
            );
            if response.clicked() {
                clicked = Some(tab);
            }
        }
    });
    clicked
}
