//! X4 save-game parser.
//!
//! Reads `~/.config/EgoSoft/X4/<id>/save/{quicksave,save_NNN}.xml.gz`. Files are
//! gzip-compressed XML, ~30 MB compressed / ~300 MB uncompressed. We stream via
//! `quick_xml` over a `flate2::read::GzDecoder` to avoid loading the whole DOM.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use flate2::read::GzDecoder;
use glam::Vec3;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use map_domain::ids::{FactionId, SectorId};
use map_domain::world::{EntityId, LiveObjectKind, SnapshotMeta, World};

use crate::xml_parser::ParseError;

/// Map of sector_macro (lowercase) → faction string from the save's per-sector
/// `<component class="sector" owner="..."/>` attribute. Caller resolves to FactionId.
pub type FactionOverrides = HashMap<String, String>;

/// Pending entity state collected during the universe walk. Once its position
/// is captured and its closing tag observed, we commit it to the World.
struct PendingEntity {
    id: EntityId,
    name: String,
    kind: LiveObjectKind,
    faction: Option<FactionId>,
    sector: SectorId,
    /// Component-stack depth at which this entity's opening `<component>` lives.
    /// Used to detect the matching End event vs. nested components inside it.
    open_depth: u32,
    /// First position captured for this entity (from its own `<offset><position/>`).
    /// Subsequent positions (from nested zones/modules) are ignored.
    position: Option<Vec3>,
}

