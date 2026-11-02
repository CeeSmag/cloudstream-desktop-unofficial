use egui::RichText;

use crate::theme;

/// Centered message for screens with nothing to show.
pub(crate) fn show(ui: &mut egui::Ui, headline: &str, hint: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(60.0);
        ui.label(RichText::new(headline).size(20.0).strong());
        ui.add_space(6.0);
        ui.label(RichText::new(hint).color(theme::MUTED_TEXT));
    });
}

/// Centered spinner with a caption.
pub(crate) fn loading(ui: &mut egui::Ui, caption: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.spinner();
        ui.add_space(6.0);
        ui.label(RichText::new(caption).color(theme::MUTED_TEXT));
    });
}
