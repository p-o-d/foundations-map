use crate::renderer::camera::OrbitCamera;
use crate::renderer::gpu::{DrawCall, SceneCallback};
use crate::theme;
use egui::{Pos2, Rect, Sense, Vec2};
use glam::Vec3;
use map_domain::ids::ObjectId;
use map_domain::objects::StaticObjectKind;
use map_domain::universe::Sector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickedTarget {
    Static(ObjectId),
    Entity(map_domain::world::EntityId),
}

pub struct SectorViewResponse {
    pub close_clicked: bool,
    pub clicked: Option<ClickedTarget>,
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
        selected_entity: Option<map_domain::world::EntityId>,
        world: Option<&map_domain::world::World>,
        universe: &map_domain::universe::Universe,
    ) -> SectorViewResponse {
        let mut close_clicked = false;

        let available = ui.available_rect_before_wrap();
        let canvas_w = available.width() * 0.80;
        let canvas_rect = Rect::from_min_size(
            available.min + Vec2::new((available.width() - canvas_w) * 0.5, 0.0),
            Vec2::new(canvas_w, available.height()),
        );

        // 30px header, remainder is 3D view
        let header_rect = Rect::from_min_size(canvas_rect.min, Vec2::new(canvas_w, 30.0));
        let view_rect = Rect::from_min_size(
            canvas_rect.min + Vec2::new(0.0, 30.0),
            Vec2::new(canvas_w, available.height() - 30.0),
        );

        // Draw header background + label
        ui.painter()
            .rect_filled(header_rect, 0.0, egui::Color32::from_rgb(15, 18, 28));
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

        // Build draw calls (model matrix only; view/proj applied below)
        let draw_calls_model = sector
            .map(|s| build_draw_calls(s, world, universe, selected_obj, selected_entity))
            .unwrap_or_default();

        // Apply view + projection to get final MVP per draw call
        let aspect = view_rect.width() / view_rect.height().max(1.0);
        let view = camera.view_matrix();
        let proj = camera.proj_matrix(aspect);
        let vp = proj * view;
        let draw_calls: Vec<DrawCall> = draw_calls_model
            .into_iter()
            .map(|dc| DrawCall {
                kind: dc.kind,
                mvp: vp * dc.mvp,
                color: dc.color,
            })
            .collect();

        // Push wgpu paint callback for the 3D view rect (mesh pipeline only;
        // sprite path retired in favour of painter-based icon rendering below).
        let cb = eframe::egui_wgpu::Callback::new_paint_callback(
            view_rect,
            SceneCallback { draw_calls },
        );
        ui.painter().add(cb);

        // Gates drawn as screen-space circles + arrows (constant size, correct orientation)
        if let Some(sec) = sector {
            draw_gates_2d(ui.painter(), view_rect, camera, sec);
        }

        // Axis orientation arrows (N/S/E/W/Up/Down) drawn on top of 3D scene
        draw_axis_arrows(ui.painter(), view_rect, camera);

        // Screen-space icon overlay (stations + live entities).
        if let Some(sec) = sector {
            draw_icons_2d(
                ui.painter(),
                view_rect,
                camera,
                sec,
                world,
                universe,
                selected_obj,
                selected_entity,
            );
        }

        // Click + hover detection — unified across static + live targets.
        let mut clicked: Option<ClickedTarget> = None;
        if canvas_resp.clicked() {
            if let (Some(pos), Some(sec)) = (canvas_resp.interact_pointer_pos(), sector) {
                clicked = pick_target(pos, view_rect, camera, sec, world);
            }
        }
        let hovered = canvas_resp
            .hover_pos()
            .and_then(|pos| sector.and_then(|sec| pick_target(pos, view_rect, camera, sec, world)));

        // Hover label: tooltip-style box over the hovered target
        if let (Some(target), Some(sec)) = (hovered, sector) {
            draw_hover_label(
                ui.painter(),
                view_rect,
                camera,
                sec,
                world,
                universe,
                target,
            );
        }

        // Border around canvas
        ui.painter().rect_stroke(
            canvas_rect,
            2.0,
            egui::Stroke::new(1.0, theme::BORDER),
            egui::StrokeKind::Outside,
        );

        SectorViewResponse {
            close_clicked,
            clicked,
        }
    }
}

