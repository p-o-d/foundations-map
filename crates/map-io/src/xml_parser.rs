use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use glam::{Vec2, Vec3};
use quick_xml::Reader;
use quick_xml::events::Event;
use rayon::prelude::*;

use map_domain::ids::{ClusterId, FactionId, ObjectId, SectorId};
use map_domain::objects::{StaticObject, StaticObjectKind};
use map_domain::universe::{Cluster, Connection, GateType, Sector, Universe};

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Xml(quick_xml::Error),
    MissingAttribute(String),
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        ParseError::Io(e)
    }
}

impl From<quick_xml::Error> for ParseError {
    fn from(e: quick_xml::Error) -> Self {
        ParseError::Xml(e)
    }
}

fn attr_value(e: &quick_xml::events::BytesStart, name: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == name {
            return Some(
                std::str::from_utf8(attr.value.as_ref())
                    .unwrap_or("")
                    .to_string(),
            );
        }
    }
    None
}

/// Parse galaxy from real X4 game directory using cat/dat archives.
///
/// Reads four files:
/// - `maps/xu_ep2_universe/galaxy.xml`   — cluster macro refs + absolute positions
/// - `maps/xu_ep2_universe/clusters.xml` — sector macro refs + relative offsets per cluster
/// - `libraries/mapdefaults.xml`          — sector macro → `{pageId,textId}` name reference
/// - `t/0001-l{locale}.xml`              — translations for detected Steam language
pub fn parse_galaxy_from_game(game_dir: &Path, locale: u32) -> Result<Universe, ParseError> {
    let load = |path: &str| {
        crate::cat_reader::read_game_file(game_dir, path).ok_or_else(|| {
            ParseError::MissingAttribute(format!("{path} not found in cat archives"))
        })
    };

    let translations_path = format!("t/0001-l{:03}.xml", locale);
    let translations_data = load(&translations_path)?;
    let translations_str = String::from_utf8_lossy(&translations_data);

    // Scan all universe map XML files (main + every DLC) once.
    let universe_files =
        crate::cat_reader::list_files_matching(game_dir, "maps/xu_ep2_universe/", ".xml");

    let mut cluster_positions: HashMap<String, (f32, f32)> = HashMap::new();
    let mut sector_placements: HashMap<String, (String, f32, f32)> = HashMap::new();
    let mut all_clusters_strs: Vec<String> = Vec::new();
    let mut all_zones_strs: Vec<String> = Vec::new();
    let mut all_sectors_strs: Vec<String> = Vec::new();

    for (path, data) in &universe_files {
        let s = String::from_utf8_lossy(data).into_owned();
        if path.ends_with("galaxy.xml") {
            if let Ok(positions) = parse_cluster_positions_xml(&s) {
                cluster_positions.extend(positions);
            }
        } else if path.ends_with("clusters.xml") {
            if let Ok(placements) = parse_sector_placements_xml(&s) {
                sector_placements.extend(placements);
            }
            all_clusters_strs.push(s);
        } else if path.ends_with("zones.xml") {
            all_zones_strs.push(s);
        } else if path.ends_with("sectors.xml") {
            all_sectors_strs.push(s);
        }
    }
    eprintln!(
        "[map] Universe XML files: {} (galaxy/clusters/zones/sectors merged)",
        universe_files.len()
    );

    let mut name_refs = HashMap::new();
    // sector macro (lowercase) → mineable resource areas, from `<resourceareas>`.
    let mut resource_areas_by_macro: HashMap<String, Vec<map_domain::resources::SectorResource>> =
        HashMap::new();
    for data in crate::cat_reader::read_all_game_files(game_dir, "libraries/mapdefaults.xml") {
        let s = String::from_utf8_lossy(&data);
        name_refs.extend(parse_sector_name_refs_xml(&s)?);
        for (macro_lc, areas) in parse_resource_areas_xml(&s) {
            resource_areas_by_macro
                .entry(macro_lc)
                .or_default()
                .extend(areas);
        }
    }
    let translations = parse_translations_xml(&translations_str)?;

    // Ware id → display name (e.g. "energycells" → "Energy Cells"). Non-fatal:
    // missing or unparseable wares.xml leaves `ware_names` empty, UI falls back
    // to raw ids. Multi-archive read for DLC extensions; first occurrence wins
    // (same convention as factions / colors).
    let mut ware_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut ware_factory_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for data in crate::cat_reader::read_all_game_files(game_dir, "libraries/wares.xml") {
        for (k, v) in parse_ware_names_xml(&data, &translations) {
            ware_names.entry(k).or_insert(v);
        }
        for (k, v) in parse_ware_factory_names_xml(&data, &translations) {
            ware_factory_names.entry(k).or_insert(v);
        }
    }
    eprintln!(
        "[map] Ware names: {}, factory names: {}",
        ware_names.len(),
        ware_factory_names.len()
    );

    // Job id → display name (e.g. "Builder Ship"). NPC ships prefix their
    // generated name with this. Multi-archive read for DLC; first wins.
    let mut job_names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for data in crate::cat_reader::read_all_game_files(game_dir, "libraries/jobs.xml") {
        for (k, v) in parse_jobs_xml(&data, &translations) {
            job_names.entry(k).or_insert(v);
        }
    }
    eprintln!("[map] Job names: {}", job_names.len());

    // Macro identifications: for each ship_*/station_* macro, read the macro
    // definition file and extract `<identification name="{p,t}"/>`. Used as a
    // fallback when a save entity lacks a per-instance name=/basename= attr.
    // First-match-wins across main + DLC archives (matches mapdefaults pattern).
    let mut macro_index: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for data in crate::cat_reader::read_all_game_files(game_dir, "index/macros.xml") {
        for (k, v) in crate::macro_index::parse_macros_index(&data) {
            macro_index.entry(k).or_insert(v);
        }
    }
    // Read each ship/station/environment macro file and extract its
    // `<identification>` ref. Each read used to re-scan every cat index from
    // scratch (O(macros × archives × lines)); instead fetch all needed files in
    // one parallel archive pass, then parse them in parallel.
    //
    // Restrict to ship + station macro files. Filter by asset PATH rather than
    // macro-name prefix so entries like landmarks_* (class="station" but no
    // `station_` prefix) are still picked up.
    let t_macro_id = std::time::Instant::now();
    let wanted_macro_files: std::collections::HashSet<String> = macro_index
        .values()
        .filter(|path| {
            path.starts_with("assets/units/")
                || path.starts_with("assets/structures/")
                || path.starts_with("assets/environments/")
        })
        .cloned()
        .collect();
    let macro_files = crate::cat_reader::read_files(game_dir, &wanted_macro_files);
    let macro_identifications: std::collections::HashMap<String, String> = macro_index
        .par_iter()
        .filter_map(|(macro_name, path)| {
            let macro_xml = macro_files.get(path)?;
            let ref_str = crate::macro_identification::parse_macro_identification(macro_xml)?;
            Some((macro_name.clone(), ref_str))
        })
        .collect();
    eprintln!(
        "[map] Macro identifications: {} ({} ms)",
        macro_identifications.len(),
        t_macro_id.elapsed().as_millis(),
    );
    // Production-module macro → ware it produces (e.g.
    // "prod_gen_energycells_macro" → "energycells"). Reuses the already-loaded
    // macro files; only `class="production"` macros carry `<production>`.
    let prod_module_wares: std::collections::HashMap<String, String> = macro_index
        .par_iter()
        .filter_map(|(macro_name, path)| {
            let macro_xml = macro_files.get(path)?;
            let ware = crate::xml_parser::parse_macro_production_ware(macro_xml)?;
            Some((macro_name.to_lowercase(), ware))
        })
        .collect();
    eprintln!("[map] Production-module wares: {}", prod_module_wares.len());

    // ---- Faction metadata: name + color from libraries/factions.xml + colors.xml.
    let mut faction_defs: std::collections::HashMap<String, crate::faction_parser::FactionDef> =
        std::collections::HashMap::new();
    for data in crate::cat_reader::read_all_game_files(game_dir, "libraries/factions.xml") {
        let text = String::from_utf8_lossy(&data);
        for (k, v) in crate::faction_parser::parse_factions_xml(&text) {
            faction_defs.entry(k).or_insert(v); // first occurrence wins (main loads first)
        }
    }
    let mut colors_map: std::collections::HashMap<String, [u8; 4]> =
        std::collections::HashMap::new();
    let mut mappings_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for data in crate::cat_reader::read_all_game_files(game_dir, "libraries/colors.xml") {
        let text = String::from_utf8_lossy(&data);
        let (c, m) = crate::faction_parser::parse_colors_xml(&text);
        for (k, v) in c {
            colors_map.entry(k).or_insert(v);
        }
        for (k, v) in m {
            mappings_map.entry(k).or_insert(v);
        }
    }
    eprintln!(
        "[map] Faction defs: {}, colors: {}, mappings: {}",
        faction_defs.len(),
        colors_map.len(),
        mappings_map.len()
    );

    // Group sectors by cluster
    let mut cluster_to_sectors: HashMap<String, Vec<String>> = HashMap::new();
    for (sector_macro, (cluster_macro, _, _)) in &sector_placements {
        cluster_to_sectors
            .entry(cluster_macro.clone())
            .or_default()
            .push(sector_macro.clone());
    }
    // Sort sector macros per cluster for deterministic layout
    for v in cluster_to_sectors.values_mut() {
        v.sort();
    }

    // Sort cluster macros for stable ID assignment
    let mut cluster_macros_sorted: Vec<String> = cluster_to_sectors.keys().cloned().collect();
    cluster_macros_sorted.sort();

    // Assign ClusterIds first so Sector entries can reference them.
    let mut cluster_macro_to_id: HashMap<String, ClusterId> = HashMap::new();
    for (i, cm) in cluster_macros_sorted.iter().enumerate() {
        cluster_macro_to_id.insert(cm.clone(), ClusterId((i + 1) as u32));
    }

    let mut sectors = Vec::new();
    let mut macro_to_id: HashMap<String, SectorId> = HashMap::new();
    let mut id_to_macro: HashMap<SectorId, String> = HashMap::new();
    let mut id_counter = 0u32;

    for cluster_macro in &cluster_macros_sorted {
        let (cx, cz) = cluster_positions
            .get(cluster_macro)
            .copied()
            .unwrap_or((0.0, 0.0));
        let cluster_center = Vec2::new(cx / 1_000_000.0, cz / 1_000_000.0);
        let sector_macros = &cluster_to_sectors[cluster_macro];
        let total = sector_macros.len();
        let cluster_id = cluster_macro_to_id[cluster_macro];

        for (idx, sector_macro) in sector_macros.iter().enumerate() {
            let name = name_refs
                .get(&sector_macro.to_lowercase())
                .and_then(|(pid, tid)| translations.get(&(*pid, *tid)))
                .map(|raw| resolve_display_name(raw, &translations))
                .or_else(|| {
                    let (pid, tid) = derive_sector_text_id(sector_macro)?;
                    translations
                        .get(&(pid, tid))
                        .map(|raw| resolve_display_name(raw, &translations))
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| macro_to_display_name(sector_macro));

            id_counter += 1;
            let id = SectorId(id_counter);
            macro_to_id.insert(sector_macro.clone(), id);
            id_to_macro.insert(id, sector_macro.clone());
            sectors.push(Sector {
                id,
                name,
                faction: None,
                map_position: cluster_center,
                static_objects: vec![],
                cluster_id: Some(cluster_id),
                index_in_cluster: idx as u32,
                cluster_sector_count: total as u32,
            });
        }
    }

    let mut clusters: Vec<Cluster> = Vec::new();
    for cluster_macro in &cluster_macros_sorted {
        let (cx, cz) = cluster_positions
            .get(cluster_macro)
            .copied()
            .unwrap_or((0.0, 0.0));
        let center = Vec2::new(cx / 1_000_000.0, cz / 1_000_000.0);
        let name = name_refs
            .get(&cluster_macro.to_lowercase())
            .and_then(|(pid, tid)| translations.get(&(*pid, *tid)))
            .map(|raw| resolve_display_name(raw, &translations))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| macro_to_display_name(cluster_macro));
        let cluster_id = cluster_macro_to_id[cluster_macro];
        clusters.push(Cluster {
            id: cluster_id,
            name,
            map_position: center,
            // Radius is unused now; renderer derives size from hex_r.
            radius: 0.0,
        });
    }
    eprintln!("[map] Clusters built: {}", clusters.len());

    let mut gate_pairs: Vec<(String, String)> = Vec::new();
    for zs in &all_zones_strs {
        gate_pairs.extend(parse_gate_connections_xml(zs, &sector_placements));
    }
    // Each zones.xml typically lists the same gate pair from both sides, so
    // we dedupe by normalised (low, high) sector-id pair to get one Standard
    // connection per gate. Superhighway connections are loaded separately
    // below and intentionally kept directional (one-way highways exist).
    let mut seen_pairs: std::collections::HashSet<(SectorId, SectorId)> =
        std::collections::HashSet::new();
    let mut connections: Vec<Connection> = Vec::new();
    for (a, b) in gate_pairs {
        let Some(&from) = macro_to_id.get(&a) else {
            continue;
        };
        let Some(&to) = macro_to_id.get(&b) else {
            continue;
        };
        if from == to {
            continue;
        }
        let key = if from.0 <= to.0 {
            (from, to)
        } else {
            (to, from)
        };
        if !seen_pairs.insert(key) {
            continue;
        }
        connections.push(Connection {
            from,
            to,
            gate_type: GateType::Standard,
        });
    }

    // Zone sector-relative positions (metres), keyed by zone macro. Gate/object
    // offsets in zones.xml are zone-relative; adding these yields true sector positions.
    let mut zone_positions: HashMap<String, (f32, f32, f32)> = HashMap::new();
    for ss in &all_sectors_strs {
        zone_positions.extend(parse_zone_positions_xml(ss));
    }
    eprintln!("[map] Zone positions: {}", zone_positions.len());

    // Parse gate positions and populate sectors.
    // Keys from parse_gate_positions_xml come from zones.xml zone names; id_to_macro keys come
    // from clusters.xml. DLC sectors may differ in casing — normalise both to lowercase.
    let mut gate_positions: HashMap<String, Vec<(f32, f32, f32, String, (f32, f32, f32))>> =
        HashMap::new();
    // Dedupe within each sector keyed on destination name — multiple DLC zones.xml
    // files often re-declare the same gate, and we don't want it listed twice in
    // the 3D static-objects section.
    let mut seen_gates: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    for zs in &all_zones_strs {
        for (k, v) in parse_gate_positions_xml(zs, &zone_positions) {
            let key = k.to_lowercase();
            let seen = seen_gates.entry(key.clone()).or_default();
            let list = gate_positions.entry(key).or_default();
            for entry in v {
                if seen.insert(entry.3.clone()) {
                    list.push(entry);
                }
            }
        }
    }
    // cluster_num → first sector display name for that cluster, used to label gates.
    let mut cluster_num_to_name: HashMap<u32, String> = HashMap::new();
    for sector in &sectors {
        if let Some(sector_macro) = id_to_macro.get(&sector.id) {
            if let Some((cluster_macro, ..)) = sector_placements.get(sector_macro) {
                if let Some(num) = cluster_macro
                    .strip_prefix("Cluster_")
                    .and_then(|s| s.strip_suffix("_macro"))
                    .and_then(|s| s.parse::<u32>().ok())
                {
                    cluster_num_to_name
                        .entry(num)
                        .or_insert(sector.name.clone());
                }
            }
        }
    }

    let mut gate_obj_counter = 0u32;
    for sector in &mut sectors {
        if let Some(gates) = id_to_macro
            .get(&sector.id)
            .and_then(|m| gate_positions.get(&m.to_lowercase()))
        {
            for (x, y, z, dest_name, rot) in gates {
                gate_obj_counter += 1;
                let dest_cluster_name = parse_gate_cluster_nums(dest_name)
                    .and_then(|(_, to_n)| cluster_num_to_name.get(&to_n).cloned());
                let gate_name = dest_cluster_name
                    .as_ref()
                    .map(|s| format!("Gate → {}", s))
                    .unwrap_or_else(|| dest_name.clone());
                let dest_for_details = dest_cluster_name.unwrap_or_else(|| dest_name.clone());
                sector.static_objects.push(StaticObject {
                    id: ObjectId(10_000 + gate_obj_counter),
                    kind: StaticObjectKind::Gate,
                    position: Vec3::new(*x, *y, *z),
                    faction: None,
                    name: gate_name,
                    rotation: Some(*rot),
                    details: vec![
                        ("Type".to_string(), "Standard gate".to_string()),
                        ("Destination".to_string(), dest_for_details),
                    ],
                });
            }
        }
    }
    eprintln!("[map] Gate objects loaded: {}", gate_obj_counter);

    // Parse non-gate static objects (stations, resource zones, anomalies) from zones.xml.
    let mut non_gate_objects: HashMap<
        String,
        Vec<(f32, f32, f32, StaticObjectKind, String, String)>,
    > = HashMap::new();
    for zs in &all_zones_strs {
        for (k, v) in parse_non_gate_objects_xml(zs, &zone_positions) {
            non_gate_objects
                .entry(k.to_lowercase())
                .or_default()
                .extend(v);
        }
    }
    let mut non_gate_counter = 0u32;
    for sector in &mut sectors {
        if let Some(objects) = id_to_macro
            .get(&sector.id)
            .and_then(|m| non_gate_objects.get(&m.to_lowercase()))
        {
            for (x, y, z, kind, name, macro_ref) in objects {
                non_gate_counter += 1;
                let resolved = resolve_static_object_name(
                    macro_ref,
                    &macro_identifications,
                    &translations,
                    name,
                );
                sector.static_objects.push(StaticObject {
                    id: ObjectId(20_000 + non_gate_counter),
                    kind: kind.clone(),
                    position: Vec3::new(*x, *y, *z),
                    faction: None,
                    name: resolved,
                    rotation: None,
                    details: vec![("Macro".to_string(), macro_ref.clone())],
                });
            }
        }
    }
    eprintln!("[map] Non-gate objects loaded: {}", non_gate_counter);

    // Parse fixed objects from god.xml (main + all DLC extensions).
    // Uses read_all_game_files since extensions supply additional god.xml entries.
    let mut god_counter = 0u32;
    let sector_macro_to_id: HashMap<String, SectorId> = macro_to_id
        .iter()
        .map(|(k, v)| (k.to_lowercase(), *v))
        .collect();
    for god_data in crate::cat_reader::read_all_game_files(game_dir, "libraries/god.xml") {
        let god_str = String::from_utf8_lossy(&god_data);
        for (sec_lower, x, y, z, kind, name, mac) in parse_god_xml(&god_str) {
            if let Some(&sec_id) = sector_macro_to_id.get(&sec_lower) {
                if let Some(sector) = sectors.iter_mut().find(|s| s.id == sec_id) {
                    god_counter += 1;
                    let resolved = resolve_static_object_name(
                        &mac,
                        &macro_identifications,
                        &translations,
                        &name,
                    );
                    sector.static_objects.push(StaticObject {
                        id: ObjectId(30_000 + god_counter),
                        kind,
                        position: Vec3::new(x, y, z),
                        faction: None,
                        name: resolved,
                        rotation: None,
                        details: vec![("Macro".to_string(), mac)],
                    });
                }
            }
        }
    }
    eprintln!("[map] God objects loaded: {}", god_counter);

    // Parse superhighway connections from clusters.xml (entry → exit, one-way).
    let mut sechighways: Vec<(String, String, String, String)> = Vec::new();
    for cs in &all_clusters_strs {
        sechighways.extend(parse_sechighway_connections_xml(cs));
    }
    // zone macro (lowercase) → (role, destination sector name)
    let mut sh_zone_info: HashMap<String, (ShRole, String)> = HashMap::new();
    for (from_sec, to_sec, entry_zone, exit_zone) in &sechighways {
        // Look up sector display names
        let from_name = sector_macro_to_id
            .get(from_sec)
            .and_then(|id| sectors.iter().find(|s| s.id == *id))
            .map(|s| s.name.clone())
            .unwrap_or_else(|| from_sec.clone());
        let to_name = sector_macro_to_id
            .get(to_sec)
            .and_then(|id| sectors.iter().find(|s| s.id == *id))
            .map(|s| s.name.clone())
            .unwrap_or_else(|| to_sec.clone());
        sh_zone_info.insert(entry_zone.clone(), (ShRole::Entry, to_name.clone()));
        sh_zone_info.insert(exit_zone.clone(), (ShRole::Exit, from_name.clone()));
        // Add 2D map connection (one-way; existing model is undirected so use from→to)
        if let (Some(&fid), Some(&tid)) = (
            sector_macro_to_id.get(from_sec),
            sector_macro_to_id.get(to_sec),
        ) {
            connections.push(Connection {
                from: fid,
                to: tid,
                gate_type: GateType::Superhighway,
            });
        }
    }
    eprintln!(
        "[map] Superhighway connections (sector pairs): {}",
        sechighways.len()
    );

    // Parse superhighway connection zones from sectors.xml (entry/exit points within sectors).
    let mut sh_counter = 0u32;
    for sectors_str in &all_sectors_strs {
        for (sec_lower, x, y, z, fallback_name, zone_lower) in
            parse_superhighway_zones_xml(sectors_str)
        {
            if let Some(&sec_id) = sector_macro_to_id.get(&sec_lower) {
                if let Some(sector) = sectors.iter_mut().find(|s| s.id == sec_id) {
                    sh_counter += 1;
                    let (name, mut details) = match sh_zone_info.get(&zone_lower) {
                        Some((ShRole::Entry, dest)) => (
                            format!("Superhighway Entry → {}", dest),
                            vec![
                                ("Type".to_string(), "Superhighway connection".to_string()),
                                ("Direction".to_string(), "Outbound".to_string()),
                                ("Destination".to_string(), dest.clone()),
                            ],
                        ),
                        Some((ShRole::Exit, src)) => (
                            format!("Superhighway Exit ← {}", src),
                            vec![
                                ("Type".to_string(), "Superhighway connection".to_string()),
                                ("Direction".to_string(), "Inbound".to_string()),
                                ("Destination".to_string(), src.clone()),
                            ],
                        ),
                        None => (
                            fallback_name,
                            vec![("Type".to_string(), "Superhighway connection".to_string())],
                        ),
                    };
                    // Silence unused warning when no extra mutation is needed.
                    let _ = &mut details;
                    sector.static_objects.push(StaticObject {
                        id: ObjectId(40_000 + sh_counter),
                        kind: StaticObjectKind::Highway,
                        position: Vec3::new(x, y, z),
                        faction: None,
                        name,
                        rotation: None,
                        details,
                    });
                }
            }
        }
    }
    eprintln!("[map] Superhighway connections loaded: {}", sh_counter);

    let sector_macros: HashMap<String, SectorId> = macro_to_id
        .iter()
        .map(|(k, v)| (k.to_lowercase(), *v))
        .collect();
    // Resolve resource areas (keyed by sector macro) to SectorId.
    let sector_resources: HashMap<SectorId, Vec<map_domain::resources::SectorResource>> =
        resource_areas_by_macro
            .into_iter()
            .filter_map(|(macro_lc, areas)| sector_macros.get(&macro_lc).map(|&id| (id, areas)))
            .collect();
    eprintln!(
        "[map] Sectors with resources: {} ({} areas)",
        sector_resources.len(),
        sector_resources.values().map(Vec::len).sum::<usize>(),
    );
    let available_locales = crate::game_path::list_available_locales(game_dir);
    let mut universe = Universe {
        sectors,
        clusters,
        connections,
        sector_macros,
        faction_strings: HashMap::new(),
        faction_table: HashMap::new(),
        ware_names,
        ware_factory_names,
        prod_module_wares,
        job_names,
        translations,
        available_locales,
        current_locale: locale,
        macro_identifications,
        zone_positions,
        sector_resources,
    };

    // Assign sequential FactionIds and populate the faction table.
    let mut next_id: u32 = 1;
    let mut sorted_keys: Vec<&String> = faction_defs.keys().collect();
    sorted_keys.sort(); // deterministic id assignment across runs
    for faction_id_str in sorted_keys {
        let def = &faction_defs[faction_id_str];
        let fid = map_domain::ids::FactionId(next_id);
        next_id += 1;
        let display_name = universe
            .translations
            .get(&def.name_textref)
            .cloned()
            .unwrap_or_else(|| faction_id_str.clone());
        let short_name = def
            .short_name_textref
            .and_then(|r| universe.translations.get(&r))
            .cloned()
            .unwrap_or_default();
        let color = crate::faction_parser::resolve_faction_color(
            &def.color_mapping,
            &colors_map,
            &mappings_map,
        )
        .unwrap_or([192, 192, 192, 255]);
        universe.faction_strings.insert(faction_id_str.clone(), fid);
        universe.faction_table.insert(
            fid,
            map_domain::universe::FactionMeta {
                display_name,
                short_name,
                color,
            },
        );
    }
    eprintln!(
        "[map] Built faction table: {} factions",
        universe.faction_table.len()
    );

    Ok(universe)
}

