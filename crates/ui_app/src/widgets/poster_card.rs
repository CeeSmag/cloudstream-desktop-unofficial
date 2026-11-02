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
    draw_badge(ui, image_rect, poster.caption.as_deref());
    if let Some(rating) = poster.caption.as_deref().and_then(parse_rating) {
        draw_rating_badge(ui, image_rect, rating);
    }
    draw_progress(ui, image_rect, poster.progress);
    draw_hover_overlay(ui, image_rect, lift);
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
    ui.painter().rect_stroke(
        rect,
        theme::ROUNDING,
        Stroke::new(1.0, theme::BORDER_SUBTLE),
        StrokeKind::Inside,
    );
}

fn draw_rating_badge(ui: &egui::Ui, image_rect: Rect, rating: &str) {
    let font = FontId::proportional(9.5);
    let star_w = ui
        .painter()
        .layout_no_wrap("\u{2605} ".into(), font.clone(), theme::GOLD_STAR)
        .size()
        .x;
    let num_w = ui
        .painter()
        .layout_no_wrap(rating.into(), font.clone(), theme::GOLD_TEXT)
        .size()
        .x;
    let total_w = star_w + num_w;
    let badge_rect = Rect::from_min_size(
        egui::pos2(image_rect.right() - total_w - 18.0, image_rect.top() + 8.0),
        Vec2::new(total_w + 10.0, 18.0),
    );
    let border = Color32::from_rgba_premultiplied(251, 191, 36, 60);
    ui.painter()
        .rect_filled(badge_rect, 9, Color32::from_black_alpha(190));
    ui.painter()
        .rect_stroke(badge_rect, 9, Stroke::new(1.0, border), StrokeKind::Inside);
    paint_rating_text(ui, badge_rect, star_w, rating);
}

fn paint_rating_text(ui: &egui::Ui, rect: Rect, star_w: f32, rating: &str) {
    let font = FontId::proportional(9.5);
    let start_x = rect.left() + 5.0;
    let center_y = rect.center().y;
    ui.painter().text(
        egui::pos2(start_x, center_y),
        Align2::LEFT_CENTER,
        "\u{2605} ",
        font.clone(),
        theme::GOLD_STAR,
    );
    ui.painter().text(
        egui::pos2(start_x + star_w, center_y),
        Align2::LEFT_CENTER,
        rating,
        font,
        theme::GOLD_TEXT,
    );
}

pub(crate) fn parse_rating(caption: &str) -> Option<&str> {
    for word in caption.split(|c: char| c.is_whitespace() || c == '·' || c == ',') {
        let trimmed = word.trim().trim_start_matches('\u{2605}');
        if trimmed.len() >= 3 && trimmed.contains('.') {
            let mut parts = trimmed.split('.');
            let whole = parts.next().and_then(|p| p.parse::<u32>().ok());
            let frac = parts.next().and_then(|p| p.parse::<u32>().ok());
            if let (Some(w), Some(f)) = (whole, frac) {
                if w <= 10 && f < 100 && parts.next().is_none() {
                    return Some(trimmed);
                }
            }
        }
    }
    None
}

fn draw_badge(ui: &egui::Ui, image_rect: Rect, caption: Option<&str>) {
    let Some(text) = caption.and_then(badge_label) else {
        return;
    };
    let font = FontId::proportional(9.5);
    let text_width = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), theme::TEXT)
        .size()
        .x;
    let badge_rect = Rect::from_min_size(
        image_rect.min + Vec2::new(8.0, 8.0),
        Vec2::new(text_width + 12.0, 18.0),
    );
    ui.painter()
        .rect_filled(badge_rect, 9, Color32::from_black_alpha(190));
    ui.painter().rect_stroke(
        badge_rect,
        9,
        Stroke::new(1.0, theme::BORDER_SUBTLE),
        StrokeKind::Inside,
    );
    ui.painter().text(
        badge_rect.center(),
        Align2::CENTER_CENTER,
        text,
        font,
        Color32::from_rgb(0xf1, 0xf5, 0xf9),
    );
}

