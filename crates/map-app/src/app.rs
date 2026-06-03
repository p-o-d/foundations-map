use std::sync::mpsc;

use crate::SnapshotMessage;
use crate::renderer::camera::OrbitCamera;
use crate::ui::{
    map_view::MapView, sector_panel::SectorPanel, sector_view::SectorView3D, top_bar::TopBar,
};
use map_domain::filter::MapFilterMode;
use map_domain::universe::Universe;
use map_domain::view::ViewMode;
use map_domain::world::{SnapshotMeta, World};

pub struct App {
    pub universe: Universe,
    pub view_mode: ViewMode,
    pub filter_mode: MapFilterMode,
    pub camera: OrbitCamera,
    pub snapshot: Option<(SnapshotMeta, World)>,
    pub snapshot_loading: bool,
    pub snapshot_tx: mpsc::Sender<SnapshotMessage>,
    pub snapshot_rx: mpsc::Receiver<SnapshotMessage>,
    pub settings: crate::settings::AppSettings,
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

        let settings = crate::settings::load(cc.storage);
        eprintln!("[map] Loaded settings: locale={}", settings.locale);

        let mut app = Self {
            universe,
            view_mode: ViewMode::initial(),
            filter_mode: MapFilterMode::Normal,
            camera: OrbitCamera::default(),
            snapshot: None,
            // Start in loading state — the initial save parse fires from main()
            // before App::new returns.
            snapshot_loading: true,
            snapshot_tx,
            snapshot_rx,
            settings,
            top_bar: TopBar::default(),
            map_view: MapView::default(),
            sector_panel: SectorPanel::default(),
            sector_view: SectorView3D::default(),
            _save_watcher: save_watcher,
        };

        // If the user previously picked a non-default locale, trigger a
        // deferred reload now so the first interactive frame uses the right
        // language. First-frame English flash is a minor UX wart.
        if app.settings.locale != 44 {
            if let Some(game_dir) = map_io::game_path::detect() {
                app.reload_galaxy(app.settings.locale, &game_dir);
            }
        }

        app
    }
}

impl App {
    pub fn reload_galaxy(&mut self, locale: u32, game_dir: &std::path::Path) {
        eprintln!("[map] Reloading universe with locale {}", locale);
        match map_io::xml_parser::parse_galaxy_from_game(game_dir, locale) {
            Ok(universe) => {
                self.universe = universe;
                self.settings.locale = locale;
                self.snapshot = None;
                self.snapshot_loading = true;
                crate::spawn_save_parse(
                    self.snapshot_tx.clone(),
                    self.universe.sector_macros.clone(),
                    self.universe.zone_positions.clone(),
                    self.universe.faction_strings.clone(),
                    (self.universe.faction_strings.len() as u32) + 1,
                );
            }
            Err(e) => {
                eprintln!("[map] Locale switch failed (parse error): {:?}", e);
            }
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        crate::settings::save(storage, &self.settings);
    }

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

        // Escape cascade:
        //  3D + selected obj/entity → clear selection + refit camera.
        //  3D + nothing selected    → close 3D view back to the 2D map.
        //  2D + sector selected     → clear sector selection.
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            match &self.view_mode {
                ViewMode::SectorView {
                    sector,
                    selected_obj,
                    selected_entity,
                } if selected_obj.is_some() || selected_entity.is_some() => {
                    let sector = *sector;
                    let positions: Vec<_> = self
                        .universe
                        .sector(sector)
                        .map(|s| s.static_objects.iter().map(|o| o.position).collect())
                        .unwrap_or_default();
                    self.camera.fit_all(&positions);
                    self.view_mode = self.view_mode.clone().deselect_all_in_sector();
                }
                ViewMode::SectorView { .. } => {
                    self.view_mode = self.view_mode.clone().close_sector_3d();
                }
                ViewMode::UniverseMap { selected: Some(_) } => {
                    self.view_mode = self.view_mode.clone().deselect_sector();
                }
                _ => {}
            }
        }

