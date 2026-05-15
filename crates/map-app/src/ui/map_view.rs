use egui::{Pos2, Rect, Response, Sense, Vec2};
use glam::Vec2 as GVec2;
use map_domain::ids::SectorId;
use map_domain::universe::{Universe, GateType};
use crate::theme;

pub struct MapView {
    pub pan: Vec2,   // offset in screen pixels
    pub zoom: f32,   // pixels per universe unit
}

impl Default for MapView {
    fn default() -> Self {
        Self { pan: Vec2::ZERO, zoom: 80.0 }
    }
}

impl MapView {
    /// Convert universe coordinates to screen position within the map rect.
    pub fn universe_to_screen(&self, rect: Rect, pos: GVec2) -> Pos2 {
        let center = rect.center();
        Pos2::new(
            center.x + self.pan.x + pos.x * self.zoom,
            center.y + self.pan.y + pos.y * self.zoom,
        )
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

        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, 0.0, theme::BG_DARK);

        // Connections
        for conn in &universe.connections {
            let from = universe.sector(conn.from).map(|s| s.map_position);
            let to   = universe.sector(conn.to).map(|s| s.map_position);
            if let (Some(f), Some(t)) = (from, to) {
                let fp = self.universe_to_screen(rect, f);
                let tp = self.universe_to_screen(rect, t);
                let color = match conn.gate_type {
                    GateType::Standard     => theme::ACCENT_DIM,
                    GateType::Superhighway => theme::GATE_GREEN,
                };
                painter.line_segment([fp, tp], (1.5, color));
            }
        }

        // Sector nodes
        let mut clicked_sector: Option<SectorId> = None;
        let mut double_clicked_sector: Option<SectorId> = None;

        for sector in &universe.sectors {
            let screen_pos = self.universe_to_screen(rect, sector.map_position);
            let half = Vec2::new(36.0, 20.0);
            let node_rect = Rect::from_center_size(screen_pos, 2.0 * half);

            let is_selected = selected == Some(sector.id);
            let border_color = if is_selected { theme::ACCENT } else { theme::BORDER };
            let fill_color   = if is_selected {
                egui::Color32::from_rgba_premultiplied(124, 58, 237, 30)
            } else {
                theme::BG_WIDGET
            };
            let border_width = if is_selected { 2.0 } else { 1.0 };

            painter.rect(node_rect, 2.0, fill_color, (border_width, border_color));
            painter.text(
                screen_pos,
                egui::Align2::CENTER_CENTER,
                &sector.name,
                egui::FontId::proportional(10.0),
                theme::TEXT_PRIMARY,
            );

            // Hit detection
            if response.clicked() {
                if let Some(ptr) = response.interact_pointer_pos() {
                    if node_rect.contains(ptr) {
                        clicked_sector = Some(sector.id);
                    }
                }
            }
            if response.double_clicked() {
                if let Some(ptr) = response.interact_pointer_pos() {
                    if node_rect.contains(ptr) {
                        double_clicked_sector = Some(sector.id);
                    }
                }
            }
        }

        // Pan: drag anywhere on the map
        if response.dragged() {
            self.pan += response.drag_delta();
        }

        // Zoom: scroll wheel, zooming toward pointer position
        if let Some(hover_pos) = response.hover_pos() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0 {
                let zoom_factor = (scroll_delta * 0.001).exp();
                let old_zoom = self.zoom;
                self.zoom = (self.zoom * zoom_factor).clamp(20.0, 400.0);
                // Adjust pan so zoom targets the pointer position
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
        let mv = MapView { pan: Vec2::ZERO, zoom: 100.0 };
        let rect = Rect::from_center_size(Pos2::new(400.0, 300.0), Vec2::new(800.0, 600.0));
        let screen = mv.universe_to_screen(rect, GVec2::new(1.0, 0.0));
        assert_eq!(screen.x, 500.0); // 400 + 1.0 * 100
    }

    #[test]
    fn universe_to_screen_applies_pan() {
        let mv = MapView { pan: Vec2::new(50.0, -30.0), zoom: 80.0 };
        let rect = Rect::from_center_size(Pos2::new(400.0, 300.0), Vec2::new(800.0, 600.0));
        let screen = mv.universe_to_screen(rect, GVec2::ZERO);
        assert_eq!(screen, Pos2::new(450.0, 270.0));
    }
}
