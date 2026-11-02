use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke};

/// Apple TV inspired palette: true black canvas, graphite surfaces, white accents.
pub(crate) const BACKGROUND: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
pub(crate) const SURFACE: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1e);
pub(crate) const SURFACE_HIGH: Color32 = Color32::from_rgb(0x2c, 0x2c, 0x2e);
pub(crate) const TEXT: Color32 = Color32::from_rgb(0xf5, 0xf5, 0xf7);
pub(crate) const MUTED_TEXT: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x93);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(0x0a, 0x84, 0xff);
pub(crate) const ERROR: Color32 = Color32::from_rgb(0xff, 0x45, 0x3a);
pub(crate) const GLASS: Color32 = Color32::from_rgba_premultiplied(40, 40, 44, 235);
pub(crate) const GLASS_FILL: Color32 = Color32::from_rgba_premultiplied(60, 60, 66, 150);

/// Poster card size (2:3).
pub(crate) const POSTER_WIDTH: f32 = 150.0;
pub(crate) const POSTER_HEIGHT: f32 = 225.0;
/// Corner radius of cards and panels.
pub(crate) const ROUNDING: u8 = 14;
/// Corner radius of pill buttons.
pub(crate) const PILL_ROUNDING: u8 = 22;
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
    visuals.extreme_bg_color = SURFACE;
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
    visuals.widgets.inactive.weak_bg_fill = SURFACE_HIGH;
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x3a, 0x3a, 0x3c);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0x48, 0x48, 0x4a);
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
