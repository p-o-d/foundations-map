use egui::{Pos2, Rect, Response, Sense, Stroke, Vec2};
use glam::Vec2 as GVec2;
use map_domain::ids::{ClusterId, FactionId, SectorId};
use map_domain::universe::{Universe, GateType};
use crate::theme;

/// Pixel-space layout offset for a sector inside its cluster.
/// Scales with `r` (which the renderer derives from `hex_r`), so sectors never
/// overlap at low zoom — unlike the previous map-unit offset that shrank with
/// `hex_r`'s clamped minimum.
fn hex_offset_pixels(index: u32, total: u32, r: f32) -> Vec2 {
    match (total, index) {
        (1, _) => Vec2::ZERO,
        (2, 0) => Vec2::new(-r * 0.5, 0.0),
        (2, 1) => Vec2::new( r * 0.5, 0.0),
        (3, 0) => Vec2::new(0.0, -r * 0.6),
        (3, 1) => Vec2::new(-r * 0.55, r * 0.4),
        (3, 2) => Vec2::new( r * 0.55, r * 0.4),
        (_, i) => {
            let theta = i as f32 / total as f32 * std::f32::consts::TAU;
            Vec2::new(r * theta.cos(), r * theta.sin())
        }
    }
}

fn faction_fill(id: FactionId) -> egui::Color32 {
    const PALETTE: &[(u8, u8, u8)] = &[
        (59,  130, 246),  // blue
        (34,  197, 94),   // green
        (168, 85,  247),  // purple
        (249, 115, 22),   // orange
        (20,  184, 166),  // teal
        (239, 68,  68),   // red
        (234, 179, 8),    // yellow
        (236, 72,  153),  // pink
    ];
    let (r, g, b) = PALETTE[id.0 as usize % PALETTE.len()];
    egui::Color32::from_rgba_unmultiplied(r, g, b, 60)
}

/// Returns quadratic bezier control point in universe space, or None if straight.
/// Works entirely in universe coordinates — zoom-invariant routing.
fn route_ctrl_point(
    from: GVec2,
    to: GVec2,
    obstacles: &[GVec2],
    clearance: f32,  // universe units
    from_id: u32,
    to_id: u32,
) -> Option<GVec2> {
    let line = to - from;
    let clearance_sq = clearance * clearance;

    let mut left: f32 = 0.0;
    let mut right: f32 = 0.0;
    for &obs in obstacles {
        let ab = line;
        let ap = obs - from;
        let len_sq = ab.dot(ab);
        let dist_sq = if len_sq < 1e-6 {
            ap.length_squared()
        } else {
            let t = (ap.dot(ab) / len_sq).clamp(0.0, 1.0);
            (obs - (from + ab * t)).length_squared()
        };
        if dist_sq < clearance_sq {
            // Cross product z: positive = left of from→to
            if line.x * ap.y - line.y * ap.x >= 0.0 { left += 1.0; } else { right += 1.0; }
        }
    }

    if left == 0.0 && right == 0.0 {
        return None;
    }

    let line_len = line.length().max(1e-6);
    let perp = GVec2::new(-line.y, line.x) / line_len;
    // Deterministic tiebreak by sector IDs to prevent flickering
    let sign: f32 = if left != right {
        if left > right { -1.0 } else { 1.0 }
    } else {
        if from_id < to_id { 1.0 } else { -1.0 }
    };
    let mid = (from + to) * 0.5;
    Some(mid + perp * (line_len * 0.25 * sign))
}

fn draw_connection(
    painter: &egui::Painter,
    fp: Pos2,
    tp: Pos2,
    ctrl_screen: Option<Pos2>,
    color: egui::Color32,
    width: f32,
) {
    match ctrl_screen {
        None => {
            painter.line_segment([fp, tp], Stroke::new(width, color));
        }
        Some(ctrl) => {
            let pts: Vec<Pos2> = (0..=12).map(|i| {
                let t = i as f32 / 12.0;
                let u = 1.0 - t;
                Pos2::new(
                    u * u * fp.x + 2.0 * u * t * ctrl.x + t * t * tp.x,
                    u * u * fp.y + 2.0 * u * t * ctrl.y + t * t * tp.y,
                )
            }).collect();
            painter.add(egui::Shape::line(pts, Stroke::new(width, color)));
        }
    }
}

