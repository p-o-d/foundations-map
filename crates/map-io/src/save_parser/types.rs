//! Shared types for the parallel save parser.

use map_domain::world::{LiveObjectKind, TradeOffer};
use std::ops::Range;

/// Map sector_macro (lowercase) → faction owner string from `<sector owner="...">`.
/// Caller resolves owner string to FactionId.
pub type FactionOverrides = std::collections::HashMap<String, String>;

/// Byte range of one `<component class="sector" …>…</component>` subtree
/// inside the decompressed save buffer.
#[derive(Debug, Clone)]
pub struct SectorChunk {
    pub sector_macro: String, // lowercase
    pub byte_range: Range<usize>,
}

/// One ship or station extracted from a sector chunk by a Stage 3 worker.
/// Caller resolves `sector_macro` → SectorId and `owner` → FactionId.
#[derive(Debug, Clone)]
pub struct EntityRecord {
    pub id: u32, // parsed from "[0xHEX]"
    pub parent_id: Option<u32>,
    pub macro_name: String,
    pub code: Option<String>,
    pub kind: LiveObjectKind,
    pub owner: Option<String>,
    /// Sum of the offsets from the nearest enclosing zone down to this entity,
    /// in km — i.e. the position relative to that zone. `merge` adds
    /// `zone_positions[zone_macro]` to reach the true sector-relative position.
    pub position: glam::Vec3, // km (metres / 1000)
    pub sector_macro: String, // lowercase
    /// Lowercase macro of the nearest enclosing `<component class="zone">`, used
    /// to look up the zone's sector-relative position. `None` if no zone ancestor.
    pub zone_macro: Option<String>,
    pub trade_offers: Vec<TradeOffer>,
    pub display_name_ref: Option<String>,
    pub production_module_macro: Option<String>,
    /// Station carries a wharf build-module (builds S/M ships).
    pub is_wharf: bool,
    /// Station carries a shipyard build-module (builds L/XL ships / carriers).
    pub is_shipyard: bool,
    /// Station macro identifies it as a trading station (`tradestation`).
    pub is_trade: bool,
    /// `nameindex` attribute → trailing roman numeral on generated names.
    pub name_index: Option<u32>,
    /// `job` id (lowercase) → job-name prefix for NPC ships.
    pub job: Option<String>,
    /// Entity carries `state="wreck"` — a destroyed hull, not a live ship.
    pub is_wreck: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_domain::world::LiveObjectKind;

    #[test]
    fn sector_chunk_holds_range_and_macro() {
        let c = SectorChunk {
            sector_macro: "cluster_01_sector001_macro".into(),
            byte_range: 100..2000,
        };
        assert_eq!(c.byte_range.len(), 1900);
        assert!(c.sector_macro.starts_with("cluster_"));
    }

    #[test]
    fn entity_record_constructs() {
        let e = EntityRecord {
            id: 0x100,
            parent_id: None,
            macro_name: "station_arg_factory_01".into(),
            code: Some("YIB-942".into()),
            kind: LiveObjectKind::Station,
            owner: Some("argon".into()),
            position: glam::Vec3::new(0.0, 0.0, 0.0),
            sector_macro: "cluster_01_sector001_macro".into(),
            zone_macro: None,
            trade_offers: vec![],
            display_name_ref: Some("{20102,1701}".into()),
            production_module_macro: None,
            is_wharf: false,
            is_shipyard: false,
            is_trade: false,
            name_index: None,
            job: None,
            is_wreck: false,
        };
        assert_eq!(e.id, 0x100);
        assert_eq!(e.display_name_ref.as_deref(), Some("{20102,1701}"));
        assert!(e.trade_offers.is_empty());
        assert_eq!(e.production_module_macro, None);
    }
}
