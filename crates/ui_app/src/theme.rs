use egui::{Color32, CornerRadius, Stroke};

/// Colours taken from CloudStream's `colors.xml`.
pub(crate) const PRIMARY: Color32 = Color32::from_rgb(0x3d, 0x50, 0xfa);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(0x3b, 0x65, 0xf5);
pub(crate) const BACKGROUND: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);
pub(crate) const CARD_BACKGROUND: Color32 = Color32::from_rgb(0x16, 0x16, 0x16);
pub(crate) const SURFACE: Color32 = Color32::from_rgb(0x2b, 0x2c, 0x30);
pub(crate) const SEARCH_FIELD: Color32 = Color32::from_rgb(0x30, 0x31, 0x35);
pub(crate) const TEXT: Color32 = Color32::from_rgb(0xe9, 0xea, 0xee);
pub(crate) const MUTED_TEXT: Color32 = Color32::from_rgb(0x9b, 0xa0, 0xa4);
pub(crate) const ERROR: Color32 = Color32::from_rgb(0xf5, 0x3b, 0x66);

/// Poster card size (2:3), matching the Android home screen.
pub(crate) const POSTER_WIDTH: f32 = 120.0;
pub(crate) const POSTER_HEIGHT: f32 = 180.0;
/// Corner radius of posters and buttons.
pub(crate) const ROUNDING: u8 = 8;

/// Applies the CloudStream dark theme to an egui context.
pub(crate) fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = SEARCH_FIELD;
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = PRIMARY;
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
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.hovered.weak_bg_fill = PRIMARY;
    ctx.set_visuals(visuals);
}
