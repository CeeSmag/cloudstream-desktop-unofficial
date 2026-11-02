use egui::Stroke;

use crate::theme;

/// A rounded graphite panel with subtle border used to group cards and settings.
pub(crate) fn show<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER_SUBTLE))
        .corner_radius(theme::ROUNDING)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}
