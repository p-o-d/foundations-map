//! Icon atlas: enum of icon ids, glyph table, UV lookup.
//!
//! Rasterisation lives in this module too but is invoked by `gpu.rs` at
//! pipeline init.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconId {
    Factory,
    WharfShipyard,
    Defense,
    Trading,
    EquipDock,
    HQ,
    PlayerStation,
    GenericStation,
    Capital,
    Medium,
    Small,
    Transport,
    Anomaly,
    ResourceZone,
}

/// Ordered list — slot index in the atlas matches array index here.
pub const GLYPHS: &[(IconId, char)] = &[
    (IconId::Factory,        '⚙'),
    (IconId::WharfShipyard,  '⎈'),
    (IconId::Defense,        '⚔'),
    (IconId::Trading,        '⛁'),
    (IconId::EquipDock,      '⚒'),
    (IconId::HQ,             '⌂'),
    (IconId::PlayerStation,  '◉'),
    (IconId::GenericStation, '▦'),
    (IconId::Capital,        '◆'),
    (IconId::Medium,         '▶'),
    (IconId::Small,          '▴'),
    (IconId::Transport,      '▭'),
    (IconId::Anomaly,        '✦'),
    (IconId::ResourceZone,   '◎'),
];

pub const ATLAS_COLS: usize = 8;
pub const ATLAS_ROWS: usize = 3;
pub const TILE_PX: usize = 48;
pub const ATLAS_W: usize = ATLAS_COLS * TILE_PX;   // 384
pub const ATLAS_H: usize = ATLAS_ROWS * TILE_PX;   // 144

/// Lookup of IconId → [u_min, v_min, du, dv] (atlas-normalised UV rect).
#[derive(Debug, Clone)]
pub struct AtlasLookup {
    pub uv: HashMap<IconId, [f32; 4]>,
}

impl AtlasLookup {
    pub fn build() -> Self {
        let mut uv = HashMap::new();
        for (idx, (icon, _)) in GLYPHS.iter().enumerate() {
            let col = idx % ATLAS_COLS;
            let row = idx / ATLAS_COLS;
            let u0 = (col * TILE_PX) as f32 / ATLAS_W as f32;
            let v0 = (row * TILE_PX) as f32 / ATLAS_H as f32;
            let du = TILE_PX as f32 / ATLAS_W as f32;
            let dv = TILE_PX as f32 / ATLAS_H as f32;
            uv.insert(*icon, [u0, v0, du, dv]);
        }
        Self { uv }
    }

    pub fn uv_of(&self, icon: IconId) -> [f32; 4] {
        *self.uv.get(&icon).expect("every IconId must be in GLYPHS")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_table_covers_every_icon_id() {
        let all: Vec<IconId> = GLYPHS.iter().map(|(i, _)| *i).collect();
        let expected = [
            IconId::Factory, IconId::WharfShipyard, IconId::Defense, IconId::Trading,
            IconId::EquipDock, IconId::HQ, IconId::PlayerStation, IconId::GenericStation,
            IconId::Capital, IconId::Medium, IconId::Small, IconId::Transport,
            IconId::Anomaly, IconId::ResourceZone,
        ];
        for e in &expected {
            assert!(all.contains(e), "missing IconId in GLYPHS: {:?}", e);
        }
        assert_eq!(all.len(), expected.len(), "GLYPHS has extras or duplicates");
    }

    #[test]
    fn atlas_layout_assigns_unique_uv_rects_per_icon() {
        let a = AtlasLookup::build();
        let mut seen: Vec<[u32; 4]> = Vec::new();
        for (icon, _) in GLYPHS {
            let r = a.uv_of(*icon);
            let key = [
                (r[0] * 1_000_000.0) as u32,
                (r[1] * 1_000_000.0) as u32,
                (r[2] * 1_000_000.0) as u32,
                (r[3] * 1_000_000.0) as u32,
            ];
            assert!(!seen.contains(&key), "duplicate UV rect for {:?}", icon);
            seen.push(key);
        }
        for [u, v, du, dv] in seen.iter().map(|[u, v, du, dv]| [
            *u as f32 / 1_000_000.0, *v as f32 / 1_000_000.0,
            *du as f32 / 1_000_000.0, *dv as f32 / 1_000_000.0,
        ]) {
            assert!(u >= 0.0 && u + du <= 1.0);
            assert!(v >= 0.0 && v + dv <= 1.0);
        }
    }
}