/// galaxy.xml: cluster_macro_name → absolute (x, z) position in metres.
///
/// Structure: single `class="galaxy"` macro whose `<connections>` has one
/// `<connection ref="clusters">` per cluster. Each may have an optional
/// `<offset><position x z /></offset>` before the self-closing
/// `<macro ref="Cluster_XX_macro" connection="galaxy" />`.
fn parse_cluster_positions_xml(xml: &str) -> Result<HashMap<String, (f32, f32)>, ParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut positions: HashMap<String, (f32, f32)> = HashMap::new();
    let mut in_galaxy = false;
    let mut in_conn = false; // inside a <connection ref="clusters"> element
    let mut in_offset = false;
    let mut conn_pos: (f32, f32) = (0.0, 0.0);
    let mut conn_cluster_ref: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,

            Event::Start(ref e) => match e.name().as_ref() {
                b"macro" => {
                    if attr_value(e, b"class").as_deref() == Some("galaxy") {
                        in_galaxy = true;
                    }
                }
                // DLC galaxy.xml uses <diff><add sel="..."> wrapper instead of <macro class="galaxy">.
                b"diff" | b"add" => in_galaxy = true,
                b"connection" if in_galaxy => {
                    if attr_value(e, b"ref").as_deref() == Some("clusters") {
                        in_conn = true;
                        conn_pos = (0.0, 0.0);
                        conn_cluster_ref = None;
                    }
                }
                b"offset" if in_conn => in_offset = true,
                _ => {}
            },

            Event::Empty(ref e) => match e.name().as_ref() {
                b"position" if in_offset => {
                    conn_pos.0 = attr_value(e, b"x")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    conn_pos.1 = attr_value(e, b"z")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                }
                b"macro" if in_conn => {
                    // connection="galaxy" identifies the cluster ref in galaxy.xml
                    if attr_value(e, b"connection").as_deref() == Some("galaxy") {
                        conn_cluster_ref = attr_value(e, b"ref");
                    }
                }
                _ => {}
            },

            Event::End(ref e) => match e.name().as_ref() {
                b"offset" => in_offset = false,
                b"connection" if in_conn => {
                    if let Some(cref) = conn_cluster_ref.take() {
                        positions.insert(cref, conn_pos);
                    }
                    in_conn = false;
                }
                b"macro" if in_galaxy => in_galaxy = false,
                _ => {}
            },

            _ => {}
        }
        buf.clear();
    }

    Ok(positions)
}

