use egui::{Pos2, Rect, Sense, Vec2};
use glam::{Mat4, Vec3};
use map_domain::ids::ObjectId;
use map_domain::objects::StaticObjectKind;
use map_domain::universe::Sector;
use crate::renderer::camera::OrbitCamera;
use crate::renderer::gpu::{DrawCall, MeshKind, SceneCallback};
use crate::theme;

pub struct SectorViewResponse {
    pub close_clicked:  bool,
    pub clicked_object: Option<ObjectId>,
}

#[derive(Default)]
pub struct SectorView3D;

impl SectorView3D {
    pub fn show(
        &mut self,
        ui:           &mut egui::Ui,
        sector:       Option<&Sector>,
        camera:       &mut OrbitCamera,
        selected_obj: Option<ObjectId>,
    ) -> SectorViewResponse {
        let mut close_clicked  = false;
        let mut clicked_object = None;

        let available = ui.available_rect_before_wrap();
        let canvas_w  = available.width() * 0.80;
        let canvas_rect = Rect::from_min_size(
            available.min + Vec2::new((available.width() - canvas_w) * 0.5, 0.0),
            Vec2::new(canvas_w, available.height()),
        );

        // 30px header, remainder is 3D view
        let header_rect = Rect::from_min_size(canvas_rect.min, Vec2::new(canvas_w, 30.0));
        let view_rect   = Rect::from_min_size(
            canvas_rect.min + Vec2::new(0.0, 30.0),
            Vec2::new(canvas_w, available.height() - 30.0),
        );

        // Draw header background + label
        ui.painter().rect_filled(header_rect, 0.0, egui::Color32::from_rgb(15, 18, 28));
        if let Some(s) = sector {
            ui.painter().text(
                header_rect.center(),
                egui::Align2::CENTER_CENTER,
                &s.name,
                egui::FontId::proportional(13.0),
                theme::TEXT_PRIMARY,
            );
        }

        // Close button (✕) — top-right of header
        let close_center = Pos2::new(header_rect.right() - 20.0, header_rect.center().y);
        let close_rect2 = Rect::from_center_size(close_center, Vec2::splat(20.0));
        ui.painter().text(
            close_center,
            egui::Align2::CENTER_CENTER,
            "✕",
            egui::FontId::proportional(12.0),
            theme::TEXT_MUTED,
        );
        if ui.allocate_rect(close_rect2, Sense::click()).clicked() {
            close_clicked = true;
        }

        // 3D canvas interaction (drag = rotate, scroll = zoom)
        let canvas_resp = ui.allocate_rect(view_rect, Sense::click_and_drag());
        if canvas_resp.dragged() {
            let delta = canvas_resp.drag_delta();
            camera.rotate(delta.x * 0.005, -delta.y * 0.005);
        }
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if canvas_resp.contains_pointer() && scroll != 0.0 {
            camera.zoom(scroll * 0.01);
        }

        // Screen-space object picking on click
        if canvas_resp.clicked() {
            if let (Some(pos), Some(sec)) = (canvas_resp.interact_pointer_pos(), sector) {
                clicked_object = pick_object(pos, view_rect, camera, sec);
            }
        }

        // Build draw calls (model matrix only; view/proj applied below)
        let draw_calls_model = sector.map(|s| build_draw_calls(s, selected_obj)).unwrap_or_default();

        // Apply view + projection to get final MVP per draw call
        let aspect = view_rect.width() / view_rect.height().max(1.0);
        let view   = camera.view_matrix();
        let proj   = camera.proj_matrix(aspect);
        let vp     = proj * view;
        let draw_calls: Vec<DrawCall> = draw_calls_model.into_iter().map(|dc| DrawCall {
            kind:  dc.kind,
            mvp:   vp * dc.mvp,
            color: dc.color,
        }).collect();

        // Push wgpu paint callback for the 3D view rect
        let cb = eframe::egui_wgpu::Callback::new_paint_callback(
            view_rect,
            SceneCallback { draw_calls },
        );
        ui.painter().add(cb);

        // Axis orientation arrows (N/S/E/W/Up/Down) drawn on top of 3D scene
        draw_axis_arrows(ui.painter(), view_rect, camera);

        // Border around canvas
        ui.painter().rect_stroke(canvas_rect, 2.0, egui::Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Outside);

        SectorViewResponse { close_clicked, clicked_object }
    }
}

