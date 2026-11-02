use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke};

/// Modern CloudStream palette: deep Midnight Navy canvas, graphite-tinted surfaces, violet accents.
pub(crate) const BACKGROUND: Color32 = Color32::from_rgb(0x0c, 0x0c, 0x16);
pub(crate) const SURFACE: Color32 = Color32::from_rgb(0x16, 0x16, 0x24);
pub(crate) const SURFACE_HIGH: Color32 = Color32::from_rgb(0x20, 0x20, 0x2e);
pub(crate) const TEXT: Color32 = Color32::from_rgb(0xf8, 0xfa, 0xfc);
pub(crate) const MUTED_TEXT: Color32 = Color32::from_rgb(0x94, 0xa3, 0xb8);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(0x7c, 0x6b, 0xff);
pub(crate) const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(124, 107, 255, 40);
pub(crate) const ERROR: Color32 = Color32::from_rgb(0xff, 0x45, 0x3a);
pub(crate) const GOLD_STAR: Color32 = Color32::from_rgb(0xfb, 0xbf, 0x24);
pub(crate) const GOLD_TEXT: Color32 = Color32::from_rgb(0xfe, 0xf0, 0x8a);
pub(crate) const BORDER_SUBTLE: Color32 = Color32::from_rgba_premultiplied(18, 18, 18, 18);
pub(crate) const BORDER_HOVER: Color32 = Color32::from_rgba_premultiplied(45, 45, 45, 45);
pub(crate) const GLASS: Color32 = Color32::from_rgba_premultiplied(22, 22, 36, 235);
pub(crate) const GLASS_FILL: Color32 = Color32::from_rgba_premultiplied(36, 36, 52, 160);

/// Poster card size (2:3).
pub(crate) const POSTER_WIDTH: f32 = 150.0;
pub(crate) const POSTER_HEIGHT: f32 = 225.0;
/// Corner radius of cards and panels.
pub(crate) const ROUNDING: u8 = 12;
/// Corner radius of pill buttons.
pub(crate) const PILL_ROUNDING: u8 = 20;
/// Left/right margin of content, so shelves line up with the hero text.
pub(crate) const PAGE_MARGIN: f32 = 40.0;

/// Fonts looked up on Windows; the first hit becomes the main UI font, the symbol font backs it up.
const MAIN_FONT_FILES: &[&str] = &["C:\\Windows\\Fonts\\segoeui.ttf"];
const SYMBOL_FONT_FILES: &[&str] = &["C:\\Windows\\Fonts\\seguisym.ttf"];

/// Applies the dark theme and system fonts to an egui context.
pub(crate) fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(0x08, 0x08, 0x10);
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.0, TEXT);
    let rounding = CornerRadius::same(ROUNDING);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.corner_radius = rounding;
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.hovered.weak_bg_fill = SURFACE_HIGH;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_HOVER);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0x28, 0x28, 0x3a);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.window_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    ctx.set_visuals(visuals);
}

/// Loads Segoe UI (and its symbol font) when present; otherwise egui's built-in fonts stay.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    if let Some(main) = first_readable(MAIN_FONT_FILES) {
        fonts.font_data.insert(
            "system-main".to_owned(),
            Arc::new(FontData::from_owned(main)),
        );
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            if let Some(list) = fonts.families.get_mut(&family) {
                list.insert(0, "system-main".to_owned());
            }
        }
    }
    if let Some(symbols) = first_readable(SYMBOL_FONT_FILES) {
        fonts.font_data.insert(
            "system-symbols".to_owned(),
            Arc::new(FontData::from_owned(symbols)),
        );
        for list in fonts.families.values_mut() {
            list.push("system-symbols".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

fn first_readable(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|path| std::fs::read(path).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_palette_has_modern_midnight_contrast() {
        assert_eq!(BACKGROUND, Color32::from_rgb(0x0C, 0x0C, 0x16));
        assert_eq!(SURFACE, Color32::from_rgb(0x16, 0x16, 0x24));
        assert_eq!(SURFACE_HIGH, Color32::from_rgb(0x20, 0x20, 0x2E));
        assert_eq!(ACCENT, Color32::from_rgb(0x7C, 0x6B, 0xFF));
    }
}
