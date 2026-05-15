mod renderer;
mod app;
mod theme;
pub mod ui;

fn main() -> eframe::Result<()> {
    // Attempt to load universe from game files
    let universe = load_universe();

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

fn load_universe() -> map_domain::universe::Universe {
    let game_path = map_io::game_path::detect();

    let Some(game_dir) = game_path else {
        eprintln!("[map] Game path not found — starting with empty universe.");
        return map_domain::universe::Universe::default();
    };

    eprintln!("[map] Found game at: {:?}", game_dir);

    match map_io::xml_parser::parse_galaxy_from_game(&game_dir) {
        Ok(universe) => {
            eprintln!("[map] Loaded {} sectors.", universe.sectors.len());
            universe
        }
        Err(e) => {
            eprintln!("[map] Failed to parse galaxy.xml: {:?}", e);
            map_domain::universe::Universe::default()
        }
    }
}
