//! Icon atlas: enum of icon ids, glyph table, UV lookup.
//!
//! Rasterisation lives in this module too but is invoked by `gpu.rs` at
//! pipeline init.

use std::collections::HashMap;

use map_domain::objects::StaticObjectKind;
use map_domain::world::LiveObjectKind;

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

pub fn classify_live(
    kind: LiveObjectKind,
    macro_name: &str,
    owner: Option<&str>,
) -> IconId {
    let m = macro_name.to_lowercase();
    match kind {
        LiveObjectKind::Station => {
            if owner == Some("player") { return IconId::PlayerStation; }
            if m.contains("wharf") || m.contains("shipyard") { return IconId::WharfShipyard; }
            if m.contains("defence") || m.contains("defense") { return IconId::Defense; }
            if m.contains("trading") { return IconId::Trading; }
            if m.contains("equip") || m.contains("dock") { return IconId::EquipDock; }
            if m.contains("hq") || m.contains("admin") || m.contains("headquarter") {
                return IconId::HQ;
            }
            if m.contains("factory") || m.contains("refinery") || m.contains("production") {
                return IconId::Factory;
            }
            IconId::GenericStation
        }
        _ if m.contains("trans") || m.contains("freight") || m.contains("miner") => IconId::Transport,
        LiveObjectKind::ShipExtraLarge | LiveObjectKind::ShipLarge => IconId::Capital,
        LiveObjectKind::ShipMedium => IconId::Medium,
        LiveObjectKind::ShipSmall => IconId::Small,
    }
}

pub fn classify_static(kind: &StaticObjectKind) -> Option<IconId> {
    match kind {
        StaticObjectKind::Anomaly => Some(IconId::Anomaly),
        StaticObjectKind::ResourceZone => Some(IconId::ResourceZone),
        StaticObjectKind::Station => Some(IconId::GenericStation),
        StaticObjectKind::Gate | StaticObjectKind::Highway => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_domain::objects::StaticObjectKind;
    use map_domain::world::LiveObjectKind;

    #[test]
    fn classify_live_routes_factory_macro_to_factory_icon() {
        let icon = classify_live(LiveObjectKind::Station, "station_arg_factory_food_01_macro", Some("argon"));
        assert_eq!(icon, IconId::Factory);
    }

    #[test]
    fn classify_live_player_owner_wins_over_macro() {
        let icon = classify_live(LiveObjectKind::Station, "station_arg_factory_food_01_macro", Some("player"));
        assert_eq!(icon, IconId::PlayerStation);
    }

    #[test]
    fn classify_live_wharf_or_shipyard() {
        assert_eq!(classify_live(LiveObjectKind::Station, "station_par_wharf_macro", Some("paranid")), IconId::WharfShipyard);
        assert_eq!(classify_live(LiveObjectKind::Station, "station_tel_shipyard_macro", Some("teladi")), IconId::WharfShipyard);
    }

    #[test]
    fn classify_live_defense_handles_both_spellings() {
        assert_eq!(classify_live(LiveObjectKind::Station, "station_arg_defence_platform_macro", None), IconId::Defense);
        assert_eq!(classify_live(LiveObjectKind::Station, "station_arg_defense_module_macro", None), IconId::Defense);
    }

    #[test]
    fn classify_live_capital_ship_by_size() {
        assert_eq!(classify_live(LiveObjectKind::ShipLarge, "ship_arg_l_destroyer_01_macro", Some("argon")), IconId::Capital);
        assert_eq!(classify_live(LiveObjectKind::ShipExtraLarge, "ship_arg_xl_carrier_01_macro", Some("argon")), IconId::Capital);
    }

    #[test]
    fn classify_live_transport_keyword_overrides_size() {
        assert_eq!(classify_live(LiveObjectKind::ShipLarge, "ship_arg_l_freighter_01_macro", Some("argon")), IconId::Transport);
        assert_eq!(classify_live(LiveObjectKind::ShipMedium, "ship_tel_m_miner_01_macro", Some("teladi")), IconId::Transport);
    }

    #[test]
    fn classify_live_small_ship_default() {
        assert_eq!(classify_live(LiveObjectKind::ShipSmall, "ship_arg_s_scout_01_macro", Some("argon")), IconId::Small);
    }

    #[test]
    fn classify_live_unknown_station_falls_back_to_generic() {
        assert_eq!(classify_live(LiveObjectKind::Station, "station_xxx_unknown_macro", None), IconId::GenericStation);
    }

    #[test]
    fn classify_static_anomaly_and_resource_zone() {
        assert_eq!(classify_static(&StaticObjectKind::Anomaly), Some(IconId::Anomaly));
        assert_eq!(classify_static(&StaticObjectKind::ResourceZone), Some(IconId::ResourceZone));
    }

    #[test]
    fn classify_static_returns_none_for_gates_and_highways() {
        assert_eq!(classify_static(&StaticObjectKind::Gate), None);
        assert_eq!(classify_static(&StaticObjectKind::Highway), None);
    }

    #[test]
    fn classify_static_returns_generic_for_station_variant() {
        assert_eq!(classify_static(&StaticObjectKind::Station), Some(IconId::GenericStation));
    }

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