/// Build per-object draw calls with model matrix (translate + scale) and color.
fn build_draw_calls(
    _sector: &Sector,
    _world: Option<&map_domain::world::World>,
    _universe: &map_domain::universe::Universe,
    _selected_obj: Option<ObjectId>,
    _selected_entity: Option<map_domain::world::EntityId>,
) -> Vec<DrawCall> {
    // Sprites now own all entity rendering (built in build_sprite_instances).
    // Gates + highways render through draw_gates_2d. Mesh pipeline is currently
    // idle; kept in place for potential future grid / ground-plane work.
    Vec::new()
}

/// Project each entity (static objects + live ships/stations) to screen space
/// and paint a ring + glyph at the result. Pure 2D-over-3D — guaranteed
/// constant pixel size regardless of camera distance.
fn draw_icons_2d(
    painter: &egui::Painter,
    view_rect: egui::Rect,
    camera: &OrbitCamera,
    sector: &Sector,
    world: Option<&map_domain::world::World>,
    universe: &map_domain::universe::Universe,
    selected_obj: Option<ObjectId>,
    selected_entity: Option<map_domain::world::EntityId>,
) {
    use crate::renderer::atlas::{IconId, SuperCategory, classify_live, classify_static};
    use crate::renderer::icons;

    let aspect = view_rect.width() / view_rect.height().max(1.0);
    let vp = camera.proj_matrix(aspect) * camera.view_matrix();

    let project = |w_pos: Vec3| -> Option<Pos2> {
        let clip = vp * w_pos.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.x.abs() > 1.5 || ndc.y.abs() > 1.5 {
            return None;
        }
        Some(Pos2::new(
            (ndc.x * 0.5 + 0.5) * view_rect.width() + view_rect.left(),
            (1.0 - (ndc.y * 0.5 + 0.5)) * view_rect.height() + view_rect.top(),
        ))
    };

    let emit = |screen: Pos2, icon: IconId, faction_color: egui::Color32, selected: bool| {
        let (half, stroke) = if selected {
            (icons::HALF_SELECTED, icons::STROKE_SELECTED)
        } else {
            (icons::HALF_NORMAL, icons::STROKE_NORMAL)
        };
        let frame_color = if selected {
            icons::SELECTION_COLOR
        } else {
            faction_color
        };
        match icon.super_category() {
            SuperCategory::Station => {
                icons::draw_station_frame(painter, screen, half, stroke, frame_color)
            }
            SuperCategory::Ship => {} // ships render glyph only, no frame
            SuperCategory::Static => icons::draw_static_frame(painter, screen, half, stroke),
        }
        icons::draw_glyph(painter, icon, screen, half, frame_color);
    };

    // Static objects.
    for obj in &sector.static_objects {
        let Some(icon) = classify_static(&obj.kind) else {
            continue;
        };
        let Some(screen) = project(obj.position) else {
            continue;
        };
        let ring = obj
            .faction
            .map(|f| crate::colors::faction_color(universe, f))
            .unwrap_or(crate::theme::TEXT_MUTED);
        emit(screen, icon, ring, selected_obj == Some(obj.id));
    }

    // Live entities (top-level only).
    if let Some(world) = world {
        let mut fid_to_str: std::collections::HashMap<map_domain::ids::FactionId, &str> =
            std::collections::HashMap::new();
        for (k, v) in &universe.faction_strings {
            fid_to_str.insert(*v, k.as_str());
        }
        for &eid in world.entities_in_sector(sector.id) {
            if world.parent_of(eid).is_some() {
                continue;
            }
            let Some(&pos) = world.positions.get(&eid) else {
                continue;
            };
            let kind = match world.kinds.get(&eid) {
                Some(k) => k.clone(),
                None => continue,
            };
            let Some(screen) = project(pos) else { continue };
            let macro_name = world.names.get(&eid).map(String::as_str).unwrap_or("");
            let owner_str = world
                .factions
                .get(&eid)
                .and_then(|f| fid_to_str.get(f).copied());
            let icon = classify_live(kind, macro_name, owner_str);
            let ring = world
                .factions
                .get(&eid)
                .copied()
                .map(|f| crate::colors::faction_color(universe, f))
                .unwrap_or(crate::theme::TEXT_MUTED);
            emit(screen, icon, ring, selected_entity == Some(eid));
        }
    }
}

