use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use glam::{Vec2, Vec3};
use quick_xml::events::Event;
use quick_xml::Reader;

use map_domain::ids::{FactionId, ObjectId, SectorId};
use map_domain::objects::{StaticObject, StaticObjectKind};
use map_domain::universe::{Sector, Universe};

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

/// Parse galaxy.xml and return a Universe with sectors populated.
///
/// Galaxy XML layout (all macros in one flat `<macros>` block):
///
/// - `class="galaxy"` macro: `<connections>` has one `<connection>` per cluster.
///   Each galaxy connection contains:
///   - `<macro ref="Cluster_XX_macro" connection="cluster"/>` — cluster macro ref
///   - `<offset><position x=".." z=".."/></offset>` — cluster map position
///
/// - `class="cluster"` macro: `<connections>` has one `<connection>` per sector.
///   Each cluster connection contains:
///   - `<macro ref="Cluster_XX_SectorYYY_macro" connection="sector"/>` — sector macro ref
///
/// - `class="sector"` macro: `<properties>` has:
///   - `<identification name="Human Readable Name"/>`
///   - `<owner exact="faction_id"/>`
///
/// Join: cluster_macro_name → position, sector_macro_name → cluster_macro_name,
///       sector_macro_name → (display_name, faction).
pub fn parse_galaxy(path: &Path) -> Result<Universe, ParseError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);

    // cluster_macro_name → (x, z)
    let mut cluster_positions: HashMap<String, (f32, f32)> = HashMap::new();
    // sector_macro_name → cluster_macro_name
    let mut sector_to_cluster: HashMap<String, String> = HashMap::new();
    // sector_macro_name → (display_name, Option<faction>)
    let mut sector_props: HashMap<String, (String, Option<String>)> = HashMap::new();

    // Context: which named macro are we inside?
    #[derive(Debug, Clone, PartialEq)]
    enum MacroCtx {
        None,
        Galaxy,
        Cluster(String),
        Sector(String),
        Other,
    }

    let mut ctx = MacroCtx::None;

    // State for current galaxy-level connection being parsed
    let mut in_galaxy_conn = false;
    let mut gconn_cluster_ref: Option<String> = None; // cluster macro ref from inline <macro>
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
                            None => {
                                // inline ref macro — handled in Empty arm; shouldn't appear as Start
                            }
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
                        // Self-closing inline reference macro inside a connection.
                        let conn = attr_value(e, b"connection");
                        let mref = attr_value(e, b"ref");
                        match conn.as_deref() {
                            Some("cluster") if in_galaxy_conn => {
                                // Record which cluster macro this galaxy connection points to.
                                gconn_cluster_ref = mref;
                            }
                            Some("sector") => {
                                // Map sector macro → current cluster macro
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
                        // Commit the (cluster_macro_ref → position) mapping.
                        if let (Some(cref), Some(pos)) =
                            (gconn_cluster_ref.take(), gconn_position.take())
                        {
                            cluster_positions.insert(cref, pos);
                        }
                        in_galaxy_conn = false;
                    }
                    b"macro" => {
                        // Named macros don't nest, so closing any macro resets context.
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

    // Join tables → sectors
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
///
/// Structure: sector macro → zone connection → zone macro → object connections.
/// Each object connection has: offset/position, identification/name, optional owner, type/class.
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