/// clusters.xml: sector_macro_name → (cluster_macro_name, relative_x, relative_z).
///
/// Each `class="cluster"` macro has `<connections>` with `<connection ref="sectors">`
/// per sector.  Each sector connection has an optional `<offset><position x z /></offset>`
/// and a self-closing `<macro ref="Cluster_XX_SectorYYY_macro" connection="cluster" />`.
fn parse_sector_placements_xml(
    xml: &str,
) -> Result<HashMap<String, (String, f32, f32)>, ParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut placements: HashMap<String, (String, f32, f32)> = HashMap::new();
    let mut current_cluster: Option<String> = None;
    // Depth of non-cluster macros nested inside current_cluster macro.
    // Needed to distinguish End("macro") for cluster vs nested highway macros.
    let mut nested_macro_depth: u32 = 0;
    let mut in_sector_conn = false;
    let mut in_offset = false;
    let mut conn_pos: (f32, f32) = (0.0, 0.0);
    let mut conn_sector_ref: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,

            Event::Start(ref e) => match e.name().as_ref() {
                b"macro" => {
                    if attr_value(e, b"class").as_deref() == Some("cluster") {
                        current_cluster = attr_value(e, b"name");
                        nested_macro_depth = 0;
                    } else if current_cluster.is_some() {
                        // Non-sector macro (e.g. highway macro) nested inside cluster
                        nested_macro_depth += 1;
                    }
                }
                b"connection" if current_cluster.is_some() => {
                    if attr_value(e, b"ref").as_deref() == Some("sectors") {
                        in_sector_conn = true;
                        conn_pos = (0.0, 0.0);
                        conn_sector_ref = None;
                    }
                }
                b"offset" if in_sector_conn => in_offset = true,
                _ => {}
            },

            Event::Empty(ref e) => match e.name().as_ref() {
                b"position" if in_offset => {
                    conn_pos.0 = attr_value(e, b"x")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    conn_pos.1 = attr_value(e, b"z")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                }
                b"macro" if in_sector_conn => {
                    // connection="cluster" identifies the sector macro inside a cluster
                    if attr_value(e, b"connection").as_deref() == Some("cluster") {
                        conn_sector_ref = attr_value(e, b"ref");
                    }
                }
                _ => {}
            },

            Event::End(ref e) => match e.name().as_ref() {
                b"offset" => in_offset = false,
                b"connection" if in_sector_conn => {
                    if let (Some(sref), Some(cluster)) =
                        (conn_sector_ref.take(), current_cluster.as_deref())
                    {
                        placements.insert(sref, (cluster.to_string(), conn_pos.0, conn_pos.1));
                    }
                    in_sector_conn = false;
                }
                b"macro" if current_cluster.is_some() => {
                    if nested_macro_depth == 0 {
                        // This End closes the cluster macro itself
                        current_cluster = None;
                    } else {
                        nested_macro_depth -= 1;
                    }
                }
                _ => {}
            },

            _ => {}
        }
        buf.clear();
    }

    Ok(placements)
}

