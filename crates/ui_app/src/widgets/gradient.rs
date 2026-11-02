use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Painter, Pos2, Rect, Shape};

/// Which way a gradient fades from `from` to `to`.
#[derive(Clone, Copy)]
pub(crate) enum Direction {
    LeftToRight,
    TopToBottom,
}

/// Fills `rect` with a two-colour linear gradient; `colors` is (start, end).
pub(crate) fn fill(
    painter: &Painter,
    rect: Rect,
    direction: Direction,
    colors: (Color32, Color32),
) {
    let (from, to) = colors;
    let (first, second, third, fourth) = match direction {
        Direction::LeftToRight => (from, to, to, from),
        Direction::TopToBottom => (from, from, to, to),
    };
    let corners: [(Pos2, Color32); 4] = [
        (rect.left_top(), first),
        (rect.right_top(), second),
        (rect.right_bottom(), third),
        (rect.left_bottom(), fourth),
    ];
    let mut mesh = Mesh::default();
    for (pos, color) in corners {
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(Shape::mesh(mesh));
}
