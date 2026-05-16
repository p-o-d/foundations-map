mod renderer;
mod app;
mod theme;
pub mod ui;

use std::collections::HashMap;
use std::sync::mpsc;

use map_domain::ids::{FactionId, SectorId};
use map_domain::universe::Universe;
use map_domain::world::{SnapshotMeta, World};
use map_io::save_parser::FactionOverrides;

/// Message produced by the background save-parser thread.
pub enum SnapshotMessage {
    Loaded {
        meta: SnapshotMeta,
        world: World,
        faction_overrides: FactionOverrides,
    },
    None,
    Error(String),
}

fn main() -> eframe::Result<()> {
    // Attempt to load universe from game files
    let universe = load_universe();

    // Spawn the initial save-parse on a background thread so the UI starts
    // without blocking on ~300 MB of XML decompression.
    let (snapshot_tx, snapshot_rx) = mpsc::channel::<SnapshotMessage>();
    let sector_macros = universe.sector_macros.clone();
    let initial_tx = snapshot_tx.clone();
    std::thread::spawn(move || {
        let msg = parse_latest_save(&sector_macros).unwrap_or(SnapshotMessage::None);
        let _ = initial_tx.send(msg);
    });

    // Start a watcher on the save directory; whenever a .xml.gz settles,
    // kick off a fresh save parse via the existing snapshot channel.
    let watcher = if let Some(dir) = save_dir() {
        let (wtx, wrx) = std::sync::mpsc::channel();
        match map_io::save_watcher::watch_save_dir(&dir, wtx) {
            Ok(w) => {
                eprintln!("[map] Watching save dir: {:?}", dir);
                let parse_tx = snapshot_tx.clone();
                let sector_macros = universe.sector_macros.clone();
                std::thread::spawn(move || {
                    for event in wrx {
                        match event {
                            map_io::save_watcher::WatcherEvent::SaveChanged(path) => {
                                eprintln!("[map] Save changed: {:?}", path);
                                spawn_save_parse(parse_tx.clone(), sector_macros.clone());
                            }
                        }
                    }
                });
                Some(w)
            }
            Err(e) => {
                eprintln!("[map] save watcher failed: {:?}", e);
                None
            }
        }
    } else {
        None
    };

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
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, universe, snapshot_tx, snapshot_rx, watcher)))),
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

/// Locate the latest save, parse it (resolving entities via `sector_macros`), and
/// produce a `SnapshotMessage`. Returns `None` if no save dir/file is present —
/// callers map that to `SnapshotMessage::None`. Parser errors are mapped to
/// `SnapshotMessage::Error`.
pub fn parse_latest_save(
    sector_macros: &HashMap<String, SectorId>,
) -> Option<SnapshotMessage> {
    let dir = save_dir()?;
    let path = latest_save(&dir)?;
    eprintln!("[map] Loading save: {:?}", path);
    match map_io::save_parser::parse_save(&path, Some(sector_macros)) {
        Ok((meta, world, faction_overrides)) => {
            eprintln!(
                "[map] Snapshot: time={:.1}s money={} location={}",
                meta.game_time_seconds, meta.player_money, meta.player_location_name
            );
            eprintln!("[map] Faction overrides: {} sectors", faction_overrides.len());
            Some(SnapshotMessage::Loaded { meta, world, faction_overrides })
        }
        Err(e) => {
            eprintln!("[map] save_parser error: {:?}", e);
            Some(SnapshotMessage::Error(format!("{:?}", e)))
        }
    }
}

/// Apply per-sector faction overrides (sector_macro → faction-name) to the
/// universe in-place. Assigns FactionIds first-seen with a counter starting
/// at 1. Logs the number of sectors updated.
pub fn apply_faction_overrides(universe: &mut Universe, overrides: &FactionOverrides) {
    let mut faction_name_to_id: HashMap<String, FactionId> = HashMap::new();
    let mut next_faction_id: u32 = 1;
    let mut applied: usize = 0;

    for (macro_key, faction_name) in overrides.iter() {
        let Some(&sector_id) = universe.sector_macros.get(macro_key) else { continue; };
        let fid = *faction_name_to_id.entry(faction_name.clone()).or_insert_with(|| {
            let id = FactionId(next_faction_id);
            next_faction_id += 1;
            id
        });
        if let Some(sec) = universe.sectors.iter_mut().find(|s| s.id == sector_id) {
            sec.faction = Some(fid);
            applied += 1;
        }
    }
    eprintln!("[map] Applied {} sector faction overrides", applied);
}

/// Spawn a fresh background save-parse, sending its result down `tx`. Called
/// from the UI when the user clicks Refresh.
pub fn spawn_save_parse(
    tx: mpsc::Sender<SnapshotMessage>,
    sector_macros: HashMap<String, SectorId>,
) {
    std::thread::spawn(move || {
        let msg = parse_latest_save(&sector_macros).unwrap_or(SnapshotMessage::None);
        let _ = tx.send(msg);
    });
}