/// Project each object and entity to screen space; return the target nearest to click (within 24px).
fn pick_target(
    ptr: egui::Pos2,
    rect: egui::Rect,
    camera: &OrbitCamera,
    sector: &Sector,
    world: Option<&map_domain::world::World>,
) -> Option<ClickedTarget> {
    let aspect = rect.width() / rect.height().max(1.0);
    let vp = camera.proj_matrix(aspect) * camera.view_matrix();
    let mut best: Option<(f32, ClickedTarget)> = None;

    let project = |w_pos: Vec3| -> Option<egui::Pos2> {
        let clip = vp * w_pos.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(egui::Pos2::new(
            (ndc.x * 0.5 + 0.5) * rect.width() + rect.left(),
            (1.0 - (ndc.y * 0.5 + 0.5)) * rect.height() + rect.top(),
        ))
    };

    let consider =
        |sp: egui::Pos2, target: ClickedTarget, best: &mut Option<(f32, ClickedTarget)>| {
            let d = ((sp.x - ptr.x).powi(2) + (sp.y - ptr.y).powi(2)).sqrt();
            if d < 24.0 && best.as_ref().map_or(true, |(b, _)| d < *b) {
                *best = Some((d, target));
            }
        };

    for obj in &sector.static_objects {
        if let Some(sp) = project(obj.position) {
            consider(sp, ClickedTarget::Static(obj.id), &mut best);
        }
    }
    if let Some(world) = world {
        for &eid in world.entities_in_sector(sector.id) {
            if world.parent_of(eid).is_some() {
                continue;
            }
            let Some(&pos) = world.positions.get(&eid) else {
                continue;
            };
            if let Some(sp) = project(pos) {
                consider(sp, ClickedTarget::Entity(eid), &mut best);
            }
        }
    }
    best.map(|(_, t)| t)
}

