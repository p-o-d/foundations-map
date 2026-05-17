use std::sync::mpsc;

use crate::SnapshotMessage;
use crate::renderer::camera::OrbitCamera;
use crate::ui::{
    map_view::MapView, sector_panel::SectorPanel, sector_view::SectorView3D, top_bar::TopBar,
};
use map_domain::universe::Universe;
use map_domain::view::ViewMode;
use map_domain::world::{SnapshotMeta, World};

pub struct App {
    pub universe: Universe,
    pub view_mode: ViewMode,
    pub camera: OrbitCamera,
    pub snapshot: Option<(SnapshotMeta, World)>,
    pub snapshot_loading: bool,
    pub snapshot_tx: mpsc::Sender<SnapshotMessage>,
    pub snapshot_rx: mpsc::Receiver<SnapshotMessage>,
    top_bar: TopBar,
    map_view: MapView,
    sector_panel: SectorPanel,
    sector_view: SectorView3D,
    /// Keeps the save-dir watcher alive; dropping it would stop the watcher.
    _save_watcher: Option<map_io::save_watcher::RecommendedWatcher>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        universe: Universe,
        snapshot_tx: mpsc::Sender<SnapshotMessage>,
        snapshot_rx: mpsc::Receiver<SnapshotMessage>,
        save_watcher: Option<map_io::save_watcher::RecommendedWatcher>,
    ) -> Self {
        crate::theme::apply(&cc.egui_ctx);

        if let Some(rs) = &cc.wgpu_render_state {
            let scene = crate::renderer::gpu::GpuScene::new(&rs.device, rs.target_format);
            rs.renderer.write().callback_resources.insert(scene);
        }

        Self {
            universe,
            view_mode: ViewMode::initial(),
            camera: OrbitCamera::default(),
            snapshot: None,
            // Start in loading state — the initial save parse fires from main()
            // before App::new returns.
            snapshot_loading: true,
            snapshot_tx,
            snapshot_rx,
            top_bar: TopBar::default(),
            map_view: MapView::default(),
            sector_panel: SectorPanel::default(),
            sector_view: SectorView3D::default(),
            _save_watcher: save_watcher,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Drain any pending save-parse results.
        while let Ok(msg) = self.snapshot_rx.try_recv() {
            match msg {
                SnapshotMessage::Loading => {
                    self.snapshot_loading = true;
                    ui.ctx().request_repaint();
                }
                SnapshotMessage::Loaded {
                    meta,
                    world,
                    faction_overrides,
                } => {
                    crate::apply_faction_overrides(&mut self.universe, &faction_overrides);
                    self.snapshot = Some((meta, world));
                    self.snapshot_loading = false;
                    ui.ctx().request_repaint();
                }
                SnapshotMessage::None => {
                    eprintln!("[map] No save file found.");
                    self.snapshot_loading = false;
                }
                SnapshotMessage::Error(e) => {
                    eprintln!("[map] Save parse error: {}", e);
                    self.snapshot_loading = false;
                }
            }
        }

        // Escape: deselect object (not close)
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            if let ViewMode::SectorView {
                sector,
                selected_obj: Some(_),
            } = &self.view_mode
            {
                let sector = *sector;
                let positions: Vec<_> = self
                    .universe
                    .sector(sector)
                    .map(|s| s.static_objects.iter().map(|o| o.position).collect())
                    .unwrap_or_default();
                self.camera.fit_all(&positions);
                self.view_mode = self.view_mode.clone().deselect_object();
            }
        }

        let mut refresh_clicked = false;
        egui::Panel::top("top_bar")
            .exact_size(36.0)
            .show_inside(ui, |ui| {
                let meta = self.snapshot.as_ref().map(|(m, _)| m);
                let resp = self.top_bar.show(ui, meta, self.snapshot_loading);
                refresh_clicked = resp.refresh_clicked;
            });

        // Keep the snapshot-age label fresh without burning CPU: tick at most
        // every 30s. Real state changes (watcher, refresh button) still cause
        // immediate repaints via their own request_repaint calls.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(30));

        if refresh_clicked && !self.snapshot_loading {
            self.snapshot_loading = true;
            crate::spawn_save_parse(
                self.snapshot_tx.clone(),
                self.universe.sector_macros.clone(),
                self.universe.faction_strings.clone(),
                (self.universe.faction_strings.len() as u32) + 1,
            );
        }

        // While loading, repaint quickly so the progress spinner animates.
        if self.snapshot_loading {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        }

        egui::Panel::right("sector_panel")
            .exact_size(220.0)
            .resizable(false)
            .show_inside(ui, |ui| {
                let selected = self.view_mode.selected_sector();
                let sector = selected.and_then(|id| self.universe.sector(id));
                let panel_resp =
                    self.sector_panel
                        .show(ui, sector, &self.universe, &self.view_mode);
                if panel_resp.open_3d_clicked {
                    if let Some(s) = sector {
                        let positions: Vec<_> =
                            s.static_objects.iter().map(|o| o.position).collect();
                        self.camera.fit_all(&positions);
                    }
                    self.view_mode = self.view_mode.clone().open_sector_3d();
                }
                if panel_resp.back_to_map_clicked {
                    self.view_mode = self.view_mode.clone().close_sector_3d();
                }
                if let Some(obj_id) = panel_resp.object_clicked {
                    self.view_mode = self.view_mode.clone().select_object(obj_id);
                    if let ViewMode::SectorView { sector, .. } = &self.view_mode {
                        if let Some(s) = self.universe.sector(*sector) {
                            if let Some(obj) = s.static_objects.iter().find(|o| o.id == obj_id) {
                                self.camera.fit_all(&[obj.position]);
                            }
                        }
                    }
                }
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let selected = self.view_mode.selected_sector();

            match self.view_mode.clone() {
                ViewMode::SectorView {
                    sector,
                    selected_obj,
                } => {
                    // Compute positions before calling show to avoid double-borrow of self.universe
                    let sec = self.universe.sector(sector);
                    let sv_resp = self.sector_view.show(
                        ui,
                        sec,
                        &mut self.camera,
                        selected_obj,
                        self.snapshot.as_ref().map(|(_, w)| w),
                    );
                    if sv_resp.close_clicked {
                        self.view_mode = self.view_mode.clone().close_sector_3d();
                    }
                    if let Some(obj_id) = sv_resp.clicked_object {
                        self.view_mode = self.view_mode.clone().select_object(obj_id);
                        let positions: Vec<_> = self
                            .universe
                            .sector(sector)
                            .and_then(|s| s.static_objects.iter().find(|o| o.id == obj_id))
                            .map(|obj| vec![obj.position])
                            .unwrap_or_default();
                        self.camera.fit_all(&positions);
                    }
                }
                ViewMode::UniverseMap { .. } => {
                    let mvr = self.map_view.show(
                        ui,
                        &self.universe,
                        self.snapshot.as_ref().map(|(_, w)| w),
                        selected,
                    );
                    if let Some(sector_id) = mvr.double_clicked_sector {
                        let positions: Vec<_> = self
                            .universe
                            .sector(sector_id)
                            .map(|s| s.static_objects.iter().map(|o| o.position).collect())
                            .unwrap_or_default();
                        self.camera.fit_all(&positions);
                        self.view_mode = self
                            .view_mode
                            .clone()
                            .select_sector(sector_id)
                            .open_sector_3d();
                    } else if let Some(sector_id) = mvr.clicked_sector {
                        self.view_mode = self.view_mode.clone().select_sector(sector_id);
                    }
                }
            }
        });
    }
}