fn draw_dotted_flow(
    painter: &egui::Painter,
    fp: Pos2,
    tp: Pos2,
    ctrl_screen: Option<Pos2>,
    color: egui::Color32,
    time: f32,
) {
    const NUM_DASHES: usize = 18;
    const DASH_LEN:   f32   = 0.025; // fraction of curve length
    const STROKE_W:   f32   = 2.0;
    const SPEED:      f32   = 0.05;

    let sample = |t: f32| -> Pos2 {
        match ctrl_screen {
            None => Pos2::new(fp.x + (tp.x - fp.x) * t, fp.y + (tp.y - fp.y) * t),
            Some(c) => {
                let u = 1.0 - t;
                Pos2::new(
                    u * u * fp.x + 2.0 * u * t * c.x + t * t * tp.x,
                    u * u * fp.y + 2.0 * u * t * c.y + t * t * tp.y,
                )
            }
        }
    };
    let phase = (time * SPEED).rem_euclid(1.0);
    for i in 0..NUM_DASHES {
        let t0 = ((i as f32 / NUM_DASHES as f32) + phase).rem_euclid(1.0);
        let t1 = (t0 + DASH_LEN).rem_euclid(1.0);
        // Skip dashes spanning the wrap-around or sitting on endpoints
        if t1 < t0 { continue; }
        if t0 < 0.04 || t1 > 0.96 { continue; }
        painter.line_segment([sample(t0), sample(t1)], Stroke::new(STROKE_W, color));
    }
}

pub struct MapView {
    pub pan: Vec2,
    pub zoom: f32,
    min_zoom: f32,
    fit_pending: bool,
    last_rect_size: Vec2,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 4.0,
            min_zoom: 1.0,
            fit_pending: true,
            last_rect_size: Vec2::ZERO,
        }
    }
}

impl MapView {
    pub fn universe_to_screen(&self, rect: Rect, pos: GVec2) -> Pos2 {
        let center = rect.center();
        Pos2::new(
            center.x + self.pan.x + pos.x * self.zoom,
            center.y + self.pan.y + pos.y * self.zoom,
        )
    }

    fn fit_to(&mut self, universe: &Universe, rect: Rect) {
        if universe.sectors.is_empty() {
            return;
        }
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_z = f32::INFINITY;
        let mut max_z = f32::NEG_INFINITY;
        for s in &universe.sectors {
            min_x = min_x.min(s.map_position.x);
            max_x = max_x.max(s.map_position.x);
            min_z = min_z.min(s.map_position.y);
            max_z = max_z.max(s.map_position.y);
        }
        let span_x = (max_x - min_x).max(1.0);
        let span_z = (max_z - min_z).max(1.0);
        let margin = 0.15;
        self.zoom = ((rect.width() / span_x).min(rect.height() / span_z) * (1.0 - margin))
            .clamp(1.0, 400.0);
        self.min_zoom = self.zoom;
        let cx = (min_x + max_x) / 2.0;
        let cz = (min_z + max_z) / 2.0;
        self.pan = Vec2::new(-cx * self.zoom, -cz * self.zoom);
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        universe: &Universe,
        selected: Option<SectorId>,
    ) -> MapViewResponse {
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size(),
            Sense::click_and_drag(),
        );

