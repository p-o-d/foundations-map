//! Hand-drawn icon set for the 3D sector view.
//!
//! Three super-category frames + 14 inner-glyph paint functions. All drawing
//! goes through `egui::Painter` — no font or texture dependency.

use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

use crate::renderer::atlas::IconId;

pub const HALF_NORMAL:     f32 = 11.0;   // 22 px total
pub const HALF_SELECTED:   f32 = 15.0;   // 30 px total
pub const STROKE_NORMAL:   f32 = 1.6;
pub const STROKE_SELECTED: f32 = 2.2;

pub const SELECTION_COLOR: Color32 = Color32::from_rgb(255, 217, 25);
const STATIC_FRAME_COLOR: Color32 = Color32::from_rgb(140, 140, 140);

/// Square outline frame for station icons (faction-coloured stroke).
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

/// Square outline frame for static icons (anomaly, resource zone) — always grey.
pub fn draw_static_frame(painter: &Painter, center: Pos2, half: f32, stroke: f32) {
    let rect = Rect::from_center_size(center, Vec2::splat(half * 2.0));
    painter.rect_stroke(rect, 0.0, Stroke::new(stroke, STATIC_FRAME_COLOR), StrokeKind::Outside);
}

/// Paint the inner glyph for `icon` at `center`. `half` is the icon half-size
/// in screen pixels (HALF_NORMAL or HALF_SELECTED). `frame_color` is the
/// faction tint (or selection yellow) — used for accent details inside each
/// glyph and for the ship hull fill.
pub fn draw_glyph(
    painter: &Painter,
    icon: IconId,
    center: Pos2,
    half: f32,
    frame_color: Color32,
) {
    let s = half / 8.0;
    let white = Color32::WHITE;

    match icon {
        IconId::Factory        => glyph_factory(painter, center, s, white, frame_color),
        IconId::WharfShipyard  => glyph_wharf_shipyard(painter, center, s, white, frame_color),
        IconId::Defense        => glyph_defense(painter, center, s, white, frame_color),
        IconId::Trading        => glyph_trading(painter, center, s, white, frame_color),
        IconId::EquipDock      => glyph_equip_dock(painter, center, s, white, frame_color),
        IconId::HQ             => glyph_hq(painter, center, s, white, frame_color),
        IconId::PlayerStation  => glyph_player_station(painter, center, s, white, frame_color),
        IconId::GenericStation => glyph_generic_station(painter, center, s, white, frame_color),

        IconId::Capital   => glyph_capital(painter, center, s, frame_color),
        IconId::Medium    => glyph_medium(painter, center, s, frame_color),
        IconId::Small     => glyph_small(painter, center, s, frame_color),
        IconId::Transport => glyph_transport(painter, center, s, frame_color),

        IconId::Anomaly      => glyph_anomaly(painter, center, s),
        IconId::ResourceZone => glyph_resource_zone(painter, center, s),
    }
}

// -------------------------------------------------------------------------
// Station glyphs
// -------------------------------------------------------------------------

fn glyph_factory(p: &Painter, c: Pos2, s: f32, white: Color32, accent: Color32) {
    path_filled(p, c, s, white, &[
        (0.0, -5.9), (4.8, -3.2), (4.8, 3.2),
        (0.0, 5.9), (-4.8, 3.2), (-4.8, -3.2),
    ]);
    rect_fill(p, c, s, white, -1.4, -6.2, 2.8, 3.0);
    rect_fill(p, c, s, white, -1.4, 3.2, 2.8, 3.0);
    p.circle_filled(c, 1.9 * s, accent);
}

fn glyph_wharf_shipyard(p: &Painter, c: Pos2, s: f32, white: Color32, accent: Color32) {
    rect_fill(p, c, s, white, -5.9, -5.8, 2.7, 11.6);
    rect_fill(p, c, s, white, 3.2, -5.8, 2.7, 11.6);
    rect_fill(p, c, s, white, -5.9, -5.8, 11.8, 2.7);
    path_filled(p, c, s, accent, &[
        (0.0, -2.6), (2.4, 2.6), (0.0, 5.2), (-2.4, 2.6),
    ]);
}

