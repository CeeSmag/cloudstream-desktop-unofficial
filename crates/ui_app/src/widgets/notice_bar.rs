use egui::RichText;

use crate::notice::{Notice, NoticeKind};
use crate::theme;

/// Draws the notices; returns the index of one the user dismissed.
pub(crate) fn show(ui: &mut egui::Ui, notices: &[Notice]) -> Option<usize> {
    let mut dismissed = None;
    for (index, notice) in notices.iter().enumerate() {
        let color = match notice.kind {
            NoticeKind::Info => theme::ACCENT,
            NoticeKind::Error => theme::ERROR,
        };
        egui::Frame::new()
            .fill(theme::SURFACE)
            .stroke(egui::Stroke::new(1.0, color))
            .corner_radius(theme::ROUNDING)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&notice.text).color(theme::TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("\u{2715}").clicked() {
                            dismissed = Some(index);
                        }
                    });
                });
            });
        ui.add_space(4.0);
    }
    dismissed
}
