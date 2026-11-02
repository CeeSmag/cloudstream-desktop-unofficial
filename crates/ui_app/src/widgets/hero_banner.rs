use egui::{Color32, Rect, RichText, Sense, UiBuilder, Vec2};

use crate::ambient_backdrop;
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

/// Paints the banner: a blurred wash of the artwork's colours, the sharp poster on the right, and
/// fades so the text on the left and the shelf below stay legible.
pub(crate) fn paint_backdrop(ui: &egui::Ui, rect: Rect, image_url: Option<&str>) {
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme::SURFACE);
    if let Some(url) = image_url {
        paint_ambient(ui, rect, url);
        paint_poster(ui, rect, url);
    }
    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(70));
    let left = Rect::from_min_max(
        rect.min,
        egui::pos2(rect.left() + rect.width() * 0.7, rect.bottom()),
    );
    gradient::fill(
        painter,
        left,
        Direction::LeftToRight,
        (Color32::from_black_alpha(190), Color32::TRANSPARENT),
    );
    let bottom = Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - rect.height() * 0.4),
        rect.right_bottom(),
    );
    gradient::fill(
        painter,
        bottom,
        Direction::TopToBottom,
        (Color32::TRANSPARENT, theme::BACKGROUND),
    );
}

fn paint_ambient(ui: &egui::Ui, rect: Rect, url: &str) {
    if let Some(texture) = ambient_backdrop::texture_for(ui.ctx(), url) {
        egui::Image::from_texture((texture.id(), texture.size_vec2())).paint_at(ui, rect);
    }
}

fn paint_poster(ui: &egui::Ui, rect: Rect, url: &str) {
    let height = rect.height() - 72.0;
    let size = Vec2::new(height * 2.0 / 3.0, height);
    let poster = Rect::from_min_size(
        egui::pos2(
            rect.right() - size.x - theme::PAGE_MARGIN - 24.0,
            rect.top() + 28.0,
        ),
        size,
    );
    let shadow = egui::Shadow {
        offset: [0, 14],
        blur: 36,
        spread: 0,
        color: Color32::from_black_alpha(170),
    };
    ui.painter().add(shadow.as_shape(poster, theme::ROUNDING));
    ui.painter()
        .rect_filled(poster, theme::ROUNDING, theme::SURFACE_HIGH);
    egui::Image::new(url)
        .fit_to_exact_size(size)
        .corner_radius(theme::ROUNDING)
        .paint_at(ui, poster);
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