        // Re-fit on first frame or when window grows
        let grew = rect.width() > self.last_rect_size.x + 2.0
            || rect.height() > self.last_rect_size.y + 2.0;
        if self.fit_pending || grew {
            self.fit_to(universe, rect);
            self.fit_pending = false;
        }
        self.last_rect_size = rect.size();

        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, 0.0, theme::BG_DARK);

        // Sector nodes
        let mut clicked_sector: Option<SectorId> = None;
        let mut double_clicked_sector: Option<SectorId> = None;
        let hex_r = (self.zoom * 3.0).clamp(12.0, 80.0);

        let clicked_pos     = response.clicked().then(|| response.interact_pointer_pos()).flatten();
        let dbl_clicked_pos = response.double_clicked().then(|| response.interact_pointer_pos()).flatten();

        // Compute sector screen positions: cluster_center projected + per-sector
        // hex offset scaled by hex_r so sectors fit nicely inside the cluster hex
        // at any zoom level.
        let sector_layout_r = hex_r * 2.0;
        let sector_screens: Vec<(SectorId, Pos2)> = universe.sectors.iter()
            .map(|s| {
                let cluster_center = self.universe_to_screen(rect, s.map_position);
                let offset = hex_offset_pixels(s.index_in_cluster, s.cluster_sector_count, sector_layout_r);
                let pos = Pos2::new(cluster_center.x + offset.x, cluster_center.y + offset.y);
                (s.id, pos)
            })
            .collect();

        // Connections — routing computed in universe space (zoom-invariant)
        let obstacle_clearance_u = 3.0_f32;

        use std::collections::HashSet;
        let sh_pairs: HashSet<(SectorId, SectorId)> = universe.connections.iter()
            .filter(|c| matches!(c.gate_type, GateType::Superhighway))
            .map(|c| (c.from, c.to))
            .collect();

        let time = ui.input(|i| i.time) as f32;
        let mut needs_repaint = false;

        // Map cluster_id → sector count so we can skip drawing the cluster hex
        // for single-sector clusters (where it adds no info beyond the sector hex).
        let mut cluster_sector_counts: std::collections::HashMap<ClusterId, u32> =
            std::collections::HashMap::new();
        for s in &universe.sectors {
            if let Some(cid) = s.cluster_id {
                cluster_sector_counts.insert(cid, s.cluster_sector_count);
            }
        }

        // Cluster background hexes — drawn before sectors so they sit behind.
        // Sector positions are laid out in pixel space at radius `sector_layout_r`
        // from the cluster center, each with hex pixel radius `hex_r`, so the
        // enclosing flat-top hex's inscribed radius covers `sector_layout_r + hex_r`.
        for cluster in &universe.clusters {
            let count = cluster_sector_counts.get(&cluster.id).copied().unwrap_or(0);
            if count < 2 { continue; }
            let center = self.universe_to_screen(rect, cluster.map_position);
            let r_pixels = (sector_layout_r + hex_r) / 0.866 + 4.0;
            let pts: Vec<Pos2> = (0..6)
                .map(|i| {
                    let a = std::f32::consts::FRAC_PI_3 * i as f32;
                    Pos2::new(center.x + r_pixels * a.cos(), center.y + r_pixels * a.sin())
                })
                .collect();
            painter.add(egui::Shape::convex_polygon(
                pts,
                egui::Color32::from_rgba_unmultiplied(60, 70, 110, 25),
                Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(120, 130, 170, 80)),
            ));
            painter.text(
                Pos2::new(center.x, center.y - r_pixels - 4.0),
                egui::Align2::CENTER_BOTTOM,
                &cluster.name,
                egui::FontId::proportional(10.0),
                theme::TEXT_MUTED,
            );
        }

        for conn in &universe.connections {
            let from_s = universe.sector(conn.from);
            let to_s   = universe.sector(conn.to);
            let (Some(from_s), Some(to_s)) = (from_s, to_s) else { continue };

            let is_sh = matches!(conn.gate_type, GateType::Superhighway);
            let reverse_exists = is_sh && sh_pairs.contains(&(conn.to, conn.from));

            if is_sh && reverse_exists && conn.from.0 > conn.to.0 {
                continue;
            }

            let obstacles_u: Vec<GVec2> = universe.sectors.iter()
                .filter(|s| s.id != conn.from && s.id != conn.to)
                .map(|s| s.map_position)
                .collect();
            let ctrl_u = route_ctrl_point(
                from_s.map_position,
                to_s.map_position,
                &obstacles_u,
                obstacle_clearance_u,
                conn.from.0,
                conn.to.0,
            );
            let fp = sector_screens.iter().find(|(id, _)| *id == conn.from).map(|(_, p)| *p)
                .unwrap_or_else(|| self.universe_to_screen(rect, from_s.map_position));
            let tp = sector_screens.iter().find(|(id, _)| *id == conn.to).map(|(_, p)| *p)
                .unwrap_or_else(|| self.universe_to_screen(rect, to_s.map_position));
            let ctrl_screen = ctrl_u.map(|cu| self.universe_to_screen(rect, cu));

            match conn.gate_type {
                GateType::Standard => {
                    draw_connection(&painter, fp, tp, ctrl_screen, egui::Color32::from_rgb(80, 95, 150), 1.0);
                }
                GateType::Superhighway if reverse_exists => {
                    draw_connection(&painter, fp, tp, ctrl_screen, theme::GATE_GREEN, 2.5);
                }
                GateType::Superhighway => {
                    draw_dotted_flow(&painter, fp, tp, ctrl_screen, theme::GATE_GREEN, time);
                    needs_repaint = true;
                }
            }
        }

        if needs_repaint {
            ui.ctx().request_repaint();
        }

        for (sector, &(_, screen_pos)) in universe.sectors.iter().zip(sector_screens.iter()) {
            let is_selected = selected == Some(sector.id);

            let border_color = if is_selected { theme::ACCENT } else { theme::BORDER };
            let fill_color = if is_selected {
                egui::Color32::from_rgba_unmultiplied(124, 58, 237, 80)
            } else if let Some(fid) = sector.faction {
                faction_fill(fid)
            } else {
                theme::BG_WIDGET
            };
            let border_width = if is_selected { 2.0 } else { 1.0 };

            // Flat-top hexagon
            let pts: Vec<Pos2> = (0..6)
                .map(|i| {
                    let a = std::f32::consts::FRAC_PI_3 * i as f32;
                    Pos2::new(screen_pos.x + hex_r * a.cos(), screen_pos.y + hex_r * a.sin())
                })
                .collect();
            painter.add(egui::Shape::convex_polygon(
                pts,
                fill_color,
                Stroke::new(border_width, border_color),
            ));

            painter.text(
                Pos2::new(screen_pos.x, screen_pos.y + hex_r + 2.0),
                egui::Align2::CENTER_TOP,
                &sector.name,
                egui::FontId::proportional(9.0),
                if is_selected { theme::ACCENT } else { theme::TEXT_PRIMARY },
            );

            // Hit detection: circle around hex center
            let hit_r = hex_r + 2.0;
            if let Some(ptr) = clicked_pos {
                if (ptr - screen_pos).length() < hit_r {
                    clicked_sector = Some(sector.id);
                }
            }
            if let Some(ptr) = dbl_clicked_pos {
                if (ptr - screen_pos).length() < hit_r {
                    double_clicked_sector = Some(sector.id);
                }
            }
        }

        // Pan
        if response.dragged() {
            self.pan += response.drag_delta();
        }

        // Zoom toward pointer
        if let Some(hover_pos) = response.hover_pos() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0 {
                let zoom_factor = (scroll_delta * 0.001).exp();
                let old_zoom = self.zoom;
                self.zoom = (self.zoom * zoom_factor).clamp(self.min_zoom, 400.0);
                let center = rect.center();
                let mouse_offset = hover_pos - center;
                let scale_change = self.zoom / old_zoom;
                self.pan = mouse_offset + (self.pan - mouse_offset) * scale_change;
            }
        }

        MapViewResponse { clicked_sector, double_clicked_sector, response }
    }
}

