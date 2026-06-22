mod app;
pub mod colors;
mod renderer;
mod settings;
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
    /// Sent by `spawn_save_parse` as soon as the parse thread starts, so the UI
    /// can flip into a loading state even when the parse was triggered from a
    /// non-UI thread (file watcher).
    Loading,
    Loaded {
        meta: SnapshotMeta,
        world: World,
        faction_overrides: FactionOverrides,
    },
    None,
    Error(String),
}

fn main() -> eframe::Result<()> {
    // Profiler server (feature `profiling`). Streams scope data to a standalone
    // `puffin_viewer` over TCP. Kept alive for the whole process via the binding.
    #[cfg(feature = "profiling")]
    let _puffin_server = {
        puffin::set_scopes_on(true);
        let server = puffin_http::Server::new("127.0.0.1:8585")
            .expect("failed to start puffin server");
        eprintln!("[map] puffin server on 127.0.0.1:8585 — run: puffin_viewer --url 127.0.0.1:8585");
        server
    };

    // Attempt to load universe from game files
    let universe = load_universe(44);

    // Spawn the initial save-parse on a background thread so the UI starts
    // without blocking on ~300 MB of XML decompression.
    let (snapshot_tx, snapshot_rx) = mpsc::channel::<SnapshotMessage>();
    let sector_macros = universe.sector_macros.clone();
    let zone_positions = universe.zone_positions.clone();
    let faction_strings = universe.faction_strings.clone();
    let next_faction_id: u32 = (universe.faction_strings.len() as u32) + 1;
    let initial_tx = snapshot_tx.clone();
    let fs_init = faction_strings.clone();
    let nx_init = next_faction_id;
    let zone_positions_init = zone_positions.clone();
    std::thread::spawn(move || {
        let mut fs = fs_init;
        let mut nx = nx_init;
        let msg = parse_latest_save(&sector_macros, &zone_positions_init, &mut fs, &mut nx)
            .unwrap_or(SnapshotMessage::None);
        let _ = initial_tx.send(msg);
    });

    // Start a watcher on the save directory; whenever a .xml.gz settles,
    // kick off a fresh save parse via the existing snapshot channel.
    let watcher = if let Some((_, dir)) = find_latest_save() {
        let (wtx, wrx) = std::sync::mpsc::channel();
        match map_io::save_watcher::watch_save_dir(&dir, wtx) {
            Ok(w) => {
                eprintln!("[map] Watching save dir: {:?}", dir);
                let parse_tx = snapshot_tx.clone();
                let sector_macros = universe.sector_macros.clone();
                let zone_positions_w = zone_positions.clone();
                let faction_strings_w = faction_strings.clone();
                let next_faction_id_w = next_faction_id;
                std::thread::spawn(move || {
                    for event in wrx {
                        match event {
                            map_io::save_watcher::WatcherEvent::SaveChanged(path) => {
                                eprintln!("[map] Save changed: {:?}", path);
                                spawn_save_parse(
                                    parse_tx.clone(),
                                    sector_macros.clone(),
                                    zone_positions_w.clone(),
                                    faction_strings_w.clone(),
                                    next_faction_id_w,
                                );
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
        Box::new(move |cc| {
            Ok(Box::new(app::App::new(
                cc,
                universe,
                snapshot_tx,
                snapshot_rx,
                watcher,
            )))
        }),
    )
}

fn load_universe(locale: u32) -> map_domain::universe::Universe {
    let game_path = map_io::game_path::detect();

    let Some(game_dir) = game_path else {
        eprintln!("[map] Game path not found — starting with empty universe.");
        return map_domain::universe::Universe::default();
    };

    eprintln!("[map] Found game at: {:?}", game_dir);

    match map_io::xml_parser::parse_galaxy_from_game(&game_dir, locale) {
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

/// Enumerate all save directories X4 might use on this system. Includes:
///   - native Linux:  `~/.config/EgoSoft/X4/<id>/save/`
///   - Steam Proton:  `~/.local/share/Steam/steamapps/compatdata/392160/pfx/drive_c/users/steamuser/Documents/Egosoft/X4/<id>/save/`
///   - native Windows: `<Documents>/Egosoft/X4/<id>/save/`
fn candidate_save_dirs() -> Vec<std::path::PathBuf> {
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    if let Some(c) = dirs::config_dir() {
        roots.push(c.join("EgoSoft").join("X4"));
    }
    if let Some(d) = dirs::document_dir() {
        roots.push(d.join("Egosoft").join("X4"));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local/share/Steam/steamapps/compatdata/392160/pfx/drive_c/users/steamuser/Documents/Egosoft/X4"));
    }

    let mut out: Vec<std::path::PathBuf> = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()) {
            let save = e.path().join("save");
            if save.is_dir() {
                out.push(save);
            }
        }
    }
    out
}

/// Pick the single newest `.xml.gz` across every candidate save dir. Returns
/// `(save_file_path, containing_dir)` so the watcher can watch the active dir.
fn find_latest_save() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let mut best: Option<(
        std::time::SystemTime,
        std::path::PathBuf,
        std::path::PathBuf,
    )> = None;
    for dir in candidate_save_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with(".xml.gz") {
                continue;
            }
            let mtime = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().map_or(true, |(t, _, _)| mtime > *t) {
                best = Some((mtime, p.clone(), dir.clone()));
            }
        }
    }
    best.map(|(_, p, d)| (p, d))
}

/// Locate the latest save, parse it (resolving entities via `sector_macros`), and
/// produce a `SnapshotMessage`. Returns `None` if no save dir/file is present —
/// callers map that to `SnapshotMessage::None`. Parser errors are mapped to
/// `SnapshotMessage::Error`.
pub fn parse_latest_save(
    sector_macros: &HashMap<String, SectorId>,
    zone_positions: &HashMap<String, (f32, f32, f32)>,
    faction_strings: &mut HashMap<String, FactionId>,
    next_faction_id: &mut u32,
) -> Option<SnapshotMessage> {
    let (path, _dir) = find_latest_save()?;
    eprintln!("[map] Loading save: {:?}", path);
    match map_io::save_parser::parse_save(
        &path,
        Some(sector_macros),
        zone_positions,
        faction_strings,
        next_faction_id,
    ) {
        Ok((meta, world, faction_overrides)) => {
            eprintln!(
                "[map] Snapshot: time={:.1}s money={} location={}",
                meta.game_time_seconds, meta.player_money, meta.player_location_name
            );
            eprintln!(
                "[map] Faction overrides: {} sectors",
                faction_overrides.len()
            );
            Some(SnapshotMessage::Loaded {
                meta,
                world,
                faction_overrides,
            })
        }
        Err(e) => {
            eprintln!("[map] save_parser error: {:?}", e);
            Some(SnapshotMessage::Error(format!("{:?}", e)))
        }
    }
}

/// Apply per-sector faction overrides (sector_macro → faction-name) to the
/// universe in-place. Reuses existing FactionIds from `universe.faction_strings`
/// to avoid ID collisions with the static load; allocates new IDs only for
/// factions not seen during static load. Logs the number of sectors updated.
pub fn apply_faction_overrides(universe: &mut Universe, overrides: &FactionOverrides) {
    let mut next_faction_id: u32 = (universe.faction_strings.len() as u32) + 1;
    let mut applied: usize = 0;

    for (macro_key, faction_name) in overrides.iter() {
        let Some(&sector_id) = universe.sector_macros.get(macro_key) else {
            continue;
        };
        let key = faction_name.to_lowercase();
        let fid = *universe.faction_strings.entry(key).or_insert_with(|| {
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

/// Note: `faction_strings` is moved into the spawned thread; any new faction
/// IDs minted during parse stay inside that thread's clone and are never
/// propagated back to the watcher's snapshot. In practice X4 saves always
/// reference the canonical factions already loaded from `libraries/factions.xml`,
/// so this is a non-issue today. If mods introduce save-only factions, those
/// would get re-minted on every reparse (cosmetic: same name, different ID).
///
/// Spawn a fresh background save-parse, sending its result down `tx`. Called
/// from the UI when the user clicks Refresh.
pub fn spawn_save_parse(
    tx: mpsc::Sender<SnapshotMessage>,
    sector_macros: HashMap<String, SectorId>,
    zone_positions: HashMap<String, (f32, f32, f32)>,
    mut faction_strings: HashMap<String, FactionId>,
    mut next_faction_id: u32,
) {
    // Notify UI before parsing so it can show the loading state immediately.
    let _ = tx.send(SnapshotMessage::Loading);
    std::thread::spawn(move || {
        let msg = parse_latest_save(
            &sector_macros,
            &zone_positions,
            &mut faction_strings,
            &mut next_faction_id,
        )
        .unwrap_or(SnapshotMessage::None);
        let _ = tx.send(msg);
    });
}
