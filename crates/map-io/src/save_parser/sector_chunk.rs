//! Stage 3: parse one sector subtree using `quick_xml`.
//!
//! Runs inside a rayon worker; called once per `SectorChunk`. Returns
//! `Vec<EntityRecord>` with ships and stations. No shared state.

use glam::Vec3;
use map_domain::world::{LiveObjectKind, TradeOffer};
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
    let mut stack: Vec<Pending> = Vec::new();
    // One frame per *open* component (zone, station, ship, …) regardless of class.
    // Lets us sum the full offset chain and find the enclosing zone for an entity.
    let mut comp_stack: Vec<CompFrame> = Vec::new();
    // Depth at which we are currently inside an <offset> element; None otherwise.
    let mut offset_depth: Option<u32> = None;
    // True while inside an `<offers>` element. `<trade>` elements inside this
    // scope (with a buyer= or seller= attribute) are offers, not in-flight trades.
    let mut in_offers: bool = false;
    // Counters to track <construction><sequence> nesting for prod_* detection.
    let mut in_construction: u32 = 0;
    let mut in_seq_under_construction: u32 = 0;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"component" => {
                comp_depth += 1;
                let zone_macro = if attr_str(e, b"class").as_deref() == Some("zone") {
                    attr_str(e, b"macro").map(|m| m.to_lowercase())
                } else {
                    None
                };
                comp_stack.push(CompFrame {
                    offset: None,
                    zone_macro,
                });
                if let Some(p) = build_pending(e, comp_depth, stack.last().map(|sp| sp.id)) {
                    stack.push(p);
                }
                // A station's wharf/shipyard identity comes from its build
                // modules, which appear as nested `<component>` macros. Attribute
                // the flag to the nearest enclosing station (skip docked ships).
                if let Some(kind) = attr_str(e, b"macro").as_deref().and_then(build_module_kind) {
                    if let Some(station) = stack
                        .iter_mut()
                        .rev()
                        .find(|p| p.kind == LiveObjectKind::Station)
                    {
                        match kind {
                            BuildKind::Wharf => station.is_wharf = true,
                            BuildKind::Shipyard => station.is_shipyard = true,
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"component" => {
                if let Some(top) = stack.last() {
                    if top.open_depth == comp_depth {
                        let mut p = stack.pop().unwrap();
                        // Position relative to the enclosing zone = sum of offsets
                        // from that zone frame down to this entity. We stop at the
                        // zone (do NOT include the sector/cluster offset above it):
                        // the zone's *sector*-relative position is added later from
                        // the static `zone_positions` table. If there is no zone
                        // ancestor, fall back to the entity's own offset only.
                        let zone_idx = comp_stack.iter().rposition(|c| c.zone_macro.is_some());
                        let position = match zone_idx {
                            Some(i) => comp_stack[i..]
                                .iter()
                                .filter_map(|c| c.offset)
                                .fold(Vec3::ZERO, |a, b| a + b),
                            None => comp_stack
                                .last()
                                .and_then(|c| c.offset)
                                .unwrap_or(Vec3::ZERO),
                        };
                        let zone_macro = zone_idx.and_then(|i| comp_stack[i].zone_macro.clone());
                        out.push(EntityRecord {
                            id: p.id,
                            parent_id: p.parent_id,
                            macro_name: p.macro_name,
                            code: p.code,
                            kind: p.kind,
                            owner: p.owner,
                            position,
                            sector_macro: sector_macro.to_string(),
                            zone_macro,
                            trade_offers: std::mem::take(&mut p.trade_offers),
                            display_name_ref: p.display_name_ref.take(),
                            production_module_macro: p.production_module_macro.take(),
                            is_wharf: p.is_wharf,
                            is_shipyard: p.is_shipyard,
                            name_index: p.name_index,
                            job: p.job.take(),
                        });
                    }
                }
                comp_stack.pop();
                comp_depth = comp_depth.saturating_sub(1);
            }
            // A self-closing build-module `<component .../>`: detect only, do not
            // touch the depth/zone stacks (there is no matching End event).
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"component" => {
                if let Some(kind) = attr_str(e, b"macro").as_deref().and_then(build_module_kind) {
                    if let Some(station) = stack
                        .iter_mut()
                        .rev()
                        .find(|p| p.kind == LiveObjectKind::Station)
                    {
                        match kind {
                            BuildKind::Wharf => station.is_wharf = true,
                            BuildKind::Shipyard => station.is_shipyard = true,
                        }
                    }
                }
            }
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"offset" => {
                offset_depth = Some(comp_depth);
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"offset" => {
                offset_depth = None;
            }
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"offers" => {
                in_offers = true;
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"offers" => {
                in_offers = false;
            }
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"construction" => {
                in_construction += 1;
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"construction" => {
                if in_construction > 0 {
                    in_construction -= 1;
                    // Any open sequences from inside this construction are now closed.
                    // (In practice sequences don't outlive their construction.)
                }
            }
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"sequence" => {
                if in_construction > 0 {
                    in_seq_under_construction += 1;
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"sequence" => {
                if in_seq_under_construction > 0 {
                    in_seq_under_construction -= 1;
                }
            }
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e))
                if e.name().as_ref() == b"entry" && in_seq_under_construction > 0 =>
            {
                if let Some(macro_val) = attr_str(e, b"macro") {
                    let macro_lc = macro_val.to_lowercase();
                    if macro_lc.starts_with("prod_") {
                        if let Some(top) = stack.last_mut() {
                            if top.production_module_macro.is_none() {
                                top.production_module_macro = Some(macro_lc);
                            }
                        }
                    }
                }
            }
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"position" => {
                // Attribute this <position> to the component currently being parsed,
                // but only if the enclosing <offset> sits immediately inside it
                // (offset_depth == comp_depth). First position wins — entities may
                // carry several offset blocks (predicted/render); the first is the
                // authoritative one.
                if let (Some(frame), Some(od)) = (comp_stack.last_mut(), offset_depth) {
                    if od == comp_depth && frame.offset.is_none() {
                        let x = attr_f32(e, b"x").unwrap_or(0.0);
                        let y = attr_f32(e, b"y").unwrap_or(0.0);
                        let z = attr_f32(e, b"z").unwrap_or(0.0);
                        // X4 stores positions in metres; convert to km.
                        frame.offset = Some(Vec3::new(x / 1000.0, y / 1000.0, z / 1000.0));
                    }
                }
            }
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e))
                if e.name().as_ref() == b"trade" && in_offers =>
            {
                if let Some(offer) = build_offer(e) {
                    if let Some(top) = stack.last_mut() {
                        top.trade_offers.push(offer);
                    }
                }
            }
            // `<source job="..." class="job"/>`: the NPC job that spawned this
            // ship, attributed to the nearest enclosing entity. The game prefixes
            // generated names with the job's display name ("Builder Ship …").
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) if e.name().as_ref() == b"source" => {
                if let Some(job) = attr_str(e, b"job") {
                    if let Some(top) = stack.last_mut() {
                        if top.job.is_none() {
                            top.job = Some(job.to_lowercase());
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
    parent_id: Option<u32>,
    macro_name: String,
    code: Option<String>,
    kind: LiveObjectKind,
    owner: Option<String>,
    trade_offers: Vec<TradeOffer>,
    display_name_ref: Option<String>,
    production_module_macro: Option<String>,
    is_wharf: bool,
    is_shipyard: bool,
    name_index: Option<u32>,
    job: Option<String>,
}

/// One open `<component>` while parsing. Tracks its own `<offset>` (km) and, if
/// it is a zone, its macro — so an entity can sum the offset chain and find its
/// enclosing zone.
struct CompFrame {
    offset: Option<Vec3>,
    zone_macro: Option<String>,
}

/// A build module that identifies a station as a wharf or shipyard.
#[derive(Clone, Copy)]
enum BuildKind {
    Wharf,
    Shipyard,
}

/// Classify a component macro as a wharf/shipyard build module. Wharfs build S/M
/// ships (`buildmodule_*_ships_s/m_*`), shipyards build L/XL ships and carriers
/// (`buildmodule_*_ships_l/xl_*`, `buildmodule_*_carrier_*`). The Xenon shipyard
/// is also recognised by its station macro `station_xen_shipyard_*`.
fn build_module_kind(macro_lc: &str) -> Option<BuildKind> {
    let m = macro_lc.to_lowercase();
    if m.contains("buildmodule") {
        if m.contains("ships_s") || m.contains("ships_m") {
            return Some(BuildKind::Wharf);
        }
        if m.contains("ships_l") || m.contains("ships_xl") || m.contains("carrier") {
            return Some(BuildKind::Shipyard);
        }
    }
    if m.contains("shipyard") {
        return Some(BuildKind::Shipyard);
    }
    if m.contains("wharf") {
        return Some(BuildKind::Wharf);
    }
    None
}

fn build_pending(e: &BytesStart<'_>, depth: u32, parent_id: Option<u32>) -> Option<Pending> {
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
    let macro_name = attr_str(e, b"macro").unwrap_or_else(|| class.clone());
    let code = attr_str(e, b"code");
    let owner = attr_str(e, b"owner");
    let name_attr = attr_str(e, b"name");
    let basename_attr = attr_str(e, b"basename");
    // Stations: prefer `name=` if present, else `basename=`.
    // Ships:    use `name=`. (Ships do not carry `basename=`.)
    let display_name_ref = name_attr.or(basename_attr);
    // `nameindex` → trailing roman numeral on generated names ("… I"). The
    // `job` that prefixes NPC ship names comes from a nested `<source>`, not an
    // attribute here, so it starts as None and is filled in during the walk.
    let name_index = attr_str(e, b"nameindex").and_then(|s| s.parse().ok());

    // The station's own macro can already mark it (e.g. the Xenon shipyard,
    // whose ship-build capability lives in a single fixed macro).
    let (mut is_wharf, mut is_shipyard) = (false, false);
    if kind == LiveObjectKind::Station {
        match build_module_kind(&macro_name) {
            Some(BuildKind::Wharf) => is_wharf = true,
            Some(BuildKind::Shipyard) => is_shipyard = true,
            None => {}
        }
    }

    Some(Pending {
        open_depth: depth,
        id,
        parent_id,
        macro_name,
        code,
        kind,
        owner,
        trade_offers: Vec::new(),
        display_name_ref,
        production_module_macro: None,
        is_wharf,
        is_shipyard,
        name_index,
        job: None,
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

fn attr_i64(e: &BytesStart<'_>, name: &[u8]) -> Option<i64> {
    e.attributes()
        .filter_map(Result::ok)
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| std::str::from_utf8(&a.value).ok()?.parse::<i64>().ok())
}

fn build_offer(e: &BytesStart<'_>) -> Option<TradeOffer> {
    use map_domain::world::TradeDirection;
    // Real X4 data emits exactly one of buyer= / seller=; if somehow both,
    // buyer wins (we never observed this in saves).
    let direction = if attr_str(e, b"buyer").is_some() {
        TradeDirection::Buy
    } else if attr_str(e, b"seller").is_some() {
        TradeDirection::Sell
    } else {
        return None;
    };
    let ware_id = attr_str(e, b"ware")?.to_lowercase();
    let price = attr_i64(e, b"price").unwrap_or(0);
    let amount = attr_i64(e, b"amount").unwrap_or(0);
    let desired = attr_i64(e, b"desired").unwrap_or(0);
    Some(TradeOffer {
        ware_id,
        direction,
        price,
        amount,
        desired,
    })
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
    fn detects_wharf_and_shipyard_build_modules() {
        // Station A holds a shipyard module (builds L ships); station B a wharf
        // module (builds M ships). A docked ship sits inside A and must not
        // absorb the station's flag.
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="zone">
    <component class="station" macro="station_arg_shipyard" owner="argon" id="[0x100]">
      <offset><position x="0" y="0" z="0"/></offset>
      <component class="buildmodule" macro="buildmodule_gen_ships_l_macro" id="[0x150]"/>
      <component class="ship_s" macro="ship_arg_s_01" owner="argon" id="[0x160]">
        <offset><position x="0" y="0" z="0"/></offset>
      </component>
    </component>
    <component class="station" macro="station_arg_wharf" owner="argon" id="[0x200]">
      <offset><position x="0" y="0" z="0"/></offset>
      <component class="buildmodule" macro="buildmodule_gen_ships_m_dockarea_01_macro" id="[0x250]"/>
    </component>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let a = out.iter().find(|r| r.id == 0x100).unwrap();
        assert!(a.is_shipyard && !a.is_wharf);
        let docked = out.iter().find(|r| r.id == 0x160).unwrap();
        assert!(!docked.is_shipyard && !docked.is_wharf);
        let b = out.iter().find(|r| r.id == 0x200).unwrap();
        assert!(b.is_wharf && !b.is_shipyard);
    }

    #[test]
    fn empty_sector_returns_no_entities() {
        let chunk: &[u8] = br#"<component class="sector" macro="m"><component class="zone"></component></component>"#;
        let out = parse_sector_chunk(chunk, "m");
        assert!(out.is_empty());
    }

    #[test]
    fn nested_ship_inside_station_emits_two_records_with_parent_link() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="zone">
    <component class="station" macro="station_arg_factory_01" owner="argon" code="YIB-1" id="[0x100]">
      <offset><position x="0" y="0" z="0"/></offset>
      <connections>
        <component class="ship_xs" macro="ship_xs_drone_01" owner="argon" id="[0x200]">
          <offset><position x="500" y="0" z="0"/></offset>
        </component>
      </connections>
    </component>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        assert_eq!(out.len(), 2);

        let station = out.iter().find(|r| r.id == 0x100).unwrap();
        assert_eq!(station.parent_id, None);
        assert_eq!(station.code.as_deref(), Some("YIB-1"));
        assert_eq!(station.kind, map_domain::world::LiveObjectKind::Station);

        let drone = out.iter().find(|r| r.id == 0x200).unwrap();
        assert_eq!(drone.parent_id, Some(0x100));
        assert_eq!(drone.kind, map_domain::world::LiveObjectKind::ShipSmall);
        // Position is captured relative to drone's own offset (parent-local).
        assert!((drone.position.x - 0.5).abs() < 1e-3);
    }

    #[test]
    fn three_level_nesting_emits_chain() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" macro="parent_st" owner="argon" code="P-1" id="[0x10]">
    <offset><position x="0" y="0" z="0"/></offset>
    <component class="ship_l" macro="carrier" owner="argon" code="C-1" id="[0x20]">
      <offset><position x="1000" y="0" z="0"/></offset>
      <component class="ship_xs" macro="drone" owner="argon" id="[0x30]">
        <offset><position x="100" y="0" z="0"/></offset>
      </component>
    </component>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        assert_eq!(out.len(), 3);
        let p = out.iter().find(|r| r.id == 0x10).unwrap();
        let c = out.iter().find(|r| r.id == 0x20).unwrap();
        let d = out.iter().find(|r| r.id == 0x30).unwrap();
        assert_eq!(p.parent_id, None);
        assert_eq!(c.parent_id, Some(0x10));
        assert_eq!(d.parent_id, Some(0x20));
    }

    #[test]
    fn nested_offset_does_not_overwrite_parent_position() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" owner="argon" id="[0xA]">
    <offset><position x="7000" y="0" z="0"/></offset>
    <component class="ship_s" owner="argon" id="[0xB]">
      <offset><position x="1" y="0" z="0"/></offset>
    </component>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let st = out.iter().find(|r| r.id == 0xA).unwrap();
        assert!((st.position.x - 7.0).abs() < 1e-3);
    }

    #[test]
    fn parses_station_trade_offers() {
        use map_domain::world::TradeDirection;
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" macro="st" owner="argon" id="[0x100]">
    <offset><position x="0" y="0" z="0"/></offset>
    <trade>
      <reservations>
        <reservation id="[0x900]" buyer="[0x999]" partner="[0x100]" ware="ignored" price="1" amount="1" desired="1"/>
      </reservations>
      <offers>
        <production>
          <trade id="[0x1]" buyer="[0x100]" ware="energycells" price="1092" amount="0"/>
          <trade id="[0x2]" buyer="[0x100]" ware="spices" price="2800" amount="4672" desired="4672"/>
          <trade id="[0x3]" seller="[0x100]" ware="medicalsupplies" price="7174" amount="6299"/>
        </production>
      </offers>
    </trade>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let station = out.iter().find(|r| r.id == 0x100).expect("station present");
        assert_eq!(
            station.trade_offers.len(),
            3,
            "expected 3 offers, got {:?}",
            station.trade_offers
        );

        let ec = station
            .trade_offers
            .iter()
            .find(|o| o.ware_id == "energycells")
            .unwrap();
        assert_eq!(ec.direction, TradeDirection::Buy);
        assert_eq!(ec.price, 1092);
        assert_eq!(ec.amount, 0);
        assert_eq!(ec.desired, 0, "desired absent ⇒ 0");

        let sp = station
            .trade_offers
            .iter()
            .find(|o| o.ware_id == "spices")
            .unwrap();
        assert_eq!(sp.direction, TradeDirection::Buy);
        assert_eq!(sp.desired, 4672);

        let med = station
            .trade_offers
            .iter()
            .find(|o| o.ware_id == "medicalsupplies")
            .unwrap();
        assert_eq!(med.direction, TradeDirection::Sell);
        assert_eq!(med.amount, 6299);
    }

    #[test]
    fn reservation_outside_offers_is_not_a_trade_offer() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" macro="st" owner="argon" id="[0x100]">
    <offset><position x="0" y="0" z="0"/></offset>
    <trade>
      <reservations>
        <reservation id="[0x900]" buyer="[0x999]" partner="[0x100]" ware="ignored" price="1" amount="1" desired="1"/>
      </reservations>
    </trade>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let station = out.iter().find(|r| r.id == 0x100).unwrap();
        assert!(station.trade_offers.is_empty());
    }

    #[test]
    fn parses_ship_name_station_basename_and_literal() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="ship_l" macro="ship_par_l_trans_container_03_a_macro"
             name="{20101,122701}" code="AKV-484" owner="alliance" id="[0x10]">
    <offset><position x="0" y="0" z="0"/></offset>
  </component>
  <component class="station" macro="station_gen_factory_base_01_macro"
             basename="{20102,1701}" code="FAR-140" owner="freesplit" id="[0x20]">
    <offset><position x="0" y="0" z="0"/></offset>
  </component>
  <component class="station" macro="station_gen_factory_base_01_macro"
             name="{20103,2001}" basename="{20103,2001}"
             code="NLK-443" owner="split" id="[0x21]">
    <offset><position x="0" y="0" z="0"/></offset>
  </component>
  <component class="ship_s" macro="ship_arg_s_scout_01_a_macro"
             name="My Best Ship" code="MBS-001" owner="player" id="[0x30]">
    <offset><position x="0" y="0" z="0"/></offset>
  </component>
  <component class="ship_xs" macro="ship_gen_xs_escapepod_01_a_macro"
             code="PXP-294" owner="paranid" id="[0x40]">
    <offset><position x="0" y="0" z="0"/></offset>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let ship_l = out.iter().find(|r| r.id == 0x10).unwrap();
        assert_eq!(ship_l.display_name_ref.as_deref(), Some("{20101,122701}"));

        let station_basename = out.iter().find(|r| r.id == 0x20).unwrap();
        assert_eq!(
            station_basename.display_name_ref.as_deref(),
            Some("{20102,1701}")
        );

        // When both name= and basename= are present, name= wins.
        let station_named = out.iter().find(|r| r.id == 0x21).unwrap();
        assert_eq!(
            station_named.display_name_ref.as_deref(),
            Some("{20103,2001}")
        );

        let renamed_ship = out.iter().find(|r| r.id == 0x30).unwrap();
        assert_eq!(
            renamed_ship.display_name_ref.as_deref(),
            Some("My Best Ship")
        );

        let pod = out.iter().find(|r| r.id == 0x40).unwrap();
        assert_eq!(pod.display_name_ref, None);
    }

    #[test]
    fn captures_name_index_and_job_source() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="ship_xl" macro="ship_arg_xl_builder_01_a_macro"
             code="XWX-653" owner="argon" id="[0x10]">
    <offset><position x="0" y="0" z="0"/></offset>
    <source job="Argon_Construction_Vessel" class="job"/>
  </component>
  <component class="station" macro="station_gen_factory_base_01_macro"
             code="BPF-030" owner="argon" nameindex="3" id="[0x20]">
    <offset><position x="0" y="0" z="0"/></offset>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let ship = out.iter().find(|r| r.id == 0x10).unwrap();
        // Job is lowercased; nameindex absent here.
        assert_eq!(ship.job.as_deref(), Some("argon_construction_vessel"));
        assert_eq!(ship.name_index, None);

        let station = out.iter().find(|r| r.id == 0x20).unwrap();
        assert_eq!(station.name_index, Some(3));
        assert_eq!(station.job, None);
    }

    #[test]
    fn captures_first_prod_module_in_station_construction() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" macro="station_gen_factory_base_01_macro" id="[0x100]">
    <offset><position x="0" y="0" z="0"/></offset>
    <construction>
      <sequence>
        <entry id="[0x1]" index="1" macro="pier_spl_harbor_01_macro"/>
        <entry id="[0x2]" index="2" macro="prod_gen_microchips_macro"/>
        <entry id="[0x3]" index="3" macro="prod_gen_energycells_macro"/>
      </sequence>
    </construction>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let st = out.iter().find(|r| r.id == 0x100).unwrap();
        assert_eq!(
            st.production_module_macro.as_deref(),
            Some("prod_gen_microchips_macro")
        );
    }

    #[test]
    fn no_production_module_when_none_present() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" macro="x" id="[0x100]">
    <offset><position x="0" y="0" z="0"/></offset>
    <construction><sequence>
      <entry id="[0x1]" macro="pier_arg_harbor_01_macro"/>
    </sequence></construction>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let st = out.iter().find(|r| r.id == 0x100).unwrap();
        assert_eq!(st.production_module_macro, None);
    }

    #[test]
    fn docked_ship_inherits_no_offers() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" macro="st" owner="argon" id="[0x100]">
    <offset><position x="0" y="0" z="0"/></offset>
    <trade><offers><production>
      <trade id="[0x1]" buyer="[0x100]" ware="energycells" price="50" amount="0"/>
    </production></offers></trade>
    <connections>
      <component class="ship_xs" macro="drone" owner="argon" id="[0x200]">
        <offset><position x="0" y="0" z="0"/></offset>
      </component>
    </connections>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let drone = out.iter().find(|r| r.id == 0x200).unwrap();
        assert!(drone.trade_offers.is_empty());
        let station = out.iter().find(|r| r.id == 0x100).unwrap();
        assert_eq!(station.trade_offers.len(), 1);
    }
}
