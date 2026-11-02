use egui::{Button, Color32, Response, RichText, Vec2};

use crate::theme;

/// Height of pill buttons.
const PILL_HEIGHT: f32 = 40.0;

/// The prominent action: white pill, black text (Apple TV's "Play").
pub(crate) fn primary(ui: &mut egui::Ui, label: &str, enabled: bool) -> Response {
    let text = RichText::new(label)
        .strong()
        .size(16.0)
        .color(Color32::BLACK);
    let button = Button::new(text)
        .fill(Color32::WHITE)
        .corner_radius(theme::PILL_ROUNDING)
        .min_size(Vec2::new(120.0, PILL_HEIGHT));
    ui.add_enabled(enabled, button)
}

/// A quieter action: translucent glass pill, white text.
pub(crate) fn secondary(ui: &mut egui::Ui, label: &str) -> Response {
    let text = RichText::new(label).strong().size(15.0).color(theme::TEXT);
    let button = Button::new(text)
        .fill(theme::GLASS_FILL)
        .corner_radius(theme::PILL_ROUNDING)
        .min_size(Vec2::new(44.0, PILL_HEIGHT));
    ui.add(button)
}
