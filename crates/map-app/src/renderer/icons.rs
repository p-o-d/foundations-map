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
pub const DOT_RADIUS:      f32 = 1.6;
pub const DOT_RADIUS_SEL:  f32 = 2.0;

pub const SELECTION_COLOR: Color32 = Color32::from_rgb(255, 217, 25);
const STATIC_FRAME_COLOR: Color32 = Color32::from_rgb(140, 140, 140);

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

/// Paint the inner glyph for the given IconId at `center`. `half` is the icon
/// half-size in screen pixels (HALF_NORMAL or HALF_SELECTED). `frame_color` is
/// passed through to a couple of glyphs that need it (HQ command dot, Transport
/// dividers).
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
        // -- stations --
        IconId::Factory        => glyph_factory(painter, center, s, white),
        IconId::WharfShipyard  => glyph_wharf_shipyard(painter, center, s, white),
        IconId::Defense        => glyph_defense(painter, center, s, white),
        IconId::Trading        => glyph_trading(painter, center, s, white),
        IconId::EquipDock      => glyph_equip_dock(painter, center, s, white),
        IconId::HQ             => glyph_hq(painter, center, s, white, frame_color),
        IconId::PlayerStation  => glyph_player_station(painter, center, s, white),
        IconId::GenericStation => glyph_generic_station(painter, center, s, white),

        // -- ships --
        IconId::Capital   => glyph_capital(painter, center, s, white),
        IconId::Medium    => glyph_medium(painter, center, s, white),
        IconId::Small     => glyph_small(painter, center, s, white),
        IconId::Transport => glyph_transport(painter, center, s, white, frame_color),

        // -- static --
        IconId::Anomaly      => glyph_anomaly(painter, center, s, white),
        IconId::ResourceZone => glyph_resource_zone(painter, center, s, white),
    }
}

// -------------------------------------------------------------------------
// Station glyphs
// -------------------------------------------------------------------------

fn glyph_factory(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // stepped refinery silhouette
    let pts = [
        (-3.0, 3.0),
        (-3.0, 0.0),
        (0.0, 2.0),
        (0.0, -2.0),
        (3.0, 0.0),
        (3.0, -3.0),
        (4.0, -3.0),
        (4.0, 3.0),
    ]
    .into_iter()
    .map(|(x, y)| Pos2::new(c.x + x * s, c.y + y * s))
    .collect::<Vec<_>>();
    p.add(egui::Shape::Path(egui::epaint::PathShape {
        points: pts,
        closed: true,
        fill: col,
        stroke: egui::epaint::PathStroke::NONE,
    }));
}

fn glyph_wharf_shipyard(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // L-shaped crane jib + load box
    let stroke = Stroke::new(1.6 * s, col);
    let post_top = Pos2::new(c.x - 3.0 * s, c.y - 3.0 * s);
    let post_bot = Pos2::new(c.x - 3.0 * s, c.y + 3.0 * s);
    let arm_end  = Pos2::new(c.x + 3.0 * s, c.y - 3.0 * s);
    p.line_segment([post_top, post_bot], stroke);
    p.line_segment([post_top, arm_end], stroke);
    let load = Rect::from_min_size(
        Pos2::new(c.x + 1.0 * s, c.y - 1.0 * s),
        Vec2::new(3.0 * s, 3.0 * s),
    );
    p.rect_filled(load, 0.0, col);
}

fn glyph_defense(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // heraldic shield
    let pts = [
        (0.0, -4.0),
        (4.0, -3.0),
        (3.0, 2.0),
        (0.0, 4.0),
        (-3.0, 2.0),
        (-4.0, -3.0),
    ]
    .into_iter()
    .map(|(x, y)| Pos2::new(c.x + x * s, c.y + y * s))
    .collect::<Vec<_>>();
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
}

fn glyph_trading(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // 3-coin stack — wide ellipses approximated as 24-segment polygons
    for dy in [-3.5_f32, 0.0, 3.5] {
        ellipse_filled(p, Pos2::new(c.x, c.y + dy * s), 5.0 * s, 1.2 * s, col);
    }
}

fn glyph_equip_dock(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // thick repair plus
    let v = Rect::from_center_size(c, Vec2::new(2.0 * s, 12.0 * s));
    let h = Rect::from_center_size(c, Vec2::new(12.0 * s, 2.0 * s));
    p.rect_filled(v, 0.0, col);
    p.rect_filled(h, 0.0, col);
}

