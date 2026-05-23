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
    // Depth at which we are currently inside an <offset> element; None otherwise.
    let mut offset_depth: Option<u32> = None;
    // True while inside an `<offers>` element. `<trade>` elements inside this
    // scope (with a buyer= or seller= attribute) are offers, not in-flight trades.
    let mut in_offers: bool = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"component" => {
                comp_depth += 1;
                if let Some(p) = build_pending(e, comp_depth, stack.last().map(|sp| sp.id)) {
                    stack.push(p);
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"component" => {
                if let Some(top) = stack.last() {
                    if top.open_depth == comp_depth {
                        let mut p = stack.pop().unwrap();
                        out.push(EntityRecord {
                            id: p.id,
                            parent_id: p.parent_id,
                            macro_name: p.macro_name,
                            code: p.code,
                            kind: p.kind,
                            owner: p.owner,
                            position: p.position.unwrap_or(Vec3::ZERO),
                            sector_macro: sector_macro.to_string(),
                            trade_offers: std::mem::take(&mut p.trade_offers),
                            display_name_ref: p.display_name_ref.take(),
                        });
                    }
                }
                comp_depth = comp_depth.saturating_sub(1);
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
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"position" => {
                // Only attribute position to the top pending entity if THIS <offset> sits
                // immediately inside it (open_depth == offset_depth, since <offset> is a
                // non-component child at the same comp_depth as its parent component).
                // Prevents a nested child's <offset> from overwriting its parent's position.
                if let (Some(top), Some(od)) = (stack.last_mut(), offset_depth) {
                    if top.open_depth == od && top.position.is_none() {
                        let x = attr_f32(e, b"x").unwrap_or(0.0);
                        let y = attr_f32(e, b"y").unwrap_or(0.0);
                        let z = attr_f32(e, b"z").unwrap_or(0.0);
                        // X4 stores positions in metres; convert to km.
                        top.position = Some(Vec3::new(x / 1000.0, y / 1000.0, z / 1000.0));
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
    position: Option<Vec3>,
    trade_offers: Vec<TradeOffer>,
    display_name_ref: Option<String>,
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

    Some(Pending {
        open_depth: depth,
        id,
        parent_id,
        macro_name,
        code,
        kind,
        owner,
        position: None,
        trade_offers: Vec::new(),
        display_name_ref,
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
