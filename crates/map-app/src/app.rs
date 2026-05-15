use map_domain::universe::Universe;
use map_domain::view::ViewMode;
use crate::ui::top_bar::TopBar;

pub struct App {
    pub universe: Universe,
    pub view_mode: ViewMode,
    top_bar: TopBar,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, universe: Universe) -> Self {
        crate::theme::apply(&cc.egui_ctx);
        Self {
            universe,
            view_mode: ViewMode::initial(),
            top_bar: TopBar::default(),
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

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("2D map area — coming in next task");
        });
    }
}
