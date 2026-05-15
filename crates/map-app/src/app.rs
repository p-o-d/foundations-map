use map_domain::universe::Universe;
use map_domain::view::ViewMode;
use crate::ui::{top_bar::TopBar, map_view::MapView, sector_panel::SectorPanel};

pub struct App {
    pub universe: Universe,
    pub view_mode: ViewMode,
    top_bar: TopBar,
    map_view: MapView,
    sector_panel: SectorPanel,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, universe: Universe) -> Self {
        crate::theme::apply(&cc.egui_ctx);
        Self {
            universe,
            view_mode: ViewMode::initial(),
            top_bar: TopBar::default(),
            map_view: MapView::default(),
            sector_panel: SectorPanel::default(),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top_bar")
            .exact_size(36.0)
            .show_inside(ui, |ui| {
                self.top_bar.show(ui);
            });

        egui::Panel::right("sector_panel")
            .exact_size(220.0)
            .resizable(false)
            .show_inside(ui, |ui| {
                let selected = self.view_mode.selected_sector();
                let sector = selected.and_then(|id| self.universe.sector(id));
                let panel_resp = self.sector_panel.show(ui, sector, &self.universe);
                if panel_resp.open_3d_clicked {
                    self.view_mode = self.view_mode.clone().open_sector_3d();
                }
                if panel_resp.back_to_map_clicked {
                    self.view_mode = self.view_mode.clone().close_sector_3d();
                }
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let selected = self.view_mode.selected_sector();
            let mvr = self.map_view.show(ui, &self.universe, selected);

            if let Some(sector_id) = mvr.double_clicked_sector {
                self.view_mode = self.view_mode.clone().select_sector(sector_id).open_sector_3d();
            } else if let Some(sector_id) = mvr.clicked_sector {
                self.view_mode = self.view_mode.clone().select_sector(sector_id);
            }
        });
    }
}