pub(crate) fn badge_label(caption: &str) -> Option<&'static str> {
    if caption.eq_ignore_ascii_case("movie") {
        Some("MOVIE")
    } else if caption.eq_ignore_ascii_case("tvseries") || caption.eq_ignore_ascii_case("series") {
        Some("SERIES")
    } else if caption.eq_ignore_ascii_case("anime") {
        Some("ANIME")
    } else if caption.eq_ignore_ascii_case("cartoon") {
        Some("CARTOON")
    } else if caption.eq_ignore_ascii_case("live") {
        Some("LIVE")
    } else if caption.eq_ignore_ascii_case("ova") {
        Some("OVA")
    } else {
        None
    }
}

fn draw_hover_overlay(ui: &egui::Ui, rect: Rect, lift: f32) {
    if lift <= 0.0 {
        return;
    }
    let outline = Stroke::new(
        1.5,
        Color32::from_rgba_premultiplied(124, 107, 255, (220.0 * lift) as u8),
    );
    ui.painter()
        .rect_stroke(rect, theme::ROUNDING, outline, StrokeKind::Inside);
    ui.painter().rect_filled(
        rect,
        theme::ROUNDING,
        Color32::from_black_alpha((85.0 * lift) as u8),
    );
    let center = rect.center();
    let circle_radius = 21.0 * lift;
    ui.painter().circle_filled(
        center,
        circle_radius,
        Color32::from_white_alpha((35.0 * lift) as u8),
    );
    ui.painter().circle_stroke(
        center,
        circle_radius,
        Stroke::new(1.0, Color32::from_white_alpha((130.0 * lift) as u8)),
    );
    let tri = [
        center + Vec2::new(-4.0, -6.5) * lift,
        center + Vec2::new(-4.0, 6.5) * lift,
        center + Vec2::new(6.5, 0.0) * lift,
    ];
    ui.painter().add(egui::Shape::convex_polygon(
        tri.to_vec(),
        Color32::from_white_alpha((240.0 * lift) as u8),
        Stroke::NONE,
    ));
}

fn draw_progress(ui: &egui::Ui, image_rect: Rect, progress: Option<f32>) {
    let Some(fraction) = progress else { return };
    let track = Rect::from_min_size(
        egui::pos2(image_rect.left() + 8.0, image_rect.bottom() - 10.0),
        Vec2::new(image_rect.width() - 16.0, 3.5),
    );
    ui.painter()
        .rect_filled(track, 2, Color32::from_black_alpha(160));
    let filled = Rect::from_min_size(
        track.min,
        Vec2::new(track.width() * fraction.clamp(0.0, 1.0), 3.5),
    );
    ui.painter().rect_filled(filled, 2, theme::ACCENT);
}

fn draw_labels(ui: &egui::Ui, card: Rect, poster: &PosterData<'_>) {
    let top = egui::pos2(card.left() + 2.0, card.top() + theme::POSTER_HEIGHT + 8.0);
    let galley = ui.painter().layout(
        poster.title.to_owned(),
        FontId::proportional(13.5),
        theme::TEXT,
        theme::POSTER_WIDTH - 4.0,
    );
    ui.painter()
        .with_clip_rect(Rect::from_min_size(
            top,
            Vec2::new(theme::POSTER_WIDTH, 34.0),
        ))
        .galley(top, galley, theme::TEXT);
    if let Some(caption) = &poster.caption {
        ui.painter().text(
            egui::pos2(top.x, top.y + 36.0),
            Align2::LEFT_TOP,
            caption,
            FontId::proportional(11.5),
            theme::MUTED_TEXT,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_label_recognises_common_media_types() {
        assert_eq!(badge_label("Movie"), Some("MOVIE"));
        assert_eq!(badge_label("TvSeries"), Some("SERIES"));
        assert_eq!(badge_label("Anime"), Some("ANIME"));
        assert_eq!(badge_label("Unknown"), None);
    }

    #[test]
    fn parse_rating_extracts_valid_scores() {
        assert_eq!(parse_rating("Movie \u{00B7} 8.5"), Some("8.5"));
        assert_eq!(parse_rating("\u{2605} 9.2"), Some("9.2"));
        assert_eq!(parse_rating("TV Series"), None);
    }
}
