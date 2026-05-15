use map_domain::ids::ObjectId;
use map_domain::universe::Sector;
use crate::renderer::camera::OrbitCamera;

pub struct SectorViewResponse {
    pub close_clicked:  bool,
    pub clicked_object: Option<ObjectId>,
}

#[derive(Default)]
pub struct SectorView3D;

impl SectorView3D {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        sector: Option<&Sector>,
        camera: &mut OrbitCamera,
        selected_obj: Option<ObjectId>,
    ) -> SectorViewResponse {
        // Stub: dark rectangle with sector name
        let available = ui.available_rect_before_wrap();
        let painter = ui.painter_at(available);
        painter.rect_filled(available, 4.0, egui::Color32::from_rgb(5, 7, 12));

        if let Some(sector) = sector {
            painter.text(
                available.center_top() + egui::Vec2::new(0.0, 12.0),
                egui::Align2::CENTER_TOP,
                &sector.name,
                egui::FontId::proportional(14.0),
                crate::theme::TEXT_PRIMARY,
            );
        }

        // Suppress unused warnings for stub
        let _ = (camera, selected_obj);

        SectorViewResponse { close_clicked: false, clicked_object: None }
    }
}
