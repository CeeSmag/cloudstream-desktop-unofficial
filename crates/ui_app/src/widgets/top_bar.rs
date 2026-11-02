use egui::{Align2, Color32, FontId, Sense, Vec2};

use crate::navigation::Tab;
use crate::theme;

/// Height of the top navigation bar.
pub(crate) const HEIGHT: f32 = 64.0;
/// Horizontal padding inside each tab pill.
const PILL_PADDING: f32 = 18.0;
/// Space between pills.
const PILL_GAP: f32 = 6.0;

/// Draws the centred tab bar; returns the tab the user clicked.
pub(crate) fn show(ui: &mut egui::Ui, current: Tab) -> Option<Tab> {
    let widths: Vec<f32> = Tab::ALL
        .iter()
        .map(|tab| label_width(ui, *tab) + PILL_PADDING * 2.0)
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + PILL_GAP * (Tab::ALL.len() as f32 - 1.0);
    let mut clicked = None;
    ui.horizontal_centered(|ui| {
        ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
        ui.spacing_mut().item_spacing.x = PILL_GAP;
        for (tab, width) in Tab::ALL.into_iter().zip(widths) {
            if pill(ui, tab, width, tab == current) {
                clicked = Some(tab);
            }
        }
    });
    clicked
}

fn label_width(ui: &egui::Ui, tab: Tab) -> f32 {
    ui.painter()
        .layout_no_wrap(
            tab.label().to_owned(),
            FontId::proportional(16.0),
            theme::TEXT,
        )
        .size()
        .x
}

fn pill(ui: &mut egui::Ui, tab: Tab, width: f32, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 36.0), Sense::click());
    if active {
        ui.painter()
            .rect_filled(rect, theme::PILL_ROUNDING, Color32::from_white_alpha(46));
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, theme::PILL_ROUNDING, Color32::from_white_alpha(20));
    }
    let color = if active {
        theme::TEXT
    } else {
        theme::MUTED_TEXT
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        tab.label(),
        FontId::proportional(16.0),
        color,
    );
    response.clicked()
}