/// mapdefaults.xml: sector_macro_name → (page_id, text_id) for name lookup.
///
/// Each `<dataset macro="...">` may contain `<identification name="{pageId,textId}"/>`.
fn parse_sector_name_refs_xml(xml: &str) -> Result<HashMap<String, (u32, u32)>, ParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut name_refs: HashMap<String, (u32, u32)> = HashMap::new();
    let mut current_macro: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,

            Event::Start(ref e) | Event::Empty(ref e) => match e.name().as_ref() {
                b"dataset" => {
                    current_macro = attr_value(e, b"macro");
                }
                b"identification" => {
                    if let (Some(macro_name), Some(name_attr)) =
                        (&current_macro, attr_value(e, b"name"))
                    {
                        if let Some(ids) = parse_page_text_ref(&name_attr) {
                            name_refs.insert(macro_name.to_lowercase(), ids);
                        }
                    }
                }
                _ => {}
            },

            Event::End(ref e) => {
                if e.name().as_ref() == b"dataset" {
                    current_macro = None;
                }
            }

            _ => {}
        }
        buf.clear();
    }

    Ok(name_refs)
}

/// Translation file: (page_id, text_id) → display name string.
///
/// Retains every page as plain trimmed text. Composite entries on pages 20003
/// (cluster names) and 20004 (sector names) like `{20005,6002}(Grand Exchange)`
/// are stored verbatim so the runtime resolver can substitute `{p,t}` refs
/// against the chosen locale's translations and apply X4 display extraction.
fn parse_translations_xml(xml: &str) -> Result<HashMap<(u32, u32), String>, ParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut translations: HashMap<(u32, u32), String> = HashMap::new();
    let mut current_page: Option<u32> = None;
    let mut current_text_id: Option<u32> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,

            Event::Start(ref e) => match e.name().as_ref() {
                b"page" => {
                    current_page = attr_value(e, b"id").and_then(|s| s.parse().ok());
                }
                b"t" => {
                    current_text_id = attr_value(e, b"id").and_then(|s| s.parse().ok());
                }
                _ => {}
            },

            Event::Text(e) => {
                if let (Some(page_id), Some(text_id)) = (current_page, current_text_id) {
                    let decoded = e.decode().unwrap_or_default();
                    let content =
                        quick_xml::escape::unescape(&decoded).unwrap_or_else(|_| decoded.clone());
                    // Store the raw trimmed text for every page. Display-time
                    // logic in map_domain::translations resolves {p,t} refs
                    // and applies X4 display-name extraction.
                    let trimmed = content.trim();
                    if !trimmed.is_empty() {
                        translations.insert((page_id, text_id), trimmed.to_string());
                    }
                    current_text_id = None;
                }
            }

            Event::End(ref e) => match e.name().as_ref() {
                b"page" => current_page = None,
                b"t" => current_text_id = None,
                _ => {}
            },

            _ => {}
        }
        buf.clear();
    }

    Ok(translations)
}

/// Parse `{pageId,textId}` from a string like `{20004,10011}`.
fn parse_page_text_ref(s: &str) -> Option<(u32, u32)> {
    let inner = s.trim().strip_prefix('{')?.strip_suffix('}')?;
    let (page, text) = inner.split_once(',')?;
    let page_id: u32 = page.trim().parse().ok()?;
    let text_id: u32 = text.trim().parse().ok()?;
    Some((page_id, text_id))
}

/// zones.xml: return deduplicated (from_sector_macro, to_sector_macro) gate pairs.
///
/// Parses zone macros for gate connections whose names follow the pattern
/// `connection_ClusterGate{A}To{B}`. Two-pass matching pairs A→B with B→A entries
/// to get the exact destination sector; falls back to any sector in the dest cluster.
fn parse_gate_connections_xml(
    xml: &str,
    sector_placements: &HashMap<String, (String, f32, f32)>,
) -> Vec<(String, String)> {
    // cluster_macro → first sector macro seen for that cluster
    let mut cluster_first: HashMap<String, String> = HashMap::new();
    for (sector, (cluster, ..)) in sector_placements {
        cluster_first
            .entry(cluster.clone())
            .or_insert_with(|| sector.clone());
    }

    // First pass: collect (source_sector, from_cluster_num, to_cluster_num)
    let mut raw: Vec<(String, u32, u32)> = Vec::new();
    let mut current_sector: Option<String> = None;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"macro" => {
                    let class = attr_value(e, b"class").unwrap_or_default();
                    if class == "zone" {
                        let name = attr_value(e, b"name").unwrap_or_default();
                        current_sector = zone_name_to_sector_macro(&name);
                    }
                }
                b"connection" => {
                    if let Some(ref src) = current_sector {
                        if let Some(conn_name) = attr_value(e, b"name") {
                            if let Some((from_n, to_n)) = parse_gate_cluster_nums(&conn_name) {
                                raw.push((src.clone(), from_n, to_n));
                            }
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::End(ref e)) => {
                if e.name().as_ref() == b"macro" {
                    current_sector = None;
                }
            }
            _ => {}
        }
        buf.clear();
    }

    // Build lookup: (from_n, to_n) → sectors that have a gate in that direction
    let mut gate_map: HashMap<(u32, u32), Vec<String>> = HashMap::new();
    for (sector, from_n, to_n) in &raw {
        gate_map
            .entry((*from_n, *to_n))
            .or_default()
            .push(sector.clone());
    }

    // Second pass: match each gate with its reverse to get exact sector pair
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut connections = Vec::new();

    for (sector_a, from_n, to_n) in &raw {
        // Prefer the sector in dest cluster that has a matching return gate
        let sector_b = gate_map
            .get(&(*to_n, *from_n))
            .and_then(|v| v.first())
            .map(String::as_str)
            .or_else(|| {
                let dest_cluster = format!("Cluster_{:02}_macro", to_n);
                cluster_first.get(&dest_cluster).map(String::as_str)
            });

        if let Some(sb) = sector_b {
            let key = if sector_a.as_str() <= sb {
                (sector_a.clone(), sb.to_string())
            } else {
                (sb.to_string(), sector_a.clone())
            };
            if seen.insert(key) {
                connections.push((sector_a.clone(), sb.to_string()));
            }
        }
    }

    connections
}

/// zones.xml: sector_macro_name → list of gate positions (x, y, z in km, dest_connection_name).
///
/// Gate connections are named `connection_ClusterGate{N}To{M}`.
/// Their `<offset><position x y z />` is in metres; we divide by 1000 to get km.
fn parse_gate_positions_xml(
    xml: &str,
    zone_positions: &HashMap<String, (f32, f32, f32)>,
) -> HashMap<String, Vec<(f32, f32, f32, String, (f32, f32, f32))>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut result: HashMap<String, Vec<(f32, f32, f32, String, (f32, f32, f32))>> = HashMap::new();
    let mut current_sector: Option<String> = None;
    // Sector-relative position of the zone currently being parsed (metres). Gate
    // offsets are zone-relative; add this to recover the true sector position.
    let mut current_zone_off: (f32, f32, f32) = (0.0, 0.0, 0.0);
    let mut in_gate_conn = false;
    let mut in_offset = false;
    let mut gate_dest: Option<String> = None;
    let mut gate_pos: (f32, f32, f32) = (0.0, 0.0, 0.0);
    let mut gate_rot: (f32, f32, f32) = (0.0, 0.0, 0.0);
    // First `<offset>` under a gate connection wins — animated-gate prop macros
    // nest their own offsets, which must not overwrite the gate's position.
    let mut gate_pos_set = false;
    // `<macro>` nesting depth and the depth at which the current zone macro opened.
    // A gate's prop macro is also a `<macro>`; its `</macro>` must NOT reset the
    // zone/gate state — only the zone macro's own close does.
    let mut macro_depth: u32 = 0;
    let mut zone_macro_depth: Option<u32> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"macro" => {
                    macro_depth += 1;
                    let class = attr_value(e, b"class").unwrap_or_default();
                    if class == "zone" {
                        let name = attr_value(e, b"name").unwrap_or_default();
                        current_sector = zone_name_to_sector_macro(&name);
                        current_zone_off = zone_positions
                            .get(&name.to_lowercase())
                            .copied()
                            .unwrap_or((0.0, 0.0, 0.0));
                        zone_macro_depth = Some(macro_depth);
                    }
                }
                b"connection" if current_sector.is_some() => {
                    let conn_name = attr_value(e, b"name").unwrap_or_default();
                    if parse_gate_cluster_nums(&conn_name).is_some() {
                        in_gate_conn = true;
                        gate_pos = (0.0, 0.0, 0.0);
                        gate_rot = (0.0, 0.0, 0.0);
                        gate_pos_set = false;
                        gate_dest = Some(conn_name);
                    }
                }
                b"offset" if in_gate_conn => in_offset = true,
                _ => {}
            },
            Ok(Event::Empty(ref e)) => {
                if in_offset && !gate_pos_set {
                    match e.name().as_ref() {
                        b"position" => {
                            gate_pos.0 = attr_value(e, b"x")
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0.0);
                            gate_pos.1 = attr_value(e, b"y")
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0.0);
                            gate_pos.2 = attr_value(e, b"z")
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0.0);
                        }
                        b"quaternion" => {
                            let qx: f32 = attr_value(e, b"qx")
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0.0);
                            let qy: f32 = attr_value(e, b"qy")
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0.0);
                            let qz: f32 = attr_value(e, b"qz")
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0.0);
                            let qw: f32 = attr_value(e, b"qw")
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(1.0);
                            let q = glam::Quat::from_xyzw(qx, qy, qz, qw).normalize();
                            let (yaw, pitch, roll) = q.to_euler(glam::EulerRot::YXZ);
                            gate_rot = (pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees());
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"offset" => {
                    in_offset = false;
                    // Lock the gate's position after its first offset closes so
                    // nested prop-macro offsets don't clobber it.
                    if in_gate_conn {
                        gate_pos_set = true;
                    }
                }
                b"connection" if in_gate_conn => {
                    if let (Some(sector), Some(dest)) = (&current_sector, gate_dest.take()) {
                        result.entry(sector.clone()).or_default().push((
                            (gate_pos.0 + current_zone_off.0) / 1000.0,
                            (gate_pos.1 + current_zone_off.1) / 1000.0,
                            (gate_pos.2 + current_zone_off.2) / 1000.0,
                            dest,
                            gate_rot,
                        ));
                    }
                    in_gate_conn = false;
                }
                b"macro" => {
                    // Only the zone macro's own close clears the zone context.
                    // Inner `<macro>` (e.g. gate props) must not reset it.
                    if zone_macro_depth == Some(macro_depth) {
                        current_sector = None;
                        current_zone_off = (0.0, 0.0, 0.0);
                        zone_macro_depth = None;
                        in_gate_conn = false;
                    }
                    macro_depth = macro_depth.saturating_sub(1);
                }
                _ => {}
            },
            _ => {}
        }
        buf.clear();
    }
    result
}