pub struct MapViewResponse {
    pub clicked_sector: Option<SectorId>,
    pub double_clicked_sector: Option<SectorId>,
    pub response: Response,
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2 as GVec2;

    #[test]
    fn default_zoom_is_positive() {
        let mv = MapView::default();
        assert!(mv.zoom > 0.0);
    }

    #[test]
    fn universe_to_screen_center_at_origin() {
        let mv = MapView::default();
        let rect = Rect::from_center_size(Pos2::new(400.0, 300.0), Vec2::new(800.0, 600.0));
        let screen = mv.universe_to_screen(rect, GVec2::ZERO);
        assert_eq!(screen, Pos2::new(400.0, 300.0));
    }

    #[test]
    fn universe_to_screen_applies_zoom() {
        let mv = MapView { pan: Vec2::ZERO, zoom: 100.0, min_zoom: 1.0, fit_pending: false, last_rect_size: Vec2::ZERO };
        let rect = Rect::from_center_size(Pos2::new(400.0, 300.0), Vec2::new(800.0, 600.0));
        let screen = mv.universe_to_screen(rect, GVec2::new(1.0, 0.0));
        assert_eq!(screen.x, 500.0);
    }

    #[test]
    fn universe_to_screen_applies_pan() {
        let mv = MapView { pan: Vec2::new(50.0, -30.0), zoom: 80.0, min_zoom: 1.0, fit_pending: false, last_rect_size: Vec2::ZERO };
        let rect = Rect::from_center_size(Pos2::new(400.0, 300.0), Vec2::new(800.0, 600.0));
        let screen = mv.universe_to_screen(rect, GVec2::ZERO);
        assert_eq!(screen, Pos2::new(450.0, 270.0));
    }
}
