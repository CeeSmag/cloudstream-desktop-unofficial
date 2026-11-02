use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::navigation::Tab;
use crate::theme;

/// Height of the top navigation bar.
pub(crate) const HEIGHT: f32 = 64.0;
/// Horizontal padding inside each tab pill.
const PILL_PADDING: f32 = 18.0;
/// Space between pills.
const PILL_GAP: f32 = 6.0;

/// Draws the centered tab bar with branding and border; returns clicked tab.
pub(crate) fn show(ui: &mut egui::Ui, current: Tab) -> Option<Tab> {
    let bar_rect = ui.max_rect();
    ui.painter().hline(
        bar_rect.x_range(),
        bar_rect.bottom() - 1.0,
        Stroke::new(1.0, theme::BORDER_SUBTLE),
    );
    draw_brand(ui);
    draw_tabs(ui, current)
}

fn draw_brand(ui: &mut egui::Ui) {
    let brand_rect = Rect::from_min_size(
        ui.max_rect().min + Vec2::new(theme::PAGE_MARGIN, 0.0),
        Vec2::new(140.0, HEIGHT),
    );
    let center_y = brand_rect.center().y;
    ui.painter().text(
        egui::pos2(brand_rect.left(), center_y),
        Align2::LEFT_CENTER,
        "CloudStream",
        FontId::proportional(17.0),
        theme::TEXT,
    );
    ui.painter().circle_filled(
        egui::pos2(brand_rect.left() + 98.0, center_y - 4.0),
        2.5,
        theme::ACCENT,
    );
}

fn draw_tabs(ui: &mut egui::Ui, current: Tab) -> Option<Tab> {
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
            FontId::proportional(15.0),
            theme::TEXT,
        )
        .size()
        .x
}

fn pill(ui: &mut egui::Ui, tab: Tab, width: f32, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 36.0), Sense::click());
    if active {
        ui.painter()
            .rect_filled(rect, theme::PILL_ROUNDING, theme::ACCENT_SOFT);
        ui.painter().rect_stroke(
            rect,
            theme::PILL_ROUNDING,
            Stroke::new(1.0, Color32::from_rgba_premultiplied(124, 107, 255, 120)),
            StrokeKind::Inside,
        );
        let indicator = Rect::from_center_size(
            egui::pos2(rect.center().x, rect.bottom() - 3.0),
            Vec2::new(18.0, 2.5),
        );
        ui.painter().rect_filled(indicator, 1.25, theme::ACCENT);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, theme::PILL_ROUNDING, Color32::from_white_alpha(16));
        ui.painter().rect_stroke(
            rect,
            theme::PILL_ROUNDING,
            Stroke::new(1.0, theme::BORDER_HOVER),
            StrokeKind::Inside,
        );
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
        FontId::proportional(15.0),
        color,
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_bar_height_is_standard() {
        assert_eq!(HEIGHT, 64.0);
    }
}
