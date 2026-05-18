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
