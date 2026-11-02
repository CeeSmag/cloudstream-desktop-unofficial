use egui::RichText;

use crate::theme;

/// A bold shelf or section heading.
pub(crate) fn show(ui: &mut egui::Ui, text: &str) {
    ui.add_space(18.0);
    ui.label(RichText::new(text).size(22.0).strong().color(theme::TEXT));
    ui.add_space(8.0);
}

/// Small grey caption text.
pub(crate) fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::MUTED_TEXT));
}
