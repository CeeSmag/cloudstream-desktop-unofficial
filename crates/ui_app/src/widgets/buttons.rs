use egui::{Button, Color32, Response, RichText, Stroke, Vec2};

use crate::theme;

/// Height of pill buttons.
const PILL_HEIGHT: f32 = 40.0;

/// The prominent action: white pill, dark text with subtle shadow.
pub(crate) fn primary(ui: &mut egui::Ui, label: &str, enabled: bool) -> Response {
    let text = RichText::new(label)
        .strong()
        .size(15.5)
        .color(Color32::from_rgb(0x0c, 0x0c, 0x16));
    let button = Button::new(text)
        .fill(Color32::WHITE)
        .corner_radius(theme::PILL_ROUNDING)
        .min_size(Vec2::new(120.0, PILL_HEIGHT));
    ui.add_enabled(enabled, button)
}

/// A quieter action: translucent glass pill with subtle border.
pub(crate) fn secondary(ui: &mut egui::Ui, label: &str) -> Response {
    let text = RichText::new(label).strong().size(14.5).color(theme::TEXT);
    let button = Button::new(text)
        .fill(theme::GLASS_FILL)
        .stroke(Stroke::new(1.0, theme::BORDER_SUBTLE))
        .corner_radius(theme::PILL_ROUNDING)
        .min_size(Vec2::new(44.0, PILL_HEIGHT));
    ui.add(button)
}

/// A selectable category / filter chip matching the desktop client style.
pub(crate) fn filter_chip(ui: &mut egui::Ui, label: &str, is_selected: bool) -> Response {
    let text_color = if is_selected {
        theme::TEXT
    } else {
        theme::MUTED_TEXT
    };
    let text = RichText::new(label).strong().size(13.5).color(text_color);
    let fill = if is_selected {
        theme::ACCENT_SOFT
    } else {
        Color32::from_white_alpha(12)
    };
    let stroke = if is_selected {
        Stroke::new(1.0, Color32::from_rgba_premultiplied(124, 107, 255, 140))
    } else {
        Stroke::new(1.0, theme::BORDER_SUBTLE)
    };
    let button = Button::new(text)
        .fill(fill)
        .stroke(stroke)
        .corner_radius(10)
        .min_size(Vec2::new(60.0, 32.0));
    ui.add(button)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pill_height_is_standard() {
        assert_eq!(PILL_HEIGHT, 40.0);
    }
}