/// Build per-object draw calls with model matrix (translate + scale) and color.
fn build_draw_calls(sector: &Sector, selected: Option<ObjectId>) -> Vec<DrawCall> {
    sector.static_objects.iter().map(|obj| {
        let scale = match obj.kind {
            StaticObjectKind::Station      => 3.0,
            StaticObjectKind::Gate         => 4.0,
            StaticObjectKind::ResourceZone => 8.0,
            StaticObjectKind::Anomaly      => 2.0,
        };
        let kind = match obj.kind {
            StaticObjectKind::Station      => MeshKind::Box,
            StaticObjectKind::Gate         => MeshKind::Ring,
            StaticObjectKind::ResourceZone => MeshKind::Sphere,
            StaticObjectKind::Anomaly      => MeshKind::Sphere,
        };
        let color = if selected == Some(obj.id) {
            [1.0, 0.8, 0.1, 1.0]  // yellow = selected
        } else {
            kind_color(&obj.kind)
        };
        let model = Mat4::from_translation(obj.position)
            * Mat4::from_scale(Vec3::splat(scale));
        DrawCall { kind, mvp: model, color }
    }).collect()
}

fn kind_color(kind: &StaticObjectKind) -> [f32; 4] {
    match kind {
        StaticObjectKind::Station      => [0.4, 0.6, 1.0, 1.0],
        StaticObjectKind::Gate         => [0.2, 0.9, 0.4, 1.0],
        StaticObjectKind::ResourceZone => [0.5, 0.3, 0.9, 0.5],
        StaticObjectKind::Anomaly      => [1.0, 0.4, 0.2, 1.0],
    }
}

/// Project each object to screen space; return id of object nearest to click (within 20px).
fn pick_object(
    ptr:    Pos2,
    rect:   Rect,
    camera: &OrbitCamera,
    sector: &Sector,
) -> Option<ObjectId> {
    let aspect = rect.width() / rect.height().max(1.0);
    let vp     = camera.proj_matrix(aspect) * camera.view_matrix();
    let mut best_id   = None;
    let mut best_dist = f32::MAX;

    for obj in &sector.static_objects {
        let clip = vp * obj.position.extend(1.0);
        if clip.w <= 0.0 { continue; }
        let ndc = clip.truncate() / clip.w;
        let sx  = (ndc.x * 0.5 + 0.5) * rect.width()  + rect.left();
        let sy  = (1.0 - (ndc.y * 0.5 + 0.5)) * rect.height() + rect.top();
        let dist = ((sx - ptr.x).powi(2) + (sy - ptr.y).powi(2)).sqrt();
        if dist < 20.0 && dist < best_dist {
            best_dist = dist;
            best_id   = Some(obj.id);
        }
    }
    best_id
}

/// Draw 6 direction arrows from sector origin: E(+X), W(-X), Up(+Y), Dn(-Y), N(-Z), S(+Z).
/// Arrow length scales with camera distance so they remain visible at all zoom levels.
fn draw_axis_arrows(painter: &egui::Painter, view_rect: Rect, camera: &OrbitCamera) {
    let aspect = view_rect.width() / view_rect.height().max(1.0);
    let vp  = camera.proj_matrix(aspect) * camera.view_matrix();
    let arm = camera.distance * 0.15;

    let center = camera.target;
    let axes: &[(&str, Vec3, egui::Color32)] = &[
        ("E",  center + Vec3::new( arm, 0.0,  0.0), egui::Color32::from_rgb(220,  80,  80)),
        ("W",  center + Vec3::new(-arm, 0.0,  0.0), egui::Color32::from_rgb(160,  50,  50)),
        ("Up", center + Vec3::new(0.0,  arm,  0.0), egui::Color32::from_rgb( 80, 220,  80)),
        ("Dn", center + Vec3::new(0.0, -arm,  0.0), egui::Color32::from_rgb( 50, 130,  50)),
        ("N",  center + Vec3::new(0.0, 0.0, -arm),  egui::Color32::from_rgb( 80, 180, 220)),
        ("S",  center + Vec3::new(0.0, 0.0,  arm),  egui::Color32::from_rgb(220, 140,  50)),
    ];

    let project = |world: Vec3| -> Option<Pos2> {
        let clip = vp * world.extend(1.0);
        if clip.w <= 0.0 { return None; }
        let ndc = clip.truncate() / clip.w;
        if ndc.x.abs() > 1.5 || ndc.y.abs() > 1.5 { return None; }
        Some(Pos2::new(
            (ndc.x * 0.5 + 0.5) * view_rect.width()  + view_rect.left(),
            (1.0 - (ndc.y * 0.5 + 0.5)) * view_rect.height() + view_rect.top(),
        ))
    };

    let Some(origin) = project(center) else { return };

    for (label, end_world, color) in axes {
        let Some(end) = project(*end_world) else { continue };
        painter.line_segment([origin, end], egui::Stroke::new(2.0, *color));

        // Arrowhead triangle
        let dir  = (end - origin).normalized();
        let perp = Vec2::new(-dir.y, dir.x);
        painter.add(egui::Shape::convex_polygon(
            vec![end, end - dir * 8.0 + perp * 4.0, end - dir * 8.0 - perp * 4.0],
            *color,
            egui::Stroke::NONE,
        ));

        // Label just beyond arrowhead
        painter.text(
            end + dir * 10.0,
            egui::Align2::CENTER_CENTER,
            *label,
            egui::FontId::monospace(10.0),
            *color,
        );
    }
}