fn glyph_trading(p: &Painter, c: Pos2, s: f32, white: Color32, accent: Color32) {
    rect_fill(p, c, s, white, -5.7, -4.9, 4.0, 4.0);
    rect_fill(p, c, s, white,  1.7, -4.9, 4.0, 4.0);
    rect_fill(p, c, s, white, -2.0,  1.1, 4.0, 4.0);
    path_filled(p, c, s, accent, &[
        (-3.7, -0.9), (0.0, 1.1), (3.7, -0.9),
        ( 3.7,  1.2), (0.0, 3.2), (-3.7, 1.2),
    ]);
}

fn glyph_equip_dock(p: &Painter, c: Pos2, s: f32, white: Color32, accent: Color32) {
    path_filled(p, c, s, white, &[
        (-5.9, -5.5), (-2.3, -5.5), (-2.3, -2.7),
        (-4.0, -2.7), (-4.0,  2.7), (-2.3,  2.7),
        (-2.3,  5.5), (-5.9,  5.5),
    ]);
    path_filled(p, c, s, white, &[
        ( 5.9, -5.5), ( 2.3, -5.5), ( 2.3, -2.7),
        ( 4.0, -2.7), ( 4.0,  2.7), ( 2.3,  2.7),
        ( 2.3,  5.5), ( 5.9,  5.5),
    ]);
    rect_fill(p, c, s, accent, -1.7, -1.7, 3.4, 3.4);
}

fn glyph_hq(p: &Painter, c: Pos2, s: f32, white: Color32, accent: Color32) {
    path_filled(p, c, s, white, &[
        ( 0.0, -6.2), ( 3.1, -3.4), ( 2.0, -1.2),
        ( 5.7,  1.2), ( 3.8,  5.5), ( 0.0,  3.6),
        (-3.8,  5.5), (-5.7,  1.2), (-2.0, -1.2),
        (-3.1, -3.4),
    ]);
    p.circle_filled(c, 1.8 * s, accent);
}

fn glyph_player_station(p: &Painter, c: Pos2, s: f32, white: Color32, accent: Color32) {
    rect_fill(p, c, s, accent, -5.8, -1.4, 3.1, 2.8);
    rect_fill(p, c, s, accent,  2.7, -1.4, 3.1, 2.8);
    rect_fill(p, c, s, accent, -1.4, -5.8, 2.8, 3.1);
    rect_fill(p, c, s, accent, -1.4,  2.7, 2.8, 3.1);
    path_filled(p, c, s, white, &[
        (0.0, -2.8), (2.1, 0.0), (0.0, 2.8), (-2.1, 0.0),
    ]);
}

fn glyph_generic_station(p: &Painter, c: Pos2, s: f32, white: Color32, accent: Color32) {
    rect_fill(p, c, s, white, -3.5, -3.5, 7.0, 7.0);
    rect_fill(p, c, s, white, -5.8, -1.9, 2.8, 3.8);
    rect_fill(p, c, s, white,  3.0, -1.9, 2.8, 3.8);
    rect_fill(p, c, s, white, -1.9, -5.8, 3.8, 2.8);
    p.circle_filled(c, 1.4 * s, accent);
}

fn glyph_defense(p: &Painter, c: Pos2, s: f32, white: Color32, accent: Color32) {
    path_filled(p, c, s, white, &[
        ( 0.0, -6.1), ( 3.2, -1.6), ( 1.5,  0.2),
        (-1.5,  0.2), (-3.2, -1.6),
    ]);
    path_filled(p, c, s, white, &[
        ( 5.9,  4.7), ( 0.8,  4.7), (-0.1,  2.4),
        ( 1.6,  0.8), ( 4.8,  1.6),
    ]);
    path_filled(p, c, s, white, &[
        (-5.9,  4.7), (-0.8,  4.7), ( 0.1,  2.4),
        (-1.6,  0.8), (-4.8,  1.6),
    ]);
    path_filled(p, c, s, accent, &[
        ( 0.0, -2.3), ( 2.4, -0.2), ( 1.5,  3.2),
        ( 0.0,  4.4), (-1.5,  3.2), (-2.4, -0.2),
    ]);
}

// -------------------------------------------------------------------------
// Ship glyphs — concave hulls in faction colour, no frame.
// -------------------------------------------------------------------------