        let mut refresh_clicked = false;
        let mut locale_change: Option<u32> = None;
        egui::Panel::top("top_bar")
            .exact_size(36.0)
            .show_inside(ui, |ui| {
                let meta = self.snapshot.as_ref().map(|(m, _)| m);
                let resp = self.top_bar.show(
                    ui,
                    meta,
                    self.snapshot_loading,
                    &self.universe.available_locales,
                    self.settings.locale,
                    self.filter_mode,
                );
                refresh_clicked = resp.refresh_clicked;
                locale_change = resp.locale_changed_to;
                if let Some(mode) = resp.filter_changed_to {
                    self.filter_mode = mode;
                }
            });
        if let Some(new_locale) = locale_change {
            if let Some(game_dir) = map_io::game_path::detect() {
                self.reload_galaxy(new_locale, &game_dir);
            }
        }

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
                self.universe.zone_positions.clone(),
                self.universe.faction_strings.clone(),
                (self.universe.faction_strings.len() as u32) + 1,
            );
        }

        // While loading, repaint quickly so the progress spinner animates.
        if self.snapshot_loading {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        }

        // Hide the side panel entirely when nothing is selected (universe map with
        // no sector clicked). The 3D view always has a sector, so the panel stays.
        if self.view_mode.selected_sector().is_some() {
            egui::Panel::right("sector_panel")
                .default_size(330.0)
                .size_range(220.0..=600.0)
                .resizable(true)
                .frame(
                    egui::Frame::default()
                        .fill(ui.style().visuals.panel_fill)
                        .inner_margin(egui::Margin {
                            left: 4,
                            right: 12,
                            top: 0,
                            bottom: 0,
                        }),
                )
                .show_inside(ui, |ui| {
                    let selected = self.view_mode.selected_sector();
                    let sector = selected.and_then(|id| self.universe.sector(id));
                    let panel_resp = self.sector_panel.show(
                        ui,
                        sector,
                        &self.universe,
                        &self.view_mode,
                        self.snapshot.as_ref().map(|(_, w)| w),
                    );
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
                                if let Some(obj) = s.static_objects.iter().find(|o| o.id == obj_id)
                                {
                                    self.camera.focus_on(obj.position);
                                }
                            }
                        }
                    }
                    if let Some(eid) = panel_resp.entity_clicked {
                        self.view_mode = self.view_mode.clone().select_entity(eid);
                        if let Some((_, world)) = &self.snapshot {
                            if let Some(&pos) = world.positions.get(&eid) {
                                self.camera.focus_on(pos);
                            }
                        }
                    }
                    if panel_resp.back_to_parent_clicked {
                        if let (Some(eid), Some((_, world))) =
                            (self.view_mode.selected_entity(), self.snapshot.as_ref())
                        {
                            if let Some(parent) = world.parent_of(eid) {
                                self.view_mode = self.view_mode.clone().select_entity(parent);
                                if let Some(&pos) = world.positions.get(&parent) {
                                    self.camera.focus_on(pos);
                                }
                            }
                        }
                    }
                });
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let selected = self.view_mode.selected_sector();

            match self.view_mode.clone() {
                ViewMode::SectorView { sector, .. } => {
                    // Compute positions before calling show to avoid double-borrow of self.universe
                    let sec = self.universe.sector(sector);
                    let sv_resp = self.sector_view.show(
                        ui,
                        sec,
                        &mut self.camera,
                        self.view_mode.selected_object(),
                        self.view_mode.selected_entity(),
                        self.snapshot.as_ref().map(|(_, w)| w),
                        &self.universe,
                    );
                    if sv_resp.close_clicked {
                        self.view_mode = self.view_mode.clone().close_sector_3d();
                    }
                    match sv_resp.clicked {
                        Some(crate::ui::sector_view::ClickedTarget::Static(obj_id)) => {
                            self.view_mode = self.view_mode.clone().select_object(obj_id);
                            if let Some(pos) = self
                                .universe
                                .sector(sector)
                                .and_then(|s| s.static_objects.iter().find(|o| o.id == obj_id))
                                .map(|obj| obj.position)
                            {
                                self.camera.focus_on(pos);
                            }
                        }
                        Some(crate::ui::sector_view::ClickedTarget::Entity(eid)) => {
                            self.view_mode = self.view_mode.clone().select_entity(eid);
                            if let Some((_, world)) = &self.snapshot {
                                if let Some(&pos) = world.positions.get(&eid) {
                                    self.camera.focus_on(pos);
                                }
                            }
                        }
                        None => {}
                    }
                    if sv_resp.clicked_empty {
                        let positions: Vec<_> = self
                            .universe
                            .sector(sector)
                            .map(|s| s.static_objects.iter().map(|o| o.position).collect())
                            .unwrap_or_default();
                        self.camera.fit_all(&positions);
                        self.view_mode = self.view_mode.clone().deselect_all_in_sector();
                    }
                }
                ViewMode::UniverseMap { .. } => {
                    // Compute the active map filter (matched sectors → hits).
                    // `None` for Normal mode means "no greying, no tooltips".
                    let world = self.snapshot.as_ref().map(|(_, w)| w);
                    let filter = (self.filter_mode != MapFilterMode::Normal).then(|| {
                        map_domain::filter::matched_sectors(self.filter_mode, &self.universe, world)
                    });
                    let mvr =
                        self.map_view
                            .show(ui, &self.universe, world, selected, filter.as_ref());
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
                    } else if mvr.clicked_empty {
                        self.view_mode = self.view_mode.clone().deselect_sector();
                    }
                }
            }
        });
    }
}
