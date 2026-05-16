mod renderer;
mod app;
mod theme;
pub mod ui;

fn main() -> eframe::Result<()> {
    // Attempt to load universe from game files
    let mut universe = load_universe();
    let snapshot = load_snapshot(&mut universe);

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
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, universe, snapshot)))),
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

fn save_dir() -> Option<std::path::PathBuf> {
    // Linux: ~/.config/EgoSoft/X4/<id>/save  ; we use dirs::config_dir() for /Documents on Windows-equivalent.
    let base = dirs::config_dir()?.join("EgoSoft").join("X4");
    let mut entries = std::fs::read_dir(&base).ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect::<Vec<_>>();
    entries.sort();
    entries.into_iter().next().map(|p| p.join("save"))
}

fn latest_save(save_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut latest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for e in std::fs::read_dir(save_dir).ok()?.filter_map(|e| e.ok()) {
        let p = e.path();
        let name = p.file_name()?.to_str()?.to_string();
        if !name.ends_with(".xml.gz") { continue; }
        let mtime = e.metadata().ok().and_then(|m| m.modified().ok()).unwrap_or(std::time::UNIX_EPOCH);
        match &latest {
            Some((t, _)) if *t >= mtime => {}
            _ => latest = Some((mtime, p)),
        }
    }
    latest.map(|(_, p)| p)
}

fn load_snapshot(
    universe: &mut map_domain::universe::Universe,
) -> Option<(map_domain::world::SnapshotMeta, map_domain::world::World)> {
    let dir = save_dir()?;
    let path = latest_save(&dir)?;
    eprintln!("[map] Loading save: {:?}", path);

    // Build sector_macro→SectorId map.
    // Sector.name isn't the macro; we need the macro string. Re-derive from cluster + sector index?
    // For now we approximate by deriving from sector macro. BUT: domain Sector doesn't expose the
    // macro. To keep this clean for Phase 3 Task 5, pass `None` to parse_save (no entities) and
    // still apply faction overrides (which are keyed by macro and don't need SectorId resolution).
    //
    // A later cleanup can plumb sector_macro through map-domain.
    let (meta, world, overrides) = match map_io::save_parser::parse_save(&path, None) {
        Ok(t) => t,
        Err(e) => { eprintln!("[map] save_parser error: {:?}", e); return None; }
    };
    eprintln!("[map] Snapshot: time={:.1}s money={} location={}", meta.game_time_seconds, meta.player_money, meta.player_location_name);
    eprintln!("[map] Faction overrides: {} sectors", overrides.len());

    // Apply faction overrides — but we don't have the macro for each Sector currently.
    // For Phase 3 Task 5 we just log the overrides and return the snapshot; faction
    // application is deferred until we wire macro through to Sector (Task 4 cleanup).
    let _ = (universe, overrides);

    Some((meta, world))
}