fn glyph_capital(p: &Painter, c: Pos2, s: f32, col: Color32) {
    path_filled(p, c, s, col, &[
        ( 0.0, -7.8), ( 5.6, -2.2), ( 6.7,  4.8), ( 3.4,  5.8),
        ( 0.0,  7.5), (-3.4,  5.8), (-6.7,  4.8), (-5.6, -2.2),
    ]);
}

fn glyph_medium(p: &Painter, c: Pos2, s: f32, col: Color32) {
    path_filled(p, c, s, col, &[
        ( 0.0, -7.4), ( 1.2, -4.1), ( 4.7, -1.6), ( 2.2,  0.4),
        ( 1.2,  6.5), ( 0.0,  5.5), (-1.2,  6.5), (-2.2,  0.4),
        (-4.7, -1.6), (-1.2, -4.1),
    ]);
}

fn glyph_small(p: &Painter, c: Pos2, s: f32, col: Color32) {
    path_filled(p, c, s, col, &[
        ( 0.0, -5.6), ( 2.6, -3.8), ( 1.3, -1.5), ( 3.7,  0.7),
        ( 1.2,  1.0), ( 0.0,  4.8), (-1.2,  1.0), (-3.7,  0.7),
        (-1.3, -1.5), (-2.6, -3.8),
    ]);
}

fn glyph_transport(p: &Painter, c: Pos2, s: f32, col: Color32) {
    path_filled(p, c, s, col, &[
        ( 0.0, -7.4), ( 2.2, -5.8), ( 2.8, -4.0), ( 5.8, -4.0),
        ( 6.5, -2.8), ( 6.5,  4.6), ( 4.7,  6.4), (-4.7,  6.4),
        (-6.5,  4.6), (-6.5, -2.8), (-5.8, -4.0), (-2.8, -4.0),
        (-2.2, -5.8),
    ]);
}

// -------------------------------------------------------------------------
// Static glyphs — fixed grey + white, no faction tint.
// -------------------------------------------------------------------------

fn glyph_anomaly(p: &Painter, c: Pos2, s: f32) {
    p.circle_stroke(c, 4.7 * s, Stroke::new(2.0 * s, STATIC_FRAME_COLOR));
    path_filled(p, c, s, Color32::WHITE, &[
        (-1.1, -5.8), ( 3.2, -0.8), ( 1.1, -0.8),
        ( 2.2,  5.8), (-3.2,  0.5), (-1.0,  0.5),
    ]);
}

fn glyph_resource_zone(p: &Painter, c: Pos2, s: f32) {
    path_filled(p, c, s, STATIC_FRAME_COLOR, &[
        (-5.7, -2.9), (-3.1, -5.8), (-0.7, -3.7),
        (-1.4, -0.9), (-4.7,  0.2),
    ]);
    path_filled(p, c, s, Color32::WHITE, &[
        ( 2.2, -5.5), ( 5.8, -2.9), ( 4.5,  0.5), ( 1.2, -0.7),
    ]);
    path_filled(p, c, s, STATIC_FRAME_COLOR, &[
        (-1.9,  1.4), ( 1.5,  0.8), ( 4.8,  3.4),
        ( 2.7,  5.8), (-1.1,  5.4),
    ]);
}

// -------------------------------------------------------------------------
// Drawing helpers
// -------------------------------------------------------------------------

fn rect_fill(p: &Painter, c: Pos2, s: f32, col: Color32, x: f32, y: f32, w: f32, h: f32) {
    p.rect_filled(
        Rect::from_min_size(Pos2::new(c.x + x * s, c.y + y * s), Vec2::new(w * s, h * s)),
        0.0,
        col,
    );
}

/// Closed concave polygon fill. Design-grid coords; `s` scales to screen px.
fn path_filled(p: &Painter, c: Pos2, s: f32, col: Color32, pts: &[(f32, f32)]) {
    let points = pts
        .iter()
        .map(|(x, y)| Pos2::new(c.x + x * s, c.y + y * s))
        .collect::<Vec<_>>();
    p.add(egui::Shape::Path(egui::epaint::PathShape {
        points,
        closed: true,
        fill: col,
        stroke: egui::epaint::PathStroke::NONE,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_size_larger_than_normal() {
        assert!(HALF_SELECTED > HALF_NORMAL);
        assert!(STROKE_SELECTED > STROKE_NORMAL);
    }
}
