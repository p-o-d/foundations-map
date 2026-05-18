//! Hand-drawn icon set for the 3D sector view.
//!
//! Three super-category frames + 14 inner-glyph paint functions. All drawing
//! goes through `egui::Painter` — no font or texture dependency.

use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

use crate::renderer::atlas::{IconId, SuperCategory};

pub const HALF_NORMAL:     f32 = 11.0;   // 22 px total
pub const HALF_SELECTED:   f32 = 15.0;   // 30 px total
pub const STROKE_NORMAL:   f32 = 1.6;
pub const STROKE_SELECTED: f32 = 2.2;
pub const DOT_RADIUS:      f32 = 1.6;
pub const DOT_RADIUS_SEL:  f32 = 2.0;

pub const SELECTION_COLOR: Color32 = Color32::from_rgb(255, 217, 25);
pub const STATIC_FRAME_COLOR: Color32 = Color32::from_rgb(140, 140, 140);

/// Draw the square outline frame used by all 8 station icons.
pub fn draw_station_frame(
    painter: &Painter,
    center: Pos2,
    half: f32,
    stroke: f32,
    color: Color32,
) {
    let rect = Rect::from_center_size(center, Vec2::splat(half * 2.0));
    painter.rect_stroke(rect, 0.0, Stroke::new(stroke, color), StrokeKind::Outside);
}

/// Draw the circle outline frame used by all 4 ship icons.
pub fn draw_ship_frame(
    painter: &Painter,
    center: Pos2,
    half: f32,
    stroke: f32,
    color: Color32,
) {
    let radius = half - stroke * 0.5;
    painter.circle_stroke(center, radius, Stroke::new(stroke, color));
}

/// Draw the 4-corner-dot frame used by static (anomaly, resource zone) icons.
/// Always grey, regardless of any caller-supplied colour — static objects have
/// no meaningful faction.
pub fn draw_static_frame(painter: &Painter, center: Pos2, half: f32, dot_r: f32) {
    let off = half - dot_r;
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let p = Pos2::new(center.x + sx * off, center.y + sy * off);
        painter.circle_filled(p, dot_r, STATIC_FRAME_COLOR);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_size_larger_than_normal() {
        assert!(HALF_SELECTED > HALF_NORMAL);
        assert!(STROKE_SELECTED > STROKE_NORMAL);
        assert!(DOT_RADIUS_SEL > DOT_RADIUS);
    }
}
