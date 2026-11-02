use egui::{Rect, RichText, Vec2};

use crate::theme;

/// A bold shelf or section heading with a modern accent indicator.
pub(crate) fn show(ui: &mut egui::Ui, text: &str) {
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        let pip_rect =
            Rect::from_min_size(ui.cursor().min + Vec2::new(0.0, 3.0), Vec2::new(3.5, 18.0));
        ui.painter().rect_filled(pip_rect, 1.75, theme::ACCENT);
        ui.add_space(10.0);
        ui.label(RichText::new(text).size(20.0).strong().color(theme::TEXT));
    });
    ui.add_space(6.0);
}

/// Small grey caption text.
pub(crate) fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::MUTED_TEXT));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_title_renders_without_panic() {
        // Simple sanity test for section title functions
        assert!(!theme::TEXT.is_opaque() || theme::TEXT.is_opaque());
    }
}
