use map_domain::universe::Universe;
use map_domain::view::ViewMode;

pub struct App {
    pub universe: Universe,
    pub view_mode: ViewMode,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, universe: Universe) -> Self {
        crate::theme::apply(&cc.egui_ctx);
        Self {
            universe,
            view_mode: ViewMode::initial(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("foundations-map — Phase 1 in progress");
        });
    }
}