fn glyph_hq(p: &Painter, c: Pos2, s: f32, col: Color32, dot_color: Color32) {
    // pyramid + command dot. Dot uses frame_color — faction tint (normal) or
    // selection yellow. Player-owned stations route to PlayerStation, not HQ,
    // so the white-frame case never lands here.
    let pts = vec![
        Pos2::new(c.x + 0.0 * s, c.y - 3.5 * s),
        Pos2::new(c.x + 5.0 * s, c.y + 4.0 * s),
        Pos2::new(c.x - 5.0 * s, c.y + 4.0 * s),
    ];
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
    p.circle_filled(Pos2::new(c.x, c.y + 1.5 * s), 1.4 * s, dot_color);
}

fn glyph_player_station(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // inner diamond (frame is drawn separately with white stroke by caller)
    let pts = vec![
        Pos2::new(c.x + 0.0 * s, c.y - 5.0 * s),
        Pos2::new(c.x + 5.0 * s, c.y + 0.0 * s),
        Pos2::new(c.x + 0.0 * s, c.y + 5.0 * s),
        Pos2::new(c.x - 5.0 * s, c.y + 0.0 * s),
    ];
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
}

fn glyph_generic_station(p: &Painter, c: Pos2, s: f32, col: Color32) {
    p.circle_stroke(c, 3.5 * s, Stroke::new(1.6 * s, col));
}

/// 24-segment polygon ellipse — egui doesn't have a built-in filled ellipse.
fn ellipse_filled(p: &Painter, center: Pos2, rx: f32, ry: f32, col: Color32) {
    const N: usize = 24;
    let pts: Vec<Pos2> = (0..N)
        .map(|i| {
            let t = (i as f32) * std::f32::consts::TAU / (N as f32);
            Pos2::new(center.x + rx * t.cos(), center.y + ry * t.sin())
        })
        .collect();
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
}

// Ship glyphs — Task 5
fn glyph_capital(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // 3 vertical tally bars
    for dx in [-3.5_f32, 0.0, 3.5] {
        let r = Rect::from_center_size(
            Pos2::new(c.x + dx * s, c.y),
            Vec2::new(2.0 * s, 10.0 * s),
        );
        p.rect_filled(r, 0.0, col);
    }
}

fn glyph_medium(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // 2 vertical tally bars
    for dx in [-2.0_f32, 2.0] {
        let r = Rect::from_center_size(
            Pos2::new(c.x + dx * s, c.y),
            Vec2::new(2.0 * s, 10.0 * s),
        );
        p.rect_filled(r, 0.0, col);
    }
}

fn glyph_small(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // 1 vertical tally bar
    let r = Rect::from_center_size(c, Vec2::new(2.0 * s, 10.0 * s));
    p.rect_filled(r, 0.0, col);
}

fn glyph_transport(p: &Painter, c: Pos2, s: f32, col: Color32, div: Color32) {
    // 2 stacked horizontal containers + faction-coloured dividers
    let top = Rect::from_min_size(
        Pos2::new(c.x - 7.0 * s, c.y - 5.0 * s),
        Vec2::new(14.0 * s, 3.5 * s),
    );
    let bot = Rect::from_min_size(
        Pos2::new(c.x - 7.0 * s, c.y + 1.0 * s),
        Vec2::new(14.0 * s, 3.5 * s),
    );
    p.rect_filled(top, 0.0, col);
    p.rect_filled(bot, 0.0, col);
    let div_stroke = Stroke::new(0.8 * s, div);
    p.line_segment(
        [Pos2::new(c.x, c.y - 5.0 * s), Pos2::new(c.x, c.y - 1.5 * s)],
        div_stroke,
    );
    p.line_segment(
        [Pos2::new(c.x, c.y + 1.0 * s), Pos2::new(c.x, c.y + 4.5 * s)],
        div_stroke,
    );
}

// Static glyphs — Task 6
fn glyph_anomaly(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // 4-point starburst — concave 8-vertex polygon
    let pts = [
        (0.0, -5.0),
        (1.5, -1.5),
        (5.0, 0.0),
        (1.5, 1.5),
        (0.0, 5.0),
        (-1.5, 1.5),
        (-5.0, 0.0),
        (-1.5, -1.5),
    ]
    .into_iter()
    .map(|(x, y)| Pos2::new(c.x + x * s, c.y + y * s))
    .collect::<Vec<_>>();
    p.add(egui::Shape::Path(egui::epaint::PathShape {
        points: pts,
        closed: true,
        fill: col,
        stroke: egui::epaint::PathStroke::NONE,
    }));
}

fn glyph_resource_zone(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // irregular cluster of 4 asteroid circles
    for (dx, dy, r) in [
        (-3.5_f32, -3.0_f32, 1.6_f32),
        (3.0, -4.0, 1.2),
        (0.0, 2.5, 2.0),
        (5.0, 4.0, 1.4),
    ] {
        p.circle_filled(Pos2::new(c.x + dx * s, c.y + dy * s), r * s, col);
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
