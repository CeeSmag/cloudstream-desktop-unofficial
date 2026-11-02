use egui::{Color32, Rect, RichText, Sense, UiBuilder, Vec2};

use crate::theme;
use crate::widgets::buttons;
use crate::widgets::gradient::{self, Direction};

/// What a hero banner shows.
pub(crate) struct HeroData<'a> {
    pub(crate) title: &'a str,
    pub(crate) eyebrow: String,
    pub(crate) synopsis: Option<&'a str>,
    pub(crate) image_url: Option<&'a str>,
}

/// Labels of the hero's buttons.
pub(crate) struct HeroButtons<'a> {
    pub(crate) primary: &'a str,
    pub(crate) primary_enabled: bool,
    pub(crate) secondary: Option<&'a str>,
}

/// Which button the user pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeroClick {
    Primary,
    Secondary,
}

/// Draws a full-width banner: cropped art, left-to-right and bottom fades, title and buttons.
pub(crate) fn show(
    ui: &mut egui::Ui,
    hero: &HeroData<'_>,
    height: f32,
    buttons: &HeroButtons<'_>,
) -> Option<HeroClick> {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    paint_backdrop(ui, rect, hero.image_url);
    let content = Rect::from_min_max(
        egui::pos2(rect.left() + theme::PAGE_MARGIN, rect.top() + height * 0.18),
        egui::pos2(rect.left() + rect.width() * 0.6, rect.bottom() - 28.0),
    );
    let mut clicked = None;
    ui.scope_builder(UiBuilder::new().max_rect(content), |ui| {
        clicked = draw_text(ui, hero, buttons);
    });
    clicked
}

/// Paints the banner art, cropped to a wide band, and darkens it for legible text.
pub(crate) fn paint_backdrop(ui: &egui::Ui, rect: Rect, image_url: Option<&str>) {
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme::SURFACE);
    if let Some(url) = image_url {
        let crop = Rect::from_min_max(egui::pos2(0.0, 0.12), egui::pos2(1.0, 0.62));
        egui::Image::new(url)
            .uv(crop)
            .fit_to_exact_size(rect.size())
            .paint_at(ui, rect);
    }
    gradient::fill(
        painter,
        rect,
        Direction::LeftToRight,
        (
            Color32::from_black_alpha(235),
            Color32::from_black_alpha(40),
        ),
    );
    let bottom = Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - rect.height() * 0.45),
        rect.right_bottom(),
    );
    gradient::fill(
        painter,
        bottom,
        Direction::TopToBottom,
        (Color32::TRANSPARENT, theme::BACKGROUND),
    );
}

fn draw_text(
    ui: &mut egui::Ui,
    hero: &HeroData<'_>,
    buttons: &HeroButtons<'_>,
) -> Option<HeroClick> {
    ui.label(
        RichText::new(hero.eyebrow.to_uppercase())
            .size(12.0)
            .strong()
            .color(theme::MUTED_TEXT),
    );
    ui.add(
        egui::Label::new(
            RichText::new(hero.title)
                .size(40.0)
                .strong()
                .color(theme::TEXT),
        )
        .truncate(),
    );
    if let Some(synopsis) = hero.synopsis.filter(|text| !text.is_empty()) {
        ui.add_space(4.0);
        ui.add(
            egui::Label::new(RichText::new(synopsis).size(15.0).color(theme::TEXT))
                .wrap_mode(egui::TextWrapMode::Wrap),
        );
    }
    ui.add_space(14.0);
    let mut clicked = None;
    ui.horizontal(|ui| {
        if buttons::primary(ui, buttons.primary, buttons.primary_enabled).clicked() {
            clicked = Some(HeroClick::Primary);
        }
        if let Some(label) = buttons.secondary {
            if buttons::secondary(ui, label).clicked() {
                clicked = Some(HeroClick::Secondary);
            }
        }
    });
    clicked
}