/// Draw 6 direction arrows from sector origin: E(+X), W(-X), Up(+Y), Dn(-Y), N(-Z), S(+Z).
/// Arrow length scales with camera distance so they remain visible at all zoom levels.
fn draw_axis_arrows(painter: &egui::Painter, view_rect: Rect, camera: &OrbitCamera) {
    let aspect = view_rect.width() / view_rect.height().max(1.0);
    let vp = camera.proj_matrix(aspect) * camera.view_matrix();
    let arm = camera.distance * 0.15;

    let center = Vec3::ZERO;
    let axes: &[(&str, Vec3, egui::Color32)] = &[
        (
            "E",
            center + Vec3::new(arm, 0.0, 0.0),
            egui::Color32::from_rgb(220, 80, 80),
        ),
        (
            "W",
            center + Vec3::new(-arm, 0.0, 0.0),
            egui::Color32::from_rgb(160, 50, 50),
        ),
        (
            "Up",
            center + Vec3::new(0.0, arm, 0.0),
            egui::Color32::from_rgb(80, 220, 80),
        ),
        (
            "Dn",
            center + Vec3::new(0.0, -arm, 0.0),
            egui::Color32::from_rgb(50, 130, 50),
        ),
        (
            "N",
            center + Vec3::new(0.0, 0.0, -arm),
            egui::Color32::from_rgb(80, 180, 220),
        ),
        (
            "S",
            center + Vec3::new(0.0, 0.0, arm),
            egui::Color32::from_rgb(220, 140, 50),
        ),
    ];

    let project = |world: Vec3| -> Option<Pos2> {
        let clip = vp * world.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.x.abs() > 1.5 || ndc.y.abs() > 1.5 {
            return None;
        }
        Some(Pos2::new(
            (ndc.x * 0.5 + 0.5) * view_rect.width() + view_rect.left(),
            (1.0 - (ndc.y * 0.5 + 0.5)) * view_rect.height() + view_rect.top(),
        ))
    };

    let Some(origin) = project(center) else {
        return;
    };

    for (label, end_world, color) in axes {
        let Some(end) = project(*end_world) else {
            continue;
        };
        painter.line_segment([origin, end], egui::Stroke::new(2.0, *color));

        // Arrowhead triangle
        let dir = (end - origin).normalized();
        let perp = Vec2::new(-dir.y, dir.x);
        painter.add(egui::Shape::convex_polygon(
            vec![
                end,
                end - dir * 8.0 + perp * 4.0,
                end - dir * 8.0 - perp * 4.0,
            ],
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

/// Draw a tooltip-style label box anchored to a hovered 3D target.
/// Shows code / name / faction for entities, name / type for static objects.
fn draw_hover_label(
    painter: &egui::Painter,
    view_rect: Rect,
    camera: &OrbitCamera,
    sector: &Sector,
    world: Option<&map_domain::world::World>,
    universe: &map_domain::universe::Universe,
    target: ClickedTarget,
) {
    let aspect = view_rect.width() / view_rect.height().max(1.0);
    let vp = camera.proj_matrix(aspect) * camera.view_matrix();

    let project = |w_pos: Vec3| -> Option<Pos2> {
        let clip = vp * w_pos.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(Pos2::new(
            (ndc.x * 0.5 + 0.5) * view_rect.width() + view_rect.left(),
            (1.0 - (ndc.y * 0.5 + 0.5)) * view_rect.height() + view_rect.top(),
        ))
    };

    // Lines to draw: (text, color)
    let mut lines: Vec<(String, egui::Color32)> = Vec::new();
    let anchor_pos: Option<Pos2> = match target {
        ClickedTarget::Static(obj_id) => {
            if let Some(obj) = sector.static_objects.iter().find(|o| o.id == obj_id) {
                lines.push((obj.name.clone(), crate::theme::TEXT_PRIMARY));
                lines.push((format!("Type: {:?}", obj.kind), crate::theme::TEXT_MUTED));
                project(obj.position)
            } else {
                None
            }
        }
        ClickedTarget::Entity(eid) => {
            if let Some(world) = world {
                let name = crate::colors::resolve_entity_label_without_code(world, universe, eid);
                if !name.is_empty() {
                    lines.push((name, crate::theme::TEXT_PRIMARY));
                }
                if let Some(code) = world.codes.get(&eid) {
                    lines.push((code.clone(), crate::theme::TEXT_MUTED));
                }
                if let Some(&fid) = world.factions.get(&eid) {
                    let f_name = crate::colors::faction_name(universe, fid);
                    let f_color = crate::colors::faction_color(universe, fid);
                    lines.push((f_name.to_string(), f_color));
                }
                world.positions.get(&eid).copied().and_then(project)
            } else {
                None
            }
        }
    };
    let Some(anchor) = anchor_pos else {
        return;
    };
    if lines.is_empty() {
        return;
    }

    let font = egui::FontId::proportional(11.0);
    let pad = 4.0;
    let line_h = 14.0;
    let max_w = lines
        .iter()
        .map(|(t, _)| {
            painter
                .layout_no_wrap(t.clone(), font.clone(), egui::Color32::WHITE)
                .rect
                .width()
        })
        .fold(0.0_f32, f32::max);
    let h = pad * 2.0 + line_h * lines.len() as f32;
    let label_rect = egui::Rect::from_min_size(
        anchor + egui::Vec2::new(10.0, -h - 4.0),
        egui::Vec2::new(max_w + pad * 2.0, h),
    );
    painter.rect_filled(
        label_rect,
        3.0,
        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200),
    );
    for (i, (text, color)) in lines.iter().enumerate() {
        let p = label_rect.min + egui::Vec2::new(pad, pad + i as f32 * line_h);
        painter.text(p, egui::Align2::LEFT_TOP, text, font.clone(), *color);
    }
}

/// Draw gates with correct orientation at constant pixel size on screen.
/// Arrow lies on the Y=0 horizontal plane, points toward sector center (origin), single-sided.
/// Ring is in 3D plane perpendicular to the arrow direction, polyline projected to screen.
/// World-space size derived from camera distance so on-screen size stays constant.
fn draw_gates_2d(painter: &egui::Painter, view_rect: Rect, camera: &OrbitCamera, sector: &Sector) {
    let aspect = view_rect.width() / view_rect.height().max(1.0);
    let vp = camera.proj_matrix(aspect) * camera.view_matrix();
    let cam_eye = camera.eye();
    // 2 * tan(fov_y / 2). FOV here matches OrbitCamera::proj_matrix (60° vertical).
    let fov_factor = 2.0 * 30.0_f32.to_radians().tan();

    let project = |world: Vec3| -> Option<egui::Pos2> {
        let clip = vp * world.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.x.abs() > 2.0 || ndc.y.abs() > 2.0 {
            return None;
        }
        Some(egui::Pos2::new(
            (ndc.x * 0.5 + 0.5) * view_rect.width() + view_rect.left(),
            (1.0 - (ndc.y * 0.5 + 0.5)) * view_rect.height() + view_rect.top(),
        ))
    };

    let ring_color = egui::Color32::from_rgba_premultiplied(100, 200, 255, 230);
    let arrow_color = egui::Color32::from_rgba_premultiplied(220, 220, 80, 230);
    const RING_PX: f32 = 9.0;
    const ARROW_PX: f32 = 16.0;
    const SEGMENTS: usize = 32;

    for obj in &sector.static_objects {
        if !matches!(obj.kind, StaticObjectKind::Gate | StaticObjectKind::Highway) {
            continue;
        }

        // Arrow direction: horizontal toward sector center (Y=0 plane).
        let mut dir = -obj.position;
        dir.y = 0.0;
        if dir.length() < 0.001 {
            continue;
        }
        let dir = dir.normalize();

        let is_superhighway = matches!(obj.kind, StaticObjectKind::Highway);
        // Entry vs exit: check details for "Direction" key set to "Outbound" or fall back to name.
        let is_entry = is_superhighway
            && obj
                .details
                .iter()
                .any(|(k, v)| k == "Direction" && v == "Outbound");
        // Pick colors per kind.
        let (this_ring_color, this_arrow_color) = if is_superhighway {
            (
                egui::Color32::from_rgba_premultiplied(80, 230, 120, 230),
                egui::Color32::from_rgba_premultiplied(80, 230, 120, 230),
            )
        } else {
            (ring_color, arrow_color)
        };

        // Distance from camera → world units per pixel for constant on-screen size.
        let dist = (obj.position - cam_eye).length();
        let world_per_px = dist * fov_factor / view_rect.height();
        let radius = RING_PX * world_per_px;
        let arrow_len = ARROW_PX * world_per_px;

        // Ring plane axes: perpendicular to `dir`. Up axis = Y; side axis = dir × Y.
        let axis_side = dir.cross(Vec3::Y).normalize();
        let axis_up = Vec3::Y;

        // 32-segment polyline approximating the ring in 3D, projected to screen.
        let mut pts = Vec::with_capacity(SEGMENTS);
        let mut all_ok = true;
        for i in 0..SEGMENTS {
            let t = (i as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
            let p = obj.position + radius * (t.cos() * axis_side + t.sin() * axis_up);
            match project(p) {
                Some(sp) => pts.push(sp),
                None => {
                    all_ok = false;
                    break;
                }
            }
        }
        if !all_ok {
            continue;
        }
        for i in 0..SEGMENTS {
            painter.line_segment(
                [pts[i], pts[(i + 1) % SEGMENTS]],
                egui::Stroke::new(1.5, this_ring_color),
            );
        }

        let center = Vec3::new(obj.position.x, 0.0, obj.position.z);
        let Some(s_center) = project(center) else {
            continue;
        };
        if is_superhighway {
            // Single arrow. Entry: outbound (center → outer). Exit: inbound (outer → center).
            // Outer point lies opposite to `dir` (which points toward origin), so center - dir * len.
            let outer = center - dir * arrow_len;
            let Some(s_outer) = project(outer) else {
                continue;
            };
            painter.line_segment(
                [s_center, s_outer],
                egui::Stroke::new(1.5, this_arrow_color),
            );
            if is_entry {
                draw_arrowhead(painter, s_outer, s_center, this_arrow_color);
            } else {
                draw_arrowhead(painter, s_center, s_outer, this_arrow_color);
            }
        } else {
            // Bidirectional arrows for regular gates: one toward origin, one away.
            let tip_inward = center + dir * arrow_len;
            let tip_outward = center - dir * arrow_len;
            let Some(s_inward) = project(tip_inward) else {
                continue;
            };
            let Some(s_outward) = project(tip_outward) else {
                continue;
            };
            painter.line_segment(
                [s_center, s_inward],
                egui::Stroke::new(1.5, this_arrow_color),
            );
            painter.line_segment(
                [s_center, s_outward],
                egui::Stroke::new(1.5, this_arrow_color),
            );
            draw_arrowhead(painter, s_inward, s_center, this_arrow_color);
            draw_arrowhead(painter, s_outward, s_center, this_arrow_color);
        }
    }
}

fn draw_arrowhead(
    painter: &egui::Painter,
    tip: egui::Pos2,
    tail: egui::Pos2,
    color: egui::Color32,
) {
    let dir = (tip - tail).normalized();
    let perp = egui::Vec2::new(-dir.y, dir.x);
    painter.add(egui::Shape::convex_polygon(
        vec![
            tip,
            tip - dir * 8.0 + perp * 4.0,
            tip - dir * 8.0 - perp * 4.0,
        ],
        color,
        egui::Stroke::NONE,
    ));
}
