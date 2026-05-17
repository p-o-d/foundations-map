//! Stage 3: parse one sector subtree using `quick_xml`.
//!
//! Runs inside a rayon worker; called once per `SectorChunk`. Returns
//! `Vec<EntityRecord>` with ships and stations. No shared state.

use glam::Vec3;
use map_domain::world::LiveObjectKind;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::types::EntityRecord;

/// Parse one sector subtree. Failures inside a single chunk are swallowed
/// (return whatever was parsed before the error) — Stage 2 already validated
/// the chunk boundaries, and a single bad sector should not abort the whole
/// parse.
pub fn parse_sector_chunk(slice: &[u8], sector_macro: &str) -> Vec<EntityRecord> {
    let mut reader = Reader::from_reader(slice);
    reader.config_mut().trim_text(true);

    let mut out: Vec<EntityRecord> = Vec::new();
    let mut buf: Vec<u8> = Vec::new();

    let mut comp_depth: u32 = 0;
    let mut pending: Option<Pending> = None;
    let mut entity_position_captured = false;
    let mut in_offset = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"component" => {
                comp_depth += 1;
                if pending.is_none() {
                    if let Some(p) = build_pending(e, comp_depth, sector_macro) {
                        pending = Some(p);
                        entity_position_captured = false;
                    }
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"component" => {
                if let Some(p) = pending.as_ref() {
                    if p.open_depth == comp_depth {
                        let p = pending.take().unwrap();
                        out.push(EntityRecord {
                            id: p.id,
                            name: p.name,
                            kind: p.kind,
                            owner: p.owner,
                            position: p.position.unwrap_or(Vec3::ZERO),
                            sector_macro: sector_macro.to_string(),
                        });
                        entity_position_captured = false;
                    }
                }
                comp_depth = comp_depth.saturating_sub(1);
            }
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"offset" => {
                if pending.is_some() && !entity_position_captured {
                    in_offset = true;
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"offset" => in_offset = false,
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"position" => {
                if in_offset {
                    if let Some(p) = pending.as_mut() {
                        if p.position.is_none() {
                            let x = attr_f32(e, b"x").unwrap_or(0.0);
                            let y = attr_f32(e, b"y").unwrap_or(0.0);
                            let z = attr_f32(e, b"z").unwrap_or(0.0);
                            // X4 stores positions in metres; convert to km.
                            p.position = Some(Vec3::new(x / 1000.0, y / 1000.0, z / 1000.0));
                            entity_position_captured = true;
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    out
}

struct Pending {
    open_depth: u32,
    id: u32,
    name: String,
    kind: LiveObjectKind,
    owner: Option<String>,
    position: Option<Vec3>,
}

fn build_pending(e: &BytesStart<'_>, depth: u32, _sector_macro: &str) -> Option<Pending> {
    let class = attr_str(e, b"class")?;
    let kind = match class.as_str() {
        "station" => LiveObjectKind::Station,
        "ship_xs" | "ship_s" => LiveObjectKind::ShipSmall,
        "ship_m" => LiveObjectKind::ShipMedium,
        "ship_l" => LiveObjectKind::ShipLarge,
        "ship_xl" => LiveObjectKind::ShipExtraLarge,
        _ => return None,
    };
    let id_str = attr_str(e, b"id")?;
    let id = parse_entity_id(&id_str)?;
    let name = attr_str(e, b"macro").unwrap_or_else(|| class.clone());
    let owner = attr_str(e, b"owner");

    Some(Pending {
        open_depth: depth,
        id,
        name,
        kind,
        owner,
        position: None,
    })
}

fn parse_entity_id(s: &str) -> Option<u32> {
    let inner = s.strip_prefix("[0x")?.strip_suffix(']')?;
    u32::from_str_radix(inner, 16).ok()
}

fn attr_str(e: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(Result::ok)
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}

fn attr_f32(e: &BytesStart<'_>, name: &[u8]) -> Option<f32> {
    e.attributes()
        .filter_map(Result::ok)
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| std::str::from_utf8(&a.value).ok()?.parse::<f32>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_station_and_ship_with_positions() {
        let chunk: &[u8] = br#"<component class="sector" macro="ignored">
  <component class="zone">
    <component class="station" macro="station_arg_factory_01" owner="argon" id="[0x100]">
      <offset><position x="0" y="0" z="0"/></offset>
    </component>
    <component class="ship_l" macro="ship_arg_l_destroyer_01" owner="argon" id="[0x101]">
      <offset><position x="1000" y="0" z="2000"/></offset>
    </component>
  </component>
</component>"#;

        let out = parse_sector_chunk(chunk, "macroa");
        assert_eq!(out.len(), 2);

        let station = &out[0];
        assert_eq!(station.id, 0x100);
        assert_eq!(station.kind, map_domain::world::LiveObjectKind::Station);
        assert_eq!(station.owner.as_deref(), Some("argon"));
        assert_eq!(station.sector_macro, "macroa");
        assert!((station.position.x - 0.0).abs() < 1e-3);

        let ship = &out[1];
        assert_eq!(ship.id, 0x101);
        assert_eq!(ship.kind, map_domain::world::LiveObjectKind::ShipLarge);
        assert!((ship.position.x - 1.0).abs() < 1e-3);
        assert!((ship.position.z - 2.0).abs() < 1e-3);
    }

    #[test]
    fn empty_sector_returns_no_entities() {
        let chunk: &[u8] = br#"<component class="sector" macro="m"><component class="zone"></component></component>"#;
        let out = parse_sector_chunk(chunk, "m");
        assert!(out.is_empty());
    }
}