/// `Zone003_Cluster_01_Sector001_macro` → `Cluster_01_Sector001_macro`
pub fn zone_name_to_sector_macro(name: &str) -> Option<String> {
    if !name.starts_with("Zone") {
        return None;
    }
    let start = name.find("Cluster_")?;
    let s = &name[start..];
    if s.ends_with("_macro") {
        Some(s.to_string())
    } else {
        None
    }
}

/// Extract sector macro (lowercase) from a zone macro name.
/// `"zone001_cluster_05_sector001_macro"` → `"cluster_05_sector001_macro"`
fn zone_macro_to_sector_lower(mac: &str) -> Option<String> {
    let lower = mac.to_lowercase();
    lower.find("cluster_").map(|pos| lower[pos..].to_string())
}

/// Extract sector macro (lowercase) from a SHCon zone macro name.
/// `"tzoneCluster_01_Sector002SHCon5_GateZone_macro"` → `"cluster_01_sector002_macro"`
fn shcon_zone_to_sector_macro_lower(zm: &str) -> Option<String> {
    let lower = zm.to_lowercase();
    let start = lower.find("cluster_")?;
    let rest = &lower[start..];
    let end = rest.find("shcon")?;
    Some(format!("{}_macro", &rest[..end]))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShRole {
    Entry,
    Exit,
}

/// clusters.xml: parse sechighway connections.
/// Returns list of (from_sector_macro_lower, to_sector_macro_lower, entry_zone_macro_lower, exit_zone_macro_lower).
fn parse_sechighway_connections_xml(xml: &str) -> Vec<(String, String, String, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut result = Vec::new();
    let mut in_sh = false;
    let mut current_role: Option<ShRole> = None;
    let mut entry_sec: Option<String> = None;
    let mut exit_sec: Option<String> = None;
    let mut entry_zone: Option<String> = None;
    let mut exit_zone: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"connection" {
                    let r = attr_value(e, b"ref").unwrap_or_default();
                    if r == "sechighways" {
                        in_sh = true;
                        entry_sec = None;
                        exit_sec = None;
                        entry_zone = None;
                        exit_zone = None;
                    } else if in_sh {
                        current_role = match r.as_str() {
                            "entrypoint" => Some(ShRole::Entry),
                            "exitpoint" => Some(ShRole::Exit),
                            _ => None,
                        };
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                if in_sh && current_role.is_some() && e.name().as_ref() == b"macro" {
                    if let Some(ref_attr) = attr_value(e, b"ref") {
                        let zone_lower = ref_attr.to_lowercase();
                        if let Some(sec) = shcon_zone_to_sector_macro_lower(&zone_lower) {
                            match current_role {
                                Some(ShRole::Entry) => {
                                    entry_sec = Some(sec);
                                    entry_zone = Some(zone_lower);
                                }
                                Some(ShRole::Exit) => {
                                    exit_sec = Some(sec);
                                    exit_zone = Some(zone_lower);
                                }
                                None => {}
                            }
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                if e.name().as_ref() == b"connection" {
                    if current_role.is_some() {
                        current_role = None;
                    } else if in_sh {
                        if let (Some(from), Some(to), Some(ez), Some(xz)) = (
                            entry_sec.take(),
                            exit_sec.take(),
                            entry_zone.take(),
                            exit_zone.take(),
                        ) {
                            result.push((from, to, ez, xz));
                        }
                        in_sh = false;
                    }
                }
            }
            _ => {}
        }
        buf.clear();
    }
    result
}

/// sectors.xml: SHCon zones — superhighway entry/exit points within a sector.
/// Returns (sector_macro_lower, x_km, y_km, z_km, display_name, zone_macro_lower).
/// Zone macro pattern: `tzone<SectorMacroFragment>SHCon{N}_GateZone_macro`.
fn parse_superhighway_zones_xml(xml: &str) -> Vec<(String, f32, f32, f32, String, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut result = Vec::new();
    let mut current_sector: Option<String> = None;
    let mut in_conn = false;
    let mut in_offset = false;
    let mut pos: (f32, f32, f32) = (0.0, 0.0, 0.0);
    let mut zone_macro: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"macro" => {
                    let class = attr_value(e, b"class").unwrap_or_default();
                    if class == "sector" {
                        current_sector = attr_value(e, b"name").map(|s| s.to_lowercase());
                    }
                }
                b"connection" if current_sector.is_some() => {
                    in_conn = true;
                    pos = (0.0, 0.0, 0.0);
                    zone_macro = None;
                }
                b"offset" if in_conn => in_offset = true,
                _ => {}
            },
            Ok(Event::Empty(ref e)) => match e.name().as_ref() {
                b"position" if in_offset => {
                    pos.0 = attr_value(e, b"x")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    pos.1 = attr_value(e, b"y")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    pos.2 = attr_value(e, b"z")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                }
                b"macro" if in_conn => {
                    zone_macro = attr_value(e, b"ref");
                }
                _ => {}
            },
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"offset" => in_offset = false,
                b"connection" if in_conn => {
                    if let (Some(sec), Some(zm)) = (&current_sector, zone_macro.take()) {
                        // Only SHCon zones (superhighway connection points).
                        let zm_lower = zm.to_lowercase();
                        if zm_lower.contains("shcon") {
                            // Extract SHConN for display fallback.
                            let shcon_label = zm_lower
                                .find("shcon")
                                .map(|i| {
                                    let rest = &zm_lower[i..];
                                    let end = rest
                                        .find(|c: char| !c.is_ascii_alphanumeric())
                                        .unwrap_or(rest.len());
                                    rest[..end].to_string()
                                })
                                .unwrap_or_else(|| "shcon".into());
                            let name = format!("Superhighway {}", shcon_label.to_uppercase());
                            result.push((
                                sec.clone(),
                                pos.0 / 1000.0,
                                pos.1 / 1000.0,
                                pos.2 / 1000.0,
                                name,
                                zm_lower,
                            ));
                        }
                    }
                    in_conn = false;
                }
                b"macro" => {
                    current_sector = None;
                    in_conn = false;
                }
                _ => {}
            },
            _ => {}
        }
        buf.clear();
    }
    result
}

/// sectors.xml: each `<macro class="sector">` lists `<connection ref="zones">`
/// entries that place every zone at a sector-relative `<offset><position>` (metres).
/// Returns `zone_macro_lowercase → (x, y, z) metres`.
///
/// Object/gate offsets inside a zone (from zones.xml) are *zone-relative*; adding
/// the zone's sector position here yields the true sector-relative position.
fn parse_zone_positions_xml(xml: &str) -> HashMap<String, (f32, f32, f32)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut result: HashMap<String, (f32, f32, f32)> = HashMap::new();
    let mut in_sector = false;
    let mut in_zone_conn = false;
    let mut in_offset = false;
    let mut pos: (f32, f32, f32) = (0.0, 0.0, 0.0);
    let mut zone_macro: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"macro" => {
                    if attr_value(e, b"class").as_deref() == Some("sector") {
                        in_sector = true;
                    }
                }
                b"connection" if in_sector => {
                    if attr_value(e, b"ref").as_deref() == Some("zones") {
                        in_zone_conn = true;
                        pos = (0.0, 0.0, 0.0);
                        zone_macro = None;
                    }
                }
                b"offset" if in_zone_conn => in_offset = true,
                _ => {}
            },
            Ok(Event::Empty(ref e)) => match e.name().as_ref() {
                b"position" if in_offset => {
                    pos.0 = attr_value(e, b"x")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    pos.1 = attr_value(e, b"y")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    pos.2 = attr_value(e, b"z")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                }
                b"macro" if in_zone_conn => {
                    zone_macro = attr_value(e, b"ref");
                }
                _ => {}
            },
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"offset" => in_offset = false,
                b"connection" if in_zone_conn => {
                    if let Some(zm) = zone_macro.take() {
                        result.insert(zm.to_lowercase(), pos);
                    }
                    in_zone_conn = false;
                }
                b"macro" if in_sector => {
                    // Closing the sector macro (zone macros emit Empty `<macro ref/>`,
                    // so a Start/End `macro` pair here is the sector wrapper).
                    in_sector = false;
                    in_zone_conn = false;
                }
                _ => {}
            },
            _ => {}
        }
        buf.clear();
    }
    result
}

