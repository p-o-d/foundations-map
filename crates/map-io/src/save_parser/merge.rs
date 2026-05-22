//! Stage 4: merge per-worker entity records into a single `World`.

use std::collections::HashMap;

use map_domain::ids::{FactionId, SectorId};
use map_domain::world::World;

use super::types::EntityRecord;

/// Combine all per-worker entity lists into a single `World`. Resolves each
/// record's `sector_macro` via `sector_macros` (drops records whose sector
/// isn't known). For new owner strings not already present in `faction_strings`,
/// allocates the next FactionId from `next_faction_id` and inserts the mapping.
pub fn merge(
    batches: Vec<Vec<EntityRecord>>,
    sector_macros: Option<&HashMap<String, SectorId>>,
    faction_strings: &mut HashMap<String, FactionId>,
    next_faction_id: &mut u32,
) -> World {
    let mut world = World::new();
    let Some(sector_macros) = sector_macros else {
        return world;
    };

    for batch in batches {
        for r in batch {
            let Some(&sec_id) = sector_macros.get(&r.sector_macro) else {
                continue;
            };
            let faction = r.owner.map(|name| {
                let name = name.to_lowercase();
                *faction_strings.entry(name).or_insert_with(|| {
                    let id = FactionId(*next_faction_id);
                    *next_faction_id += 1;
                    id
                })
            });
            let entity_id = r.id;
            let trade_offers = r.trade_offers;
            world.insert_entity(
                entity_id,
                r.macro_name,
                r.kind,
                faction,
                r.position,
                sec_id,
                r.parent_id,
                r.code,
            );
            if !trade_offers.is_empty() {
                world.trade_offers.insert(entity_id, trade_offers);
            }
        }
    }
    world
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_domain::ids::SectorId;
    use map_domain::world::LiveObjectKind;
    use std::collections::HashMap;

    use crate::save_parser::types::EntityRecord;

    #[test]
    fn merges_records_and_assigns_faction_ids() {
        let records = vec![
            EntityRecord {
                id: 0x10, parent_id: None, macro_name: "station_a".into(), code: None,
                kind: LiveObjectKind::Station, owner: Some("argon".into()),
                position: glam::Vec3::ZERO, sector_macro: "sa".into(),
                trade_offers: vec![],
            },
            EntityRecord {
                id: 0x11, parent_id: Some(0x10), macro_name: "drone".into(), code: Some("D-1".into()),
                kind: LiveObjectKind::ShipSmall, owner: Some("argon".into()),
                position: glam::Vec3::ZERO, sector_macro: "sa".into(),
                trade_offers: vec![],
            },
        ];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut next = 1u32;
        let world = merge(vec![records], Some(&sm), &mut fs, &mut next);
        assert_eq!(world.names.len(), 2);
        assert_eq!(world.parent_of(0x11), Some(0x10));
        assert_eq!(world.children_of(0x10), &[0x11]);
        assert_eq!(world.codes.get(&0x11).map(String::as_str), Some("D-1"));
        assert_eq!(fs.get("argon").copied(), Some(FactionId(1)));
        assert_eq!(next, 2);
    }

    #[test]
    fn unknown_sector_drops_entity() {
        let records = vec![EntityRecord {
            id: 0xFFFF, parent_id: None, macro_name: "x".into(), code: None,
            kind: LiveObjectKind::ShipSmall, owner: None,
            position: glam::Vec3::ZERO, sector_macro: "unknown".into(),
            trade_offers: vec![],
        }];
        let sm: HashMap<String, SectorId> = HashMap::new();
        let mut fs = HashMap::new();
        let mut next = 1u32;
        let world = merge(vec![records], Some(&sm), &mut fs, &mut next);
        assert!(world.names.is_empty());
    }

    #[test]
    fn no_sector_macros_drops_all() {
        let records = vec![EntityRecord {
            id: 1, parent_id: None, macro_name: "x".into(), code: None,
            kind: LiveObjectKind::Station, owner: None,
            position: glam::Vec3::ZERO, sector_macro: "anything".into(),
            trade_offers: vec![],
        }];
        let mut fs = HashMap::new();
        let mut next = 1u32;
        let world = merge(vec![records], None, &mut fs, &mut next);
        assert!(world.names.is_empty());
    }

    #[test]
    fn trade_offers_propagated_to_world() {
        use map_domain::world::{TradeDirection, TradeOffer};
        let records = vec![EntityRecord {
            id: 0x10,
            parent_id: None,
            macro_name: "station_a".into(),
            code: None,
            kind: LiveObjectKind::Station,
            owner: Some("argon".into()),
            position: glam::Vec3::ZERO,
            sector_macro: "sa".into(),
            trade_offers: vec![TradeOffer {
                ware_id: "energycells".into(),
                direction: TradeDirection::Buy,
                price: 1092,
                amount: 0,
                desired: 1200,
            }],
        }];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut next = 1u32;
        let world = merge(vec![records], Some(&sm), &mut fs, &mut next);
        let offers = world.trade_offers_of(0x10);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].ware_id, "energycells");
        assert_eq!(offers[0].direction, TradeDirection::Buy);
    }
}
