//! Shared types for the parallel save parser.

use map_domain::world::LiveObjectKind;
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
    pub name: String,
    pub kind: LiveObjectKind,
    pub owner: Option<String>,
    pub position: glam::Vec3, // already km (metres / 1000)
    pub sector_macro: String, // lowercase
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
            name: "station_arg_factory_01".into(),
            kind: LiveObjectKind::Station,
            owner: Some("argon".into()),
            position: glam::Vec3::new(0.0, 0.0, 0.0),
            sector_macro: "cluster_01_sector001_macro".into(),
        };
        assert_eq!(e.id, 0x100);
        assert_eq!(e.owner.as_deref(), Some("argon"));
    }
}
