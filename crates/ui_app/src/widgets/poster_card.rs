use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::theme;

/// What a poster card shows.
pub(crate) struct PosterData<'a> {
    pub(crate) title: &'a str,
    pub(crate) image_url: Option<&'a str>,
    pub(crate) caption: Option<String>,
    /// Fraction watched, drawn as a bar along the bottom of the poster.
    pub(crate) progress: Option<f32>,
}

/// Space reserved under the poster for the title and caption.
const LABEL_HEIGHT: f32 = 56.0;
/// How much a hovered poster grows.
const HOVER_SCALE: f32 = 0.06;

/// Draws one poster with its title; the poster lifts when hovered. True when clicked.
pub(crate) fn show(ui: &mut egui::Ui, poster: &PosterData<'_>) -> bool {
    let size = Vec2::new(theme::POSTER_WIDTH, theme::POSTER_HEIGHT + LABEL_HEIGHT);
    let (card, response) = ui.allocate_exact_size(size, Sense::click());
    let lift = ui
        .ctx()
        .animate_bool_with_time(response.id, response.hovered(), 0.12);
    let base = Rect::from_min_size(
        card.min,
        Vec2::new(theme::POSTER_WIDTH, theme::POSTER_HEIGHT),
    );
    let image_rect = base.expand2(Vec2::new(
        theme::POSTER_WIDTH * HOVER_SCALE * lift / 2.0,
        theme::POSTER_HEIGHT * HOVER_SCALE * lift / 2.0,
    ));
    draw_shadow(ui, image_rect, lift);
    draw_image(ui, image_rect, poster);
    draw_progress(ui, image_rect, poster.progress);
    if lift > 0.0 {
        let outline = Stroke::new(2.0, Color32::from_white_alpha((200.0 * lift) as u8));
        ui.painter()
            .rect_stroke(image_rect, theme::ROUNDING, outline, StrokeKind::Inside);
    }
    draw_labels(ui, card, poster);
    response.clicked()
}

fn draw_shadow(ui: &egui::Ui, rect: Rect, lift: f32) {
    if lift <= 0.0 {
        return;
    }
    let shadow = egui::Shadow {
        offset: [0, (10.0 * lift) as i8],
        blur: (24.0 * lift) as u8,
        spread: 0,
        color: Color32::from_black_alpha((160.0 * lift) as u8),
    };
    ui.painter().add(shadow.as_shape(rect, theme::ROUNDING));
}

fn draw_image(ui: &egui::Ui, rect: Rect, poster: &PosterData<'_>) {
    ui.painter()
        .rect_filled(rect, theme::ROUNDING, theme::SURFACE_HIGH);
    match poster.image_url {
        Some(url) => {
            egui::Image::new(url)
                .fit_to_exact_size(rect.size())
                .corner_radius(theme::ROUNDING)
                .paint_at(ui, rect);
        }
        None => {
            let initial = poster
                .title
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default();
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                initial,
                FontId::proportional(44.0),
                theme::MUTED_TEXT,
            );
        }
    }
}

fn draw_progress(ui: &egui::Ui, image_rect: Rect, progress: Option<f32>) {
    let Some(fraction) = progress else { return };
    let track = Rect::from_min_size(
        egui::pos2(image_rect.left() + 10.0, image_rect.bottom() - 12.0),
        Vec2::new(image_rect.width() - 20.0, 4.0),
    );
    ui.painter()
        .rect_filled(track, 2, Color32::from_white_alpha(70));
    let filled = Rect::from_min_size(
        track.min,
        Vec2::new(track.width() * fraction.clamp(0.0, 1.0), 4.0),
    );
    ui.painter().rect_filled(filled, 2, Color32::WHITE);
}

fn draw_labels(ui: &egui::Ui, card: Rect, poster: &PosterData<'_>) {
    let top = egui::pos2(card.left() + 2.0, card.top() + theme::POSTER_HEIGHT + 10.0);
    let galley = ui.painter().layout(
        poster.title.to_owned(),
        FontId::proportional(14.0),
        theme::TEXT,
        theme::POSTER_WIDTH - 4.0,
    );
    ui.painter()
        .with_clip_rect(Rect::from_min_size(
            top,
            Vec2::new(theme::POSTER_WIDTH, 36.0),
        ))
        .galley(top, galley, theme::TEXT);
    if let Some(caption) = &poster.caption {
        ui.painter().text(
            egui::pos2(top.x, top.y + 38.0),
            Align2::LEFT_TOP,
            caption,
            FontId::proportional(12.0),
            theme::MUTED_TEXT,
        );
    }
}
