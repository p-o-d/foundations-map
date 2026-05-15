use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use glam::{Vec2, Vec3};
use quick_xml::events::Event;
use quick_xml::Reader;

use map_domain::ids::{FactionId, ObjectId, SectorId};
use map_domain::objects::{StaticObject, StaticObjectKind};
use map_domain::universe::{Connection, GateType, Sector, Universe};

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
pub fn parse_galaxy_from_game(game_dir: &Path) -> Result<Universe, ParseError> {
    let load = |path: &str| {
        crate::cat_reader::read_game_file(game_dir, path)
            .ok_or_else(|| ParseError::MissingAttribute(format!("{path} not found in cat archives")))
    };

    let galaxy_data = load("maps/xu_ep2_universe/galaxy.xml")?;
    let clusters_data = load("maps/xu_ep2_universe/clusters.xml")?;

    let translations_data = load("t/0001-l044.xml")?;

    let galaxy_str = String::from_utf8_lossy(&galaxy_data);
    let clusters_str = String::from_utf8_lossy(&clusters_data);
    let translations_str = String::from_utf8_lossy(&translations_data);

    // cluster_macro_name → absolute (x, z) in game units (metres)
    let cluster_positions = parse_cluster_positions_xml(&galaxy_str)?;
    // sector_macro_name → (cluster_macro_name, relative_x, relative_z)
    let sector_placements = parse_sector_placements_xml(&clusters_str)?;
    // Merge name refs from main game + all DLC mapdefaults.xml files
    let mut name_refs = HashMap::new();
    for data in crate::cat_reader::read_all_game_files(game_dir, "libraries/mapdefaults.xml") {
        let s = String::from_utf8_lossy(&data);
        name_refs.extend(parse_sector_name_refs_xml(&s)?);
    }
    // (page_id, text_id) → display name string
    let translations = parse_translations_xml(&translations_str)?;

    let zones_data = load("maps/xu_ep2_universe/zones.xml")?;
    let zones_str = String::from_utf8_lossy(&zones_data);

    let mut sectors = Vec::new();
    let mut macro_to_id: HashMap<String, SectorId> = HashMap::new();
    let mut id_counter = 0u32;

    for (sector_macro, (cluster_macro, dx, dz)) in &sector_placements {
        let (cx, cz) = cluster_positions
            .get(cluster_macro)
            .copied()
            .unwrap_or((0.0, 0.0));
        let map_position = Vec2::new((cx + dx) / 1_000_000.0, (cz + dz) / 1_000_000.0);

        // Try mapdefaults first; fall back to derived ID (cluster_num*10000 + sector_num*10 + 1)
        let name = name_refs
            .get(&sector_macro.to_lowercase())
            .and_then(|(pid, tid)| translations.get(&(*pid, *tid)))
            .cloned()
            .or_else(|| {
                let (pid, tid) = derive_sector_text_id(sector_macro)?;
                translations.get(&(pid, tid)).cloned()
            })
            .unwrap_or_else(|| macro_to_display_name(sector_macro));

        id_counter += 1;
        let id = SectorId(id_counter);
        macro_to_id.insert(sector_macro.clone(), id);
        sectors.push(Sector {
            id,
            name,
            faction: None,
            map_position,
            static_objects: vec![],
        });
    }

    let gate_pairs = parse_gate_connections_xml(&zones_str, &sector_placements);
    let connections: Vec<Connection> = gate_pairs
        .into_iter()
        .filter_map(|(a, b)| {
            Some(Connection {
                from: *macro_to_id.get(&a)?,
                to:   *macro_to_id.get(&b)?,
                gate_type: GateType::Standard,
            })
        })
        .collect();

    Ok(Universe { sectors, connections })
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
    let mut in_conn = false;       // inside a <connection ref="clusters"> element
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
fn parse_sector_name_refs_xml(
    xml: &str,
) -> Result<HashMap<String, (u32, u32)>, ParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut name_refs: HashMap<String, (u32, u32)> = HashMap::new();
    let mut current_macro: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,

            Event::Start(ref e) | Event::Empty(ref e) => {
                match e.name().as_ref() {
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
                }
            }

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
/// Reads entries in page 20004 (sector names).  Each `<t id="N">` value has the
/// form `{ref1} {ref2}(Sector Name)` — the last parenthetical is the display name.
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
                    if current_page == Some(20004) {
                        current_text_id = attr_value(e, b"id").and_then(|s| s.parse().ok());
                    }
                }
                _ => {}
            },

            Event::Text(e) => {
                if let (Some(page_id), Some(text_id)) = (current_page, current_text_id) {
                    let decoded = e.decode().unwrap_or_default();
                    let content = quick_xml::escape::unescape(&decoded)
                        .unwrap_or_else(|_| decoded.clone());
                    if let Some(name) = extract_last_parenthetical(&content) {
                        translations.insert((page_id, text_id), name);
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

/// Extract the last parenthetical content from a translation string.
/// `"{20003,10001} {20402,1}(Grand Exchange I)"` → `"Grand Exchange I"`
fn extract_last_parenthetical(s: &str) -> Option<String> {
    let open = s.rfind('(')?;
    let close = s[open..].find(')')?;
    let name = s[open + 1..open + close].trim();
    if name.is_empty() { None } else { Some(name.to_string()) }
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
        cluster_first.entry(cluster.clone()).or_insert_with(|| sector.clone());
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
        gate_map.entry((*from_n, *to_n)).or_default().push(sector.clone());
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

/// `Zone003_Cluster_01_Sector001_macro` → `Cluster_01_Sector001_macro`
fn zone_name_to_sector_macro(name: &str) -> Option<String> {
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

/// `connection_ClusterGate001To004` → `(1, 4)`
fn parse_gate_cluster_nums(name: &str) -> Option<(u32, u32)> {
    let inner = name.strip_prefix("connection_ClusterGate")?;
    let sep = inner.find("To")?;
    let from: u32 = inner[..sep].parse().ok()?;
    let to: u32 = inner[sep + 2..].parse().ok()?;
    Some((from, to))
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
                    b"macro" => {
                        match attr_value(e, b"class").as_deref() {
                            Some("galaxy") => ctx = MacroCtx::Galaxy,
                            Some("cluster") => {
                                ctx = MacroCtx::Cluster(
                                    attr_value(e, b"name").unwrap_or_default(),
                                )
                            }
                            Some("sector") => {
                                ctx = MacroCtx::Sector(
                                    attr_value(e, b"name").unwrap_or_default(),
                                )
                            }
                            Some(_) => ctx = MacroCtx::Other,
                            None => {}
                        }
                    }
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
        let (name, faction_str) = sector_props
            .get(sector_macro)
            .cloned()
            .unwrap_or_default();

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
        });
    }

    Ok(Universe {
        sectors,
        connections: vec![],
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
                        if let (Some(pos), Some(name), Some(kind)) = (
                            pending_pos.take(),
                            pending_name.take(),
                            pending_kind.take(),
                        ) {
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