/// god.xml: fixed object placements.
/// Returns (sector_macro_lowercase, x_km, y_km, z_km, kind, display_name, raw_macro_ref).
fn parse_god_xml(xml: &str) -> Vec<(String, f32, f32, f32, StaticObjectKind, String, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut result = Vec::new();
    let mut in_obj = false;
    let mut loc_sec: Option<String> = None;
    let mut obj_pos: Option<(f32, f32, f32)> = None;
    let mut obj_mac: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"object" && attr_value(e, b"id").is_some() {
                    in_obj = true;
                    loc_sec = None;
                    obj_pos = None;
                    obj_mac = None;
                }
            }
            Ok(Event::Empty(ref e)) if in_obj => match e.name().as_ref() {
                b"location" => {
                    let cls = attr_value(e, b"class").unwrap_or_default();
                    let mac = attr_value(e, b"macro").unwrap_or_default();
                    loc_sec = if cls == "sector" {
                        Some(mac.to_lowercase())
                    } else {
                        zone_macro_to_sector_lower(&mac)
                    };
                }
                b"position" => {
                    let x: f32 = attr_value(e, b"x")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    let y: f32 = attr_value(e, b"y")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    let z: f32 = attr_value(e, b"z")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    obj_pos = Some((x / 1000.0, y / 1000.0, z / 1000.0));
                }
                b"object" => {
                    obj_mac = attr_value(e, b"macro");
                }
                _ => {}
            },
            Ok(Event::End(ref e)) => {
                if e.name().as_ref() == b"object" && in_obj {
                    if let (Some(sec), Some((x, y, z)), Some(mac)) =
                        (loc_sec.take(), obj_pos.take(), obj_mac.take())
                    {
                        let kind = classify_static_object(&mac);
                        let name = macro_to_display_name(&mac);
                        result.push((sec, x, y, z, kind, name, mac));
                    }
                    in_obj = false;
                }
            }
            _ => {}
        }
        buf.clear();
    }
    result
}

/// mapdefaults.xml: sector dataset macro (lowercase) → mineable resource areas.
///
/// Walks `<dataset macro=...><properties><resourceareas><resourcearea amount ref/>`.
/// Each `ref` is a 5-token `sphere_<size>_<ware>_<tier>_<speed>`; we take the
/// ware (token 2) and tier (token 3). Refs that don't parse are skipped.
fn parse_resource_areas_xml(
    xml: &str,
) -> HashMap<String, Vec<map_domain::resources::SectorResource>> {
    use map_domain::resources::SectorResource;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut result: HashMap<String, Vec<SectorResource>> = HashMap::new();
    let mut current_macro: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"dataset" => {
                current_macro = attr_value(e, b"macro").map(|m| m.to_lowercase());
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"dataset" => {
                current_macro = None;
            }
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e))
                if e.name().as_ref() == b"resourcearea" =>
            {
                if let Some(macro_lc) = &current_macro {
                    if let Some(area) = parse_resource_area(e) {
                        result.entry(macro_lc.clone()).or_default().push(area);
                    }
                }
            }
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    result
}

fn parse_resource_area(
    e: &quick_xml::events::BytesStart<'_>,
) -> Option<map_domain::resources::SectorResource> {
    use map_domain::resources::{ResourceTier, SectorResource};
    let r = attr_value(e, b"ref")?;
    let parts: Vec<&str> = r.split('_').collect();
    if parts.len() != 5 || parts[0] != "sphere" {
        return None;
    }
    let tier = ResourceTier::parse(parts[3])?;
    let amount = attr_value(e, b"amount")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    Some(SectorResource {
        ware: parts[2].to_string(),
        tier,
        amount,
    })
}

/// Classify a static object by its macro reference name.
fn classify_static_object(macro_ref: &str) -> StaticObjectKind {
    let lower = macro_ref.to_lowercase();
    if lower.contains("station")
        || lower.contains("dock")
        || lower.contains("platform")
        || lower.contains("_hq")
    {
        StaticObjectKind::Station
    } else if lower.contains("resource")
        || lower.contains("asteroid")
        || lower.contains("gas")
        || lower.contains("field")
        || lower.contains("debris")
    {
        StaticObjectKind::ResourceZone
    } else {
        StaticObjectKind::Anomaly
    }
}

/// zones.xml: sector_macro_name → non-gate object positions (x, y, z in km, kind, display name, raw macro ref).
/// Picks up `ref="object"` connections in zone macros and classifies by macro ref name.
fn parse_non_gate_objects_xml(
    xml: &str,
    zone_positions: &HashMap<String, (f32, f32, f32)>,
) -> HashMap<String, Vec<(f32, f32, f32, StaticObjectKind, String, String)>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut result: HashMap<String, Vec<(f32, f32, f32, StaticObjectKind, String, String)>> =
        HashMap::new();
    let mut current_sector: Option<String> = None;
    // Sector-relative offset of the current zone (metres); object offsets are zone-relative.
    let mut current_zone_off: (f32, f32, f32) = (0.0, 0.0, 0.0);
    let mut in_object_conn = false;
    let mut in_offset = false;
    let mut obj_pos: (f32, f32, f32) = (0.0, 0.0, 0.0);
    let mut obj_macro: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"macro" => {
                    let class = attr_value(e, b"class").unwrap_or_default();
                    if class == "zone" {
                        let name = attr_value(e, b"name").unwrap_or_default();
                        current_sector = zone_name_to_sector_macro(&name);
                        current_zone_off = zone_positions
                            .get(&name.to_lowercase())
                            .copied()
                            .unwrap_or((0.0, 0.0, 0.0));
                    }
                }
                b"connection" if current_sector.is_some() => {
                    let conn_ref = attr_value(e, b"ref").unwrap_or_default();
                    let conn_name = attr_value(e, b"name").unwrap_or_default();
                    let is_gate =
                        conn_ref == "gates" || parse_gate_cluster_nums(&conn_name).is_some();
                    if !is_gate && !conn_ref.ends_with("_gate") && !conn_ref.is_empty() {
                        in_object_conn = true;
                        obj_pos = (0.0, 0.0, 0.0);
                        obj_macro = None;
                    }
                }
                b"offset" if in_object_conn => in_offset = true,
                _ => {}
            },
            Ok(Event::Empty(ref e)) => match e.name().as_ref() {
                b"position" if in_offset => {
                    obj_pos.0 = attr_value(e, b"x")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    obj_pos.1 = attr_value(e, b"y")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    obj_pos.2 = attr_value(e, b"z")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                }
                b"macro" if in_object_conn => {
                    obj_macro = attr_value(e, b"ref");
                }
                _ => {}
            },
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"offset" => in_offset = false,
                b"connection" if in_object_conn => {
                    if let (Some(sector), Some(macro_ref)) = (&current_sector, obj_macro.take()) {
                        let kind = classify_static_object(&macro_ref);
                        let name = macro_to_display_name(&macro_ref);
                        result.entry(sector.clone()).or_default().push((
                            (obj_pos.0 + current_zone_off.0) / 1000.0,
                            (obj_pos.1 + current_zone_off.1) / 1000.0,
                            (obj_pos.2 + current_zone_off.2) / 1000.0,
                            kind,
                            name,
                            macro_ref,
                        ));
                    }
                    in_object_conn = false;
                }
                b"macro" => {
                    current_sector = None;
                    in_object_conn = false;
                }
                _ => {}
            },
            _ => {}
        }
        buf.clear();
    }
    result
}

/// `connection_ClusterGate001To004` → `(1, 4)`
/// `connection_ClusterGate031To601b` → `(31, 601)`. A cluster token may carry a
/// trailing letter (`601b`) when a sector has more than one gate to the same
/// destination cluster — the letter just disambiguates the gate, so parse the
/// leading digits and ignore it. Dropping such gates is why multi-gate sectors
/// (e.g. Heretic's End → 3 gates) previously showed too few.
fn parse_gate_cluster_nums(name: &str) -> Option<(u32, u32)> {
    let inner = name.strip_prefix("connection_ClusterGate")?;
    let sep = inner.find("To")?;
    let from = leading_digits_u32(&inner[..sep])?;
    let to = leading_digits_u32(&inner[sep + 2..])?;
    Some((from, to))
}

