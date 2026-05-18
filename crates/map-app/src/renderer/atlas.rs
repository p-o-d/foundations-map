//! Icon classification + glyph lookup for screen-space painter rendering.
//!
//! No GPU atlas — icons are drawn by `egui::Painter::text` using the
//! application's font. Each entity is classified to an `IconId`; the lookup
//! returns the Unicode codepoint for that icon.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SuperCategory {
    Station,
    Ship,
    Static,
}

impl IconId {
    pub fn super_category(self) -> SuperCategory {
        match self {
            IconId::Factory
            | IconId::WharfShipyard
            | IconId::Defense
            | IconId::Trading
            | IconId::EquipDock
            | IconId::HQ
            | IconId::PlayerStation
            | IconId::GenericStation => SuperCategory::Station,

            IconId::Capital
            | IconId::Medium
            | IconId::Small
            | IconId::Transport => SuperCategory::Ship,

            IconId::Anomaly
            | IconId::ResourceZone => SuperCategory::Static,
        }
    }
}

const GLYPHS: &[(IconId, char)] = &[
    (IconId::Factory,        '⚙'),
    (IconId::WharfShipyard,  '⎈'),
    (IconId::Defense,        '⚔'),
    (IconId::Trading,        '¤'),
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

pub fn icon_char(icon: IconId) -> char {
    GLYPHS.iter().find(|(i, _)| *i == icon).map(|(_, c)| *c).unwrap_or('?')
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

    #[test]
    fn icon_char_returns_known_glyphs() {
        assert_eq!(icon_char(IconId::Factory), '⚙');
        assert_eq!(icon_char(IconId::WharfShipyard), '⎈');
        assert_eq!(icon_char(IconId::Trading), '¤');
        assert_eq!(icon_char(IconId::Anomaly), '✦');
    }

    #[test]
    fn glyph_table_has_one_entry_per_variant() {
        let all: Vec<IconId> = GLYPHS.iter().map(|(i, _)| *i).collect();
        let expected = [
            IconId::Factory, IconId::WharfShipyard, IconId::Defense, IconId::Trading,
            IconId::EquipDock, IconId::HQ, IconId::PlayerStation, IconId::GenericStation,
            IconId::Capital, IconId::Medium, IconId::Small, IconId::Transport,
            IconId::Anomaly, IconId::ResourceZone,
        ];
        for e in &expected { assert!(all.contains(e), "missing {:?}", e); }
        assert_eq!(all.len(), expected.len());
    }

    #[test]
    fn classify_live_routes_factory_macro_to_factory_icon() {
        assert_eq!(classify_live(LiveObjectKind::Station, "station_arg_factory_food_01_macro", Some("argon")), IconId::Factory);
    }

    #[test]
    fn classify_live_player_owner_wins_over_macro() {
        assert_eq!(classify_live(LiveObjectKind::Station, "station_arg_factory_food_01_macro", Some("player")), IconId::PlayerStation);
    }

    #[test]
    fn classify_live_transport_keyword_overrides_size() {
        assert_eq!(classify_live(LiveObjectKind::ShipLarge, "ship_arg_l_freighter_01_macro", Some("argon")), IconId::Transport);
    }

    #[test]
    fn classify_static_returns_none_for_gates_and_highways() {
        assert_eq!(classify_static(&StaticObjectKind::Gate), None);
        assert_eq!(classify_static(&StaticObjectKind::Highway), None);
    }

    #[test]
    fn super_category_station_variants() {
        for v in [
            IconId::Factory, IconId::WharfShipyard, IconId::Defense, IconId::Trading,
            IconId::EquipDock, IconId::HQ, IconId::PlayerStation, IconId::GenericStation,
        ] {
            assert_eq!(v.super_category(), SuperCategory::Station, "{:?}", v);
        }
    }

    #[test]
    fn super_category_ship_variants() {
        for v in [IconId::Capital, IconId::Medium, IconId::Small, IconId::Transport] {
            assert_eq!(v.super_category(), SuperCategory::Ship, "{:?}", v);
        }
    }

    #[test]
    fn super_category_static_variants() {
        for v in [IconId::Anomaly, IconId::ResourceZone] {
            assert_eq!(v.super_category(), SuperCategory::Static, "{:?}", v);
        }
    }
}
