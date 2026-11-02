use egui::{Color32, Rect, Shadow, Stroke, StrokeKind, TextureHandle, Ui};

/// What lies behind a glass panel: a blurred picture and the screen area it covers.
pub(crate) struct Backdrop {
    pub(crate) texture: TextureHandle,
    /// The rectangle of the screen the whole texture is stretched over.
    pub(crate) area: Rect,
}

/// Tint laid over the blurred backdrop; darker when nothing is behind the panel.
const TINT_OVER_BACKDROP: Color32 = Color32::from_rgba_premultiplied(8, 8, 10, 110);
const TINT_SOLID: Color32 = Color32::from_rgba_premultiplied(34, 34, 38, 235);

/// Where `rect` falls inside `area`, as texture coordinates in `0.0..=1.0`.
pub(crate) fn uv_within(area: Rect, rect: Rect) -> Rect {
    let size = area.size().max(egui::vec2(1.0, 1.0));
    Rect::from_min_max(
        egui::pos2(
            (rect.min.x - area.min.x) / size.x,
            (rect.min.y - area.min.y) / size.y,
        ),
        egui::pos2(
            (rect.max.x - area.min.x) / size.x,
            (rect.max.y - area.min.y) / size.y,
        ),
    )
    .intersect(Rect::from_min_max(
        egui::pos2(0.0, 0.0),
        egui::pos2(1.0, 1.0),
    ))
}

/// Paints a frosted-glass panel: soft shadow, the blurred backdrop (when known), a tint, a hairline
/// border and a faint top highlight.
pub(crate) fn paint(ui: &Ui, rect: Rect, rounding: u8, backdrop: Option<&Backdrop>) {
    let painter = ui.painter();
    let shadow = Shadow {
        offset: [0, 10],
        blur: 30,
        spread: 0,
        color: Color32::from_black_alpha(150),
    };
    painter.add(shadow.as_shape(rect, rounding));
    match backdrop {
        Some(backdrop) => {
            let uv = uv_within(backdrop.area, rect);
            egui::Image::from_texture((backdrop.texture.id(), backdrop.texture.size_vec2()))
                .uv(uv)
                .corner_radius(rounding)
                .paint_at(ui, rect);
            painter.rect_filled(rect, rounding, TINT_OVER_BACKDROP);
        }
        None => {
            painter.rect_filled(rect, rounding, TINT_SOLID);
        }
    }
    painter.rect_stroke(
        rect,
        rounding,
        Stroke::new(1.0, Color32::from_white_alpha(34)),
        StrokeKind::Inside,
    );
    let highlight = Rect::from_min_size(
        rect.min + egui::vec2(f32::from(rounding), 1.0),
        egui::vec2((rect.width() - f32::from(rounding) * 2.0).max(0.0), 1.0),
    );
    painter.rect_filled(highlight, 0.0, Color32::from_white_alpha(46));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_within_maps_a_corner_panel_into_unit_coordinates() {
        let area = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 100.0));
        let panel = Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(100.0, 50.0));
        let uv = uv_within(area, panel);
        assert_eq!(
            (uv.min.x, uv.min.y, uv.max.x, uv.max.y),
            (0.5, 0.5, 1.0, 1.0)
        );
    }

    #[test]
    fn uv_within_clamps_panels_that_stick_out_of_the_area() {
        let area = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        let panel = Rect::from_min_size(egui::pos2(-20.0, 80.0), egui::vec2(60.0, 60.0));
        let uv = uv_within(area, panel);
        assert_eq!((uv.min.x, uv.max.y), (0.0, 1.0));
    }
}