/// Parse the leading ASCII-digit run of `s` (e.g. `"601b"` → `601`, `"031"` → `31`).
fn leading_digits_u32(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Derive (page_id=20004, text_id) from macro name pattern `Cluster_N_SectorM_macro`.
/// Text ID formula: cluster_num * 10000 + sector_num * 10 + 1.
/// Matches the X4 convention used for all sectors including those without mapdefaults entries.
fn derive_sector_text_id(macro_name: &str) -> Option<(u32, u32)> {
    let lower = macro_name.to_lowercase();
    let rest = lower.strip_prefix("cluster_")?;
    let sep = rest.find("_sector")?;
    let cluster_num: u32 = rest[..sep].parse().ok()?;
    let after_sector = &rest[sep + 7..]; // "_sector" = 7 chars
    let sector_part = after_sector.trim_end_matches("_macro");
    let sector_num: u32 = sector_part.parse().ok()?;
    let text_id = cluster_num * 10000 + sector_num * 10 + 1;
    Some((20004, text_id))
}

/// Fallback: convert `Cluster_01_Sector001_macro` → `"Cluster 01 Sector001"`.
fn macro_to_display_name(s: &str) -> String {
    s.trim_end_matches("_macro").replace('_', " ")
}

/// Resolve a translation lookup into a clean display name.
/// Delegates to `x4_display_name` which handles ref substitution (recursive),
/// leading-paren display extraction, trailing-paren hint stripping, and
/// backslash-paren unescaping in one unified pass.
fn resolve_display_name(raw: &str, translations: &HashMap<(u32, u32), String>) -> String {
    map_domain::translations::x4_display_name(raw, translations)
}

/// Resolve a static object's display name from its macro's `<identification>`
/// ref (e.g. aqueducts, anomalies, landmarks) via `macro_identifications` +
/// translations. Falls back to `fallback` (the macro-derived string) when the
/// macro has no identification or the ref doesn't resolve cleanly.
fn resolve_static_object_name(
    macro_ref: &str,
    macro_identifications: &HashMap<String, String>,
    translations: &HashMap<(u32, u32), String>,
    fallback: &str,
) -> String {
    if let Some(id_ref) = macro_identifications.get(&macro_ref.to_lowercase()) {
        let display = resolve_display_name(id_ref, translations);
        if !display.is_empty() && !display.starts_with('{') {
            return display;
        }
    }
    fallback.to_string()
}

/// Parse a combined single-file galaxy XML (fixture format used in tests).
///
/// All macros (`class="galaxy"`, `class="cluster"`, `class="sector"`) are in one
/// flat `<macros>` block.  Sector display names and faction come from
/// `<identification name="..."/>` and `<owner exact="..."/>` in sector macros.
pub fn parse_galaxy(path: &Path) -> Result<Universe, ParseError> {
    let xml_str = std::fs::read_to_string(path)?;
    parse_galaxy_str(&xml_str)
}

fn parse_galaxy_str(xml_str: &str) -> Result<Universe, ParseError> {
    let mut xml = Reader::from_str(xml_str);
    xml.config_mut().trim_text(true);

    // cluster_macro_name → (x, z)
    let mut cluster_positions: HashMap<String, (f32, f32)> = HashMap::new();
    // sector_macro_name → cluster_macro_name
    let mut sector_to_cluster: HashMap<String, String> = HashMap::new();
    // sector_macro_name → (display_name, Option<faction>)
    let mut sector_props: HashMap<String, (String, Option<String>)> = HashMap::new();

    #[derive(Debug, Clone, PartialEq)]
    enum MacroCtx {
        None,
        Galaxy,
        Cluster(String),
        Sector(String),
        Other,
    }

    let mut ctx = MacroCtx::None;
    let mut in_galaxy_conn = false;
    let mut gconn_cluster_ref: Option<String> = None;
    let mut gconn_position: Option<(f32, f32)> = None;
    let mut in_offset = false;
    let mut buf = Vec::new();

    loop {
        let ev = xml.read_event_into(&mut buf)?;
        match ev {
            Event::Eof => break,

            Event::Start(ref e) => {
                let tag = e.name();
                match tag.as_ref() {
                    b"macro" => match attr_value(e, b"class").as_deref() {
                        Some("galaxy") => ctx = MacroCtx::Galaxy,
                        Some("cluster") => {
                            ctx = MacroCtx::Cluster(attr_value(e, b"name").unwrap_or_default())
                        }
                        Some("sector") => {
                            ctx = MacroCtx::Sector(attr_value(e, b"name").unwrap_or_default())
                        }
                        Some(_) => ctx = MacroCtx::Other,
                        None => {}
                    },
                    b"connection" => {
                        if ctx == MacroCtx::Galaxy {
                            in_galaxy_conn = true;
                            gconn_cluster_ref = None;
                            gconn_position = None;
                        }
                    }
                    b"offset" => {
                        in_offset = true;
                    }
                    b"identification" => {
                        if let MacroCtx::Sector(ref mname) = ctx {
                            if let Some(name) = attr_value(e, b"name") {
                                sector_props
                                    .entry(mname.clone())
                                    .or_insert_with(|| (String::new(), None))
                                    .0 = name;
                            }
                        }
                    }
                    b"owner" => {
                        if let MacroCtx::Sector(ref mname) = ctx {
                            if let Some(faction) = attr_value(e, b"exact") {
                                sector_props
                                    .entry(mname.clone())
                                    .or_insert_with(|| (String::new(), None))
                                    .1 = Some(faction);
                            }
                        }
                    }
                    _ => {}
                }
            }

            Event::Empty(ref e) => {
                let tag = e.name();
                match tag.as_ref() {
                    b"macro" => {
                        let conn = attr_value(e, b"connection");
                        let mref = attr_value(e, b"ref");
                        match conn.as_deref() {
                            Some("cluster") if in_galaxy_conn => {
                                gconn_cluster_ref = mref;
                            }
                            Some("sector") => {
                                if let MacroCtx::Cluster(ref cname) = ctx {
                                    if let Some(sref) = mref {
                                        sector_to_cluster.insert(sref, cname.clone());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    b"position" if in_offset && in_galaxy_conn => {
                        let x: f32 = attr_value(e, b"x")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        let z: f32 = attr_value(e, b"z")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        gconn_position = Some((x, z));
                    }
                    b"identification" => {
                        if let MacroCtx::Sector(ref mname) = ctx {
                            if let Some(name) = attr_value(e, b"name") {
                                sector_props
                                    .entry(mname.clone())
                                    .or_insert_with(|| (String::new(), None))
                                    .0 = name;
                            }
                        }
                    }
                    b"owner" => {
                        if let MacroCtx::Sector(ref mname) = ctx {
                            if let Some(faction) = attr_value(e, b"exact") {
                                sector_props
                                    .entry(mname.clone())
                                    .or_insert_with(|| (String::new(), None))
                                    .1 = Some(faction);
                            }
                        }
                    }
                    _ => {}
                }
            }

            Event::End(ref e) => {
                let tag = e.name();
                match tag.as_ref() {
                    b"offset" => {
                        in_offset = false;
                    }
                    b"connection" if in_galaxy_conn && ctx == MacroCtx::Galaxy => {
                        if let (Some(cref), Some(pos)) =
                            (gconn_cluster_ref.take(), gconn_position.take())
                        {
                            cluster_positions.insert(cref, pos);
                        }
                        in_galaxy_conn = false;
                    }
                    b"macro" => {
                        ctx = MacroCtx::None;
                        in_galaxy_conn = false;
                    }
                    _ => {}
                }
            }

            _ => {}
        }
        buf.clear();
    }

    let mut sectors = Vec::new();
    let mut id_counter = 0u32;
    let mut faction_ids: HashMap<String, FactionId> = HashMap::new();
    let mut next_faction_id: u32 = 0;

    for (sector_macro, cluster_name) in &sector_to_cluster {
        let (name, faction_str) = sector_props.get(sector_macro).cloned().unwrap_or_default();

        let (x, z) = cluster_positions
            .get(cluster_name)
            .copied()
            .unwrap_or((0.0, 0.0));

        let map_position = Vec2::new(x / 1_000_000.0, z / 1_000_000.0);

        id_counter += 1;
        let faction = faction_str.map(|name| {
            *faction_ids.entry(name).or_insert_with(|| {
                next_faction_id += 1;
                FactionId(next_faction_id)
            })
        });

        sectors.push(Sector {
            id: SectorId(id_counter),
            name,
            faction,
            map_position,
            static_objects: vec![],
            cluster_id: None,
            index_in_cluster: 0,
            cluster_sector_count: 1,
        });
    }

    Ok(Universe {
        sectors,
        clusters: vec![],
        connections: vec![],
        sector_macros: HashMap::new(),
        faction_strings: HashMap::new(),
        faction_table: HashMap::new(),
        ware_names: HashMap::new(),
        ware_factory_names: HashMap::new(),
        prod_module_wares: HashMap::new(),
        job_names: HashMap::new(),
        translations: HashMap::new(),
        available_locales: Vec::new(),
        current_locale: 44,
        macro_identifications: HashMap::new(),
        zone_positions: HashMap::new(),
        sector_resources: HashMap::new(),
    })
}

/// Parse a sector XML file and return all static objects in the zone connections.
pub fn parse_sector_objects(path: &Path) -> Result<Vec<StaticObject>, ParseError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);

    let mut objects: Vec<StaticObject> = Vec::new();
    let mut id_counter = 0u32;
    let mut faction_ids: HashMap<String, FactionId> = HashMap::new();
    let mut next_faction_id: u32 = 0;

    let mut in_zone_macro = false;
    let mut in_object_conn = false;
    let mut in_offset = false;

    let mut pending_pos: Option<Vec3> = None;
    let mut pending_name: Option<String> = None;
    let mut pending_faction: Option<String> = None;
    let mut pending_kind: Option<StaticObjectKind> = None;

    let mut buf = Vec::new();

    loop {
        let ev = xml.read_event_into(&mut buf)?;
        match ev {
            Event::Eof => break,

            Event::Start(ref e) => {
                let tag = e.name();
                match tag.as_ref() {
                    b"macro" => {
                        if attr_value(e, b"class").as_deref() == Some("zone") {
                            in_zone_macro = true;
                        }
                    }
                    b"connection" if in_zone_macro => {
                        if attr_value(e, b"ref").as_deref() == Some("object") {
                            in_object_conn = true;
                            pending_pos = None;
                            pending_name = None;
                            pending_faction = None;
                            pending_kind = None;
                        }
                    }
                    b"offset" if in_object_conn => {
                        in_offset = true;
                    }
                    _ => {}
                }
            }

            Event::Empty(ref e) => {
                let tag = e.name();
                match tag.as_ref() {
                    b"position" if in_offset && in_object_conn => {
                        let x: f32 = attr_value(e, b"x")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        let y: f32 = attr_value(e, b"y")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        let z: f32 = attr_value(e, b"z")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        pending_pos = Some(Vec3::new(x, y, z));
                    }
                    b"identification" if in_object_conn => {
                        pending_name = attr_value(e, b"name");
                    }
                    b"owner" if in_object_conn => {
                        pending_faction = attr_value(e, b"exact");
                    }
                    b"type" if in_object_conn => {
                        pending_kind = attr_value(e, b"class").map(|c| match c.as_str() {
                            "station" => StaticObjectKind::Station,
                            "gate" => StaticObjectKind::Gate,
                            "resourcezone" => StaticObjectKind::ResourceZone,
                            _ => StaticObjectKind::Anomaly,
                        });
                    }
                    _ => {}
                }
            }

            Event::End(ref e) => {
                let tag = e.name();
                match tag.as_ref() {
                    b"offset" => {
                        in_offset = false;
                    }
                    b"connection" if in_object_conn => {
                        if let (Some(pos), Some(name), Some(kind)) =
                            (pending_pos.take(), pending_name.take(), pending_kind.take())
                        {
                            id_counter += 1;
                            let faction = pending_faction.take().map(|name| {
                                *faction_ids.entry(name).or_insert_with(|| {
                                    next_faction_id += 1;
                                    FactionId(next_faction_id)
                                })
                            });
                            objects.push(StaticObject {
                                id: ObjectId(id_counter),
                                kind,
                                position: pos,
                                faction,
                                name,
                                rotation: None,
                                details: vec![],
                            });
                        }
                        in_object_conn = false;
                    }
                    b"macro" if in_zone_macro => {
                        in_zone_macro = false;
                    }
                    _ => {}
                }
            }

            _ => {}
        }
        buf.clear();
    }

    Ok(objects)
}

/// Parse `libraries/wares.xml`. Returns `ware_id (lowercase) → display name`.
///
/// `name="{page,id}"` resolved via `translations`; literal names returned as-is;
/// unknown translation keys fall back to the raw ware id so the UI is never empty.
pub fn parse_ware_names_xml(
    xml: &[u8],
    translations: &HashMap<(u32, u32), String>,
) -> HashMap<String, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut out: HashMap<String, String> = HashMap::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) if e.name().as_ref() == b"ware" => {
                let Some(id) = attr_value(e, b"id") else {
                    continue;
                };
                let id_lc = id.to_lowercase();
                let name_attr = attr_value(e, b"name").unwrap_or_default();
                let display = if let Some((pid, tid)) = parse_page_text_ref(&name_attr) {
                    translations
                        .get(&(pid, tid))
                        .cloned()
                        .unwrap_or_else(|| id_lc.clone())
                } else if !name_attr.is_empty() {
                    name_attr
                } else {
                    id_lc.clone()
                };
                out.insert(id_lc, display);
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

/// Parse `libraries/wares.xml` `factoryname=` refs into `ware_id → display name`
/// (e.g. "energycells" → "Solar Power Plant"). This is the name the game shows
/// for an NPC factory producing the ware, distinct from the ware's own name.
/// Wares without a `factoryname` are skipped. Unresolved refs are dropped (the
/// caller falls back to other naming paths) rather than echoing the raw ware id.
pub fn parse_ware_factory_names_xml(
    xml: &[u8],
    translations: &HashMap<(u32, u32), String>,
) -> HashMap<String, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut out: HashMap<String, String> = HashMap::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) if e.name().as_ref() == b"ware" => {
                let Some(id) = attr_value(e, b"id") else {
                    continue;
                };
                let Some(fname) = attr_value(e, b"factoryname") else {
                    continue;
                };
                let display = if let Some((pid, tid)) = parse_page_text_ref(&fname) {
                    match translations.get(&(pid, tid)) {
                        Some(raw) => resolve_display_name(raw, translations),
                        None => continue,
                    }
                } else if !fname.is_empty() {
                    fname
                } else {
                    continue;
                };
                out.insert(id.to_lowercase(), display);
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

/// Parse the ware id produced by a production-module macro file, from its
/// `<production wares="..."/>` element (e.g. `prod_gen_energycells_macro` →
/// "energycells"). Returns the first listed ware (modules produce one ware in
/// practice). `None` if the macro defines no production.
pub fn parse_macro_production_ware(xml: &[u8]) -> Option<String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e))
                if e.name().as_ref() == b"production" =>
            {
                if let Some(wares) = attr_value(e, b"wares") {
                    let first = wares.split([',', ' ']).find(|s| !s.is_empty());
                    return first.map(|s| s.to_lowercase());
                }
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }
}

/// Parse `libraries/jobs.xml` into `job_id → display name` (e.g.
/// "argon_construction_vessel_xl_focused" → "Builder Ship"). NPC ships are
/// prefixed in-game with their job's name. Jobs without a resolvable name are
/// skipped.
pub fn parse_jobs_xml(
    xml: &[u8],
    translations: &HashMap<(u32, u32), String>,
) -> HashMap<String, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut out: HashMap<String, String> = HashMap::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) if e.name().as_ref() == b"job" => {
                let Some(id) = attr_value(e, b"id") else {
                    continue;
                };
                let Some(name_attr) = attr_value(e, b"name") else {
                    continue;
                };
                let display = if let Some((pid, tid)) = parse_page_text_ref(&name_attr) {
                    match translations.get(&(pid, tid)) {
                        Some(raw) => resolve_display_name(raw, translations),
                        None => continue,
                    }
                } else if !name_attr.is_empty() {
                    name_attr
                } else {
                    continue;
                };
                out.insert(id.to_lowercase(), display);
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[test]
    fn parse_resource_areas_extracts_ware_tier_amount() {
        use map_domain::resources::ResourceTier;
        let xml = r#"<?xml version="1.0"?>
<defaults>
  <dataset macro="Cluster_01_Sector001_macro">
    <properties>
      <resourceareas>
        <resourcearea amount="4" ref="sphere_large_ore_high_slow" />
        <resourcearea amount="3" ref="sphere_tiny_ore_low_fast" />
        <resourcearea amount="1" ref="sphere_small_nividium_verylow_slow" />
        <resourcearea amount="2" ref="garbage_ref" />
      </resourceareas>
    </properties>
  </dataset>
</defaults>"#;
        let map = super::parse_resource_areas_xml(xml);
        let areas = &map["cluster_01_sector001_macro"];
        // 3 valid areas; the malformed ref is skipped.
        assert_eq!(areas.len(), 3);
        assert!(areas.contains(&map_domain::resources::SectorResource {
            ware: "ore".into(),
            tier: ResourceTier::High,
            amount: 4,
        }));
        assert!(areas.contains(&map_domain::resources::SectorResource {
            ware: "nividium".into(),
            tier: ResourceTier::VeryLow,
            amount: 1,
        }));
    }

    #[test]
    fn parse_translations_xml_includes_wares_page_20201() {
        let xml = r#"<?xml version="1.0"?>
<language id="44">
  <page id="20201">
    <t id="1101">Energy Cells</t>
    <t id="1102">Medical Supplies</t>
  </page>
</language>"#;
        let map = super::parse_translations_xml(xml).unwrap();
        assert_eq!(
            map.get(&(20201, 1101)).map(String::as_str),
            Some("Energy Cells")
        );
        assert_eq!(
            map.get(&(20201, 1102)).map(String::as_str),
            Some("Medical Supplies")
        );
    }

    #[test]
    fn parse_translations_xml_skips_empty_ware_entry() {
        let xml = r#"<?xml version="1.0"?>
<language id="44">
  <page id="20201">
    <t id="9999"></t>
  </page>
</language>"#;
        let map = super::parse_translations_xml(xml).unwrap();
        assert!(map.get(&(20201, 9999)).is_none());
    }

    #[test]
    fn parse_ware_names_resolves_translations_literals_and_falls_back() {
        let xml = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wares_mini.xml"),
        )
        .unwrap();
        let mut translations: HashMap<(u32, u32), String> = HashMap::new();
        translations.insert((20201, 1101), "Energy Cells".into());
        translations.insert((20201, 1102), "Medical Supplies".into());

        let names = super::parse_ware_names_xml(&xml, &translations);

        assert_eq!(
            names.get("energycells").map(String::as_str),
            Some("Energy Cells")
        );
        assert_eq!(
            names.get("medicalsupplies").map(String::as_str),
            Some("Medical Supplies")
        );
        assert_eq!(
            names.get("literalwell").map(String::as_str),
            Some("Hand-Written Name")
        );
        // Unknown translation key falls back to raw ware id.
        assert_eq!(
            names.get("unknownware").map(String::as_str),
            Some("unknownware")
        );
        // <missingid> has no `id` attribute — skipped.
        assert!(names.get("missingid").is_none());
    }

    #[test]
    fn parse_ware_factory_names_resolves_factoryname_ref() {
        let xml = br#"<wares>
            <ware id="energycells" name="{20201,701}" factoryname="{20201,704}"/>
            <ware id="hullparts" name="{20201,1201}" factoryname="{20201,1204}"/>
            <ware id="nofactory" name="{20201,9001}"/>
        </wares>"#;
        let mut t: HashMap<(u32, u32), String> = HashMap::new();
        t.insert((20201, 704), "Solar Power Plant".into());
        t.insert((20201, 1204), "Hull Parts Factory".into());
        let m = super::parse_ware_factory_names_xml(xml, &t);
        assert_eq!(
            m.get("energycells").map(String::as_str),
            Some("Solar Power Plant")
        );
        assert_eq!(
            m.get("hullparts").map(String::as_str),
            Some("Hull Parts Factory")
        );
        // No factoryname attr → absent (not echoed as ware id).
        assert!(m.get("nofactory").is_none());
    }

    #[test]
    fn parse_macro_production_ware_extracts_first_ware() {
        let xml = br#"<macros><macro name="prod_gen_energycells_macro" class="production">
            <properties><identification name="{20104,10801}"/></properties>
            <production wares="energycells"><queue ware="energycells"/></production>
        </macro></macros>"#;
        assert_eq!(
            super::parse_macro_production_ware(xml).as_deref(),
            Some("energycells")
        );
        // Non-production macro → None.
        let no_prod = br#"<macros><macro name="x" class="ship_s"><properties/></macro></macros>"#;
        assert_eq!(super::parse_macro_production_ware(no_prod), None);
    }

    #[test]
    fn parse_jobs_resolves_job_name_ref() {
        let xml = br#"<jobs>
            <job id="argon_construction_vessel_xl_focused" name="{20204,5101}"/>
            <job id="noname"/>
        </jobs>"#;
        let mut t: HashMap<(u32, u32), String> = HashMap::new();
        t.insert((20204, 5101), "Builder Ship".into());
        let m = super::parse_jobs_xml(xml, &t);
        assert_eq!(
            m.get("argon_construction_vessel_xl_focused")
                .map(String::as_str),
            Some("Builder Ship")
        );
        assert!(m.get("noname").is_none());
    }

    #[test]
    fn parse_translations_xml_retains_all_pages_with_correct_branch() {
        let xml = r#"<?xml version="1.0"?>
<language id="44">
  <page id="20003">
    <t id="1">{20003,2} {20004,1}(Argon Prime Cluster)</t>
  </page>
  <page id="20004">
    <t id="10011">{20004,10012} {20004,10013}(Argon Prime)</t>
  </page>
  <page id="20101">
    <t id="122701">Cerberus Vanguard</t>
  </page>
  <page id="10000">
    <t id="500">Some Lore Snippet</t>
  </page>
</language>"#;
        let map = super::parse_translations_xml(xml).unwrap();
        // All pages now store PLAIN TEXT (composite refs preserved for runtime
        // resolution). The display layer applies x4_display_name which handles
        // ref substitution and trailing/leading paren extraction in one pass.
        assert_eq!(
            map.get(&(20003, 1)).map(String::as_str),
            Some("{20003,2} {20004,1}(Argon Prime Cluster)")
        );
        assert_eq!(
            map.get(&(20004, 10011)).map(String::as_str),
            Some("{20004,10012} {20004,10013}(Argon Prime)")
        );
        assert_eq!(
            map.get(&(20101, 122701)).map(String::as_str),
            Some("Cerberus Vanguard")
        );
        assert_eq!(
            map.get(&(10000, 500)).map(String::as_str),
            Some("Some Lore Snippet")
        );
    }
}
