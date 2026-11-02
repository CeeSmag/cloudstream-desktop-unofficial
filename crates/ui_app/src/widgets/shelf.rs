use std::hash::Hash;

use crate::theme;
use crate::widgets::poster_card::{self, PosterData};
use crate::widgets::section_title;

/// Vertical room above and below cards so a hovered (enlarged) poster is not clipped.
const LIFT_ROOM: f32 = 10.0;

/// A titled, horizontally scrolling row of posters. Returns the index of the clicked poster.
pub(crate) fn show(
    ui: &mut egui::Ui,
    id: impl Hash + std::fmt::Debug,
    title: &str,
    posters: &[PosterData<'_>],
) -> Option<usize> {
    ui.horizontal(|ui| {
        ui.add_space(theme::PAGE_MARGIN);
        section_title::show(ui, title);
    });
    let mut clicked = None;
    egui::ScrollArea::horizontal()
        .id_salt(id)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(theme::PAGE_MARGIN);
                ui.spacing_mut().item_spacing.x = 18.0;
                ui.vertical(|ui| {
                    ui.add_space(LIFT_ROOM);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 18.0;
                        for (index, poster) in posters.iter().enumerate() {
                            if poster_card::show(ui, poster) {
                                clicked = Some(index);
                            }
                        }
                    });
                });
                ui.add_space(theme::PAGE_MARGIN);
            });
        });
    clicked
}