/// Parse an X4 save file. Returns snapshot metadata, a `World` of live ships +
/// stations, and per-sector faction overrides.
///
/// `sector_macro_to_id` resolves sector macro strings to `SectorId`s. If `None`,
/// entities are skipped (used by tests that only need meta / overrides).
pub fn parse_save(
    path: &Path,
    sector_macro_to_id: Option<&HashMap<String, SectorId>>,
) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError> {
    let file = File::open(path)?;
    let mtime = file
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .unwrap_or(std::time::UNIX_EPOCH);
    let gz = GzDecoder::new(file);
    let mut reader = Reader::from_reader(BufReader::new(gz));
    reader.config_mut().trim_text(true);

    let mut meta = SnapshotMeta {
        path: path.to_path_buf(),
        mtime,
        game_time_seconds: 0.0,
        player_money: 0,
        player_location_name: String::new(),
        game_version: String::new(),
    };

    let mut world = World::new();
    let mut overrides = FactionOverrides::new();

    let mut buf = Vec::new();
    let mut info_done = false;

    while !info_done {
        match reader.read_event_into(&mut buf)? {
            Event::Empty(ref e) => match e.name().as_ref() {
                b"game" => {
                    if let Some(t) = attr_value(e, b"time") {
                        meta.game_time_seconds = t.parse().unwrap_or(0.0);
                    }
                    let ver = attr_value(e, b"version").unwrap_or_default();
                    let build = attr_value(e, b"build").unwrap_or_default();
                    meta.game_version = match (ver.is_empty(), build.is_empty()) {
                        (false, false) => format!("{}.{}", ver, build),
                        (false, true)  => ver,
                        (true,  false) => build,
                        _              => String::new(),
                    };
                }
                b"player" => {
                    if let Some(m) = attr_value(e, b"money") {
                        meta.player_money = m.parse().unwrap_or(0);
                    }
                    if let Some(loc) = attr_value(e, b"location") {
                        meta.player_location_name = loc;
                    }
                }
                _ => {}
            },
            Event::End(ref e) if e.name().as_ref() == b"info" => info_done = true,
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    // --- Universe tree walk ---------------------------------------------------
    let mut comp_depth: u32 = 0;
    let mut current_sector_macro: Option<String> = None;
    let mut sector_open_depth: Option<u32> = None;
    let mut pending: Option<PendingEntity> = None;
    let mut in_offset = false;
    let mut faction_name_to_id: HashMap<String, FactionId> = HashMap::new();
    let mut next_faction_id: u32 = 1;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(ref e) if e.name().as_ref() == b"component" => {
                comp_depth += 1;
                let class = attr_value(e, b"class").unwrap_or_default();
                match class.as_str() {
                    "sector" => {
                        if let Some(macro_name) = attr_value(e, b"macro") {
                            let key = macro_name.to_lowercase();
                            if let Some(owner) = attr_value(e, b"owner") {
                                overrides.insert(key.clone(), owner);
                            }
                            current_sector_macro = Some(key);
                            sector_open_depth = Some(comp_depth);
                        }
                    }
                    "station" | "ship_xs" | "ship_s" | "ship_m" | "ship_l" | "ship_xl"
                        if pending.is_none() =>
                    {
                        if let Some(p) = build_pending_entity(
                            e,
                            &class,
                            comp_depth,
                            current_sector_macro.as_deref(),
                            sector_macro_to_id,
                            &mut faction_name_to_id,
                            &mut next_faction_id,
                        ) {
                            pending = Some(p);
                        }
                    }
                    _ => {}
                }
            }
            Event::End(ref e) if e.name().as_ref() == b"component" => {
                // Commit pending entity if its component is closing.
                if let Some(p) = pending.as_ref() {
                    if p.open_depth == comp_depth {
                        let p = pending.take().unwrap();
                        let pos = p.position.unwrap_or(Vec3::ZERO);
                        world.insert_entity(p.id, p.name, p.kind, p.faction, pos, p.sector);
                    }
                }
                // Clear sector tracking if its component is closing.
                if let Some(d) = sector_open_depth {
                    if d == comp_depth {
                        sector_open_depth = None;
                        current_sector_macro = None;
                    }
                }
                comp_depth = comp_depth.saturating_sub(1);
            }
            Event::Start(ref e) if e.name().as_ref() == b"offset" => {
                in_offset = true;
            }
            Event::End(ref e) if e.name().as_ref() == b"offset" => {
                in_offset = false;
            }
            Event::Empty(ref e) if e.name().as_ref() == b"position" => {
                if in_offset {
                    if let Some(p) = pending.as_mut() {
                        if p.position.is_none() {
                            let x = attr_value(e, b"x")
                                .and_then(|s| s.parse::<f32>().ok())
                                .unwrap_or(0.0);
                            let y = attr_value(e, b"y")
                                .and_then(|s| s.parse::<f32>().ok())
                                .unwrap_or(0.0);
                            let z = attr_value(e, b"z")
                                .and_then(|s| s.parse::<f32>().ok())
                                .unwrap_or(0.0);
                            // X4 stores positions in metres; convert to km to match static-object scale.
                            p.position = Some(Vec3::new(x / 1000.0, y / 1000.0, z / 1000.0));
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok((meta, world, overrides))
}

/// Construct a pending entity from a `<component>` Start event. Returns `None`
/// if any required attribute is missing/malformed, or if the sector cannot be
/// resolved (no map provided, or unknown macro).
fn build_pending_entity(
    e: &BytesStart,
    class: &str,
    comp_depth: u32,
    current_sector_macro: Option<&str>,
    sector_macro_to_id: Option<&HashMap<String, SectorId>>,
    faction_name_to_id: &mut HashMap<String, FactionId>,
    next_faction_id: &mut u32,
) -> Option<PendingEntity> {
    let id_str = attr_value(e, b"id")?;
    let id = parse_entity_id(&id_str)?;
    let name = attr_value(e, b"macro").unwrap_or_default();
    let kind = class_to_kind(class)?;

    let faction = attr_value(e, b"owner").map(|owner| {
        *faction_name_to_id.entry(owner).or_insert_with(|| {
            let id = FactionId(*next_faction_id);
            *next_faction_id += 1;
            id
        })
    });

    let sector_macro = current_sector_macro?;
    let map = sector_macro_to_id?;
    let sector = *map.get(&sector_macro.to_lowercase())?;

    Some(PendingEntity {
        id,
        name,
        kind,
        faction,
        sector,
        open_depth: comp_depth,
        position: None,
    })
}

fn class_to_kind(class: &str) -> Option<LiveObjectKind> {
    match class {
        "station" => Some(LiveObjectKind::Station),
        "ship_xs" | "ship_s" => Some(LiveObjectKind::ShipSmall),
        "ship_m" => Some(LiveObjectKind::ShipMedium),
        "ship_l" => Some(LiveObjectKind::ShipLarge),
        "ship_xl" => Some(LiveObjectKind::ShipExtraLarge),
        _ => None,
    }
}

/// Parse an entity id of the form `[0xHEX]` into a u32. Returns `None` on any
/// malformed input — caller skips the entity rather than crashing.
fn parse_entity_id(s: &str) -> Option<EntityId> {
    let inner = s.strip_prefix("[0x")?.strip_suffix(']')?;
    u32::from_str_radix(inner, 16).ok()
}

fn attr_value(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(Result::ok)
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_domain::ids::SectorId;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini_save.xml.gz")
    }

    #[test]
    fn parse_mini_save_meta() {
        let (meta, _, _) = parse_save(&fixture_path(), None).expect("parse");
        assert_eq!(meta.player_money, 40000);
        assert!((meta.game_time_seconds - 1734.285).abs() < 1e-2);
        assert_eq!(meta.player_location_name, "{20004,10011}");
    }

    #[test]
    fn parse_mini_save_entities() {
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("cluster_01_sector001_macro".into(), SectorId(1));
        sm.insert("cluster_06_sector001_macro".into(), SectorId(2));
        let (_, world, _) = parse_save(&fixture_path(), Some(&sm)).unwrap();
        // 1 station + 1 large ship + 1 small ship + 1 medium ship = 4 entities
        assert_eq!(world.names.len(), 4);
        // 2 in sector 1, 2 in sector 2
        assert_eq!(world.entities_in_sector(SectorId(1)).len(), 2);
        assert_eq!(world.entities_in_sector(SectorId(2)).len(), 2);
    }

    #[test]
    fn parse_mini_save_faction_overrides() {
        let (_, _, overrides) = parse_save(&fixture_path(), None).unwrap();
        assert_eq!(overrides.len(), 2);
        assert_eq!(
            overrides.get("cluster_01_sector001_macro").map(|s| s.as_str()),
            Some("argon")
        );
        assert_eq!(
            overrides.get("cluster_06_sector001_macro").map(|s| s.as_str()),
            Some("teladi")
        );
    }
}
