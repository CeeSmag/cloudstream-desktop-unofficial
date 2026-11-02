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

/// Draws one poster with its title; returns true when it was clicked.
pub(crate) fn show(ui: &mut egui::Ui, poster: &PosterData<'_>) -> bool {
    let size = Vec2::new(theme::POSTER_WIDTH, theme::POSTER_HEIGHT + 40.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let image_rect = Rect::from_min_size(
        rect.min,
        Vec2::new(theme::POSTER_WIDTH, theme::POSTER_HEIGHT),
    );
    draw_image(ui, image_rect, poster);
    draw_progress(ui, image_rect, poster.progress);
    draw_title(ui, rect, image_rect, poster);
    if response.hovered() {
        ui.painter().rect_stroke(
            image_rect,
            theme::ROUNDING,
            Stroke::new(2.0, theme::PRIMARY),
            StrokeKind::Inside,
        );
    }
    response.clicked()
}

fn draw_image(ui: &mut egui::Ui, rect: Rect, poster: &PosterData<'_>) {
    ui.painter()
        .rect_filled(rect, theme::ROUNDING, theme::SURFACE);
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
                .map_or_else(String::new, |c| c.to_uppercase().to_string());
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                initial,
                FontId::proportional(40.0),
                theme::MUTED_TEXT,
            );
        }
    }
}

fn draw_progress(ui: &mut egui::Ui, image_rect: Rect, progress: Option<f32>) {
    let Some(fraction) = progress else { return };
    let bar = Rect::from_min_size(
        egui::pos2(image_rect.left(), image_rect.bottom() - 4.0),
        Vec2::new(image_rect.width(), 4.0),
    );
    ui.painter()
        .rect_filled(bar, 0.0, Color32::from_black_alpha(160));
    let filled = Rect::from_min_size(
        bar.min,
        Vec2::new(bar.width() * fraction.clamp(0.0, 1.0), 4.0),
    );
    ui.painter().rect_filled(filled, 0.0, theme::PRIMARY);
}

fn draw_title(ui: &mut egui::Ui, card: Rect, image_rect: Rect, poster: &PosterData<'_>) {
    let font = FontId::proportional(13.0);
    let top = egui::pos2(card.left() + 2.0, image_rect.bottom() + 4.0);
    let galley = ui.painter().layout(
        poster.title.to_owned(),
        font.clone(),
        theme::TEXT,
        theme::POSTER_WIDTH - 4.0,
    );
    let title_height = galley.size().y.min(32.0);
    ui.painter()
        .with_clip_rect(Rect::from_min_size(
            top,
            Vec2::new(theme::POSTER_WIDTH, 34.0),
        ))
        .galley(top, galley, theme::TEXT);
    if let Some(caption) = &poster.caption {
        let at = egui::pos2(top.x, top.y + title_height + 1.0);
        ui.painter().text(
            at,
            Align2::LEFT_TOP,
            caption,
            FontId::proportional(11.0),
            theme::MUTED_TEXT,
        );
    }
}
