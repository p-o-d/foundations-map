mod app;
mod theme;
pub mod ui;

fn main() -> eframe::Result<()> {
    let universe = map_domain::universe::Universe::default();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Foundations Map")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Foundations Map",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, universe)))),
    )
}
