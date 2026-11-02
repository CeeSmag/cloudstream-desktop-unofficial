use egui::RichText;

use crate::theme;

/// A bold heading above a row or a settings block.
pub(crate) fn show(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).size(18.0).strong().color(theme::TEXT));
    ui.add_space(4.0);
}

/// Small grey caption text.
pub(crate) fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::MUTED_TEXT));
}
