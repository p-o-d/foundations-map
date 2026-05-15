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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_bar")
            .exact_height(36.0)
            .show(ctx, |ui| {
                self.top_bar.show(ui);
            });

        egui::SidePanel::right("sector_panel")
            .exact_width(220.0)
            .resizable(false)
            .show(ctx, |ui| {
                let selected = self.view_mode.selected_sector();
                let sector = selected.and_then(|id| self.universe.sector(id));
                let panel_resp = self.sector_panel.show(ui, sector, &self.universe);
                if panel_resp.open_3d_clicked {
                    self.view_mode = self.view_mode.clone().open_sector_3d();
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
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
