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
    zone_positions: &HashMap<String, (f32, f32, f32)>,
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
            let display_name_ref = r.display_name_ref;
            let production_module_macro = r.production_module_macro;
            let (is_wharf, is_shipyard, is_trade) = (r.is_wharf, r.is_shipyard, r.is_trade);
            // Add the enclosing zone's sector-relative position (km) so live
            // objects share the same frame as static gates. Named static zones
            // carry a zero save offset and get their position from here; dynamic
            // zones (e.g. tempzone) carry their offset in `r.position` already
            // and are absent from `zone_positions`, so this adds nothing.
            let zone_off = r
                .zone_macro
                .as_ref()
                .and_then(|z| zone_positions.get(z))
                .map(|(x, y, z)| glam::Vec3::new(x / 1000.0, y / 1000.0, z / 1000.0))
                .unwrap_or(glam::Vec3::ZERO);
            world.insert_entity(
                entity_id,
                r.macro_name,
                r.kind,
                faction,
                r.position + zone_off,
                sec_id,
                r.parent_id,
                r.code,
            );
            if !trade_offers.is_empty() {
                world.trade_offers.insert(entity_id, trade_offers);
            }
            if let Some(name_ref) = display_name_ref {
                world.display_name_refs.insert(entity_id, name_ref);
            }
            if let Some(mc) = production_module_macro {
                world
                    .production_modules
                    .insert(entity_id, mc.to_lowercase());
            }
            if is_wharf {
                world.wharf_stations.insert(entity_id);
            }
            if is_shipyard {
                world.shipyard_stations.insert(entity_id);
            }
            if is_trade {
                world.trade_stations.insert(entity_id);
            }
            if let Some(ni) = r.name_index {
                world.name_index.insert(entity_id, ni);
            }
            if let Some(job) = r.job {
                world.entity_jobs.insert(entity_id, job);
            }
            if r.is_wreck {
                world.wreck_entities.insert(entity_id);
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
                id: 0x10,
                parent_id: None,
                macro_name: "station_a".into(),
                code: None,
                kind: LiveObjectKind::Station,
                owner: Some("argon".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sa".into(),
                zone_macro: None,
                trade_offers: vec![],
                display_name_ref: None,
                production_module_macro: None,
                is_wharf: false,
                is_shipyard: false,
                is_trade: false,
                name_index: None,
                job: None,
                is_wreck: false,
            },
            EntityRecord {
                id: 0x11,
                parent_id: Some(0x10),
                macro_name: "drone".into(),
                code: Some("D-1".into()),
                kind: LiveObjectKind::ShipSmall,
                owner: Some("argon".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sa".into(),
                zone_macro: None,
                trade_offers: vec![],
                display_name_ref: None,
                production_module_macro: None,
                is_wharf: false,
                is_shipyard: false,
                is_trade: false,
                name_index: None,
                job: None,
                is_wreck: false,
            },
        ];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut next = 1u32;
        let zp = HashMap::new();
        let world = merge(vec![records], Some(&sm), &zp, &mut fs, &mut next);
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
            id: 0xFFFF,
            parent_id: None,
            macro_name: "x".into(),
            code: None,
            kind: LiveObjectKind::ShipSmall,
            owner: None,
            position: glam::Vec3::ZERO,
            sector_macro: "unknown".into(),
            zone_macro: None,
            trade_offers: vec![],
            display_name_ref: None,
            production_module_macro: None,
            is_wharf: false,
            is_shipyard: false,
            is_trade: false,
            name_index: None,
            job: None,
            is_wreck: false,
        }];
        let sm: HashMap<String, SectorId> = HashMap::new();
        let mut fs = HashMap::new();
        let mut next = 1u32;
        let zp = HashMap::new();
        let world = merge(vec![records], Some(&sm), &zp, &mut fs, &mut next);
        assert!(world.names.is_empty());
    }

    #[test]
    fn no_sector_macros_drops_all() {
        let records = vec![EntityRecord {
            id: 1,
            parent_id: None,
            macro_name: "x".into(),
            code: None,
            kind: LiveObjectKind::Station,
            owner: None,
            position: glam::Vec3::ZERO,
            sector_macro: "anything".into(),
            zone_macro: None,
            trade_offers: vec![],
            display_name_ref: None,
            production_module_macro: None,
            is_wharf: false,
            is_shipyard: false,
            is_trade: false,
            name_index: None,
            job: None,
            is_wreck: false,
        }];
        let mut fs = HashMap::new();
        let mut next = 1u32;
        let zp = HashMap::new();
        let world = merge(vec![records], None, &zp, &mut fs, &mut next);
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
            zone_macro: None,
            trade_offers: vec![TradeOffer {
                ware_id: "energycells".into(),
                direction: TradeDirection::Buy,
                price: 1092,
                amount: 0,
                desired: 1200,
            }],
            display_name_ref: None,
            production_module_macro: None,
            is_wharf: false,
            is_shipyard: false,
            is_trade: false,
            name_index: None,
            job: None,
            is_wreck: false,
        }];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut next = 1u32;
        let zp = HashMap::new();
        let world = merge(vec![records], Some(&sm), &zp, &mut fs, &mut next);
        let offers = world.trade_offers_of(0x10);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].ware_id, "energycells");
        assert_eq!(offers[0].direction, TradeDirection::Buy);
    }

    #[test]
    fn production_module_propagated_to_world() {
        let records = vec![EntityRecord {
            id: 0x77,
            parent_id: None,
            macro_name: "station_gen_factory_base_01_macro".into(),
            code: None,
            kind: LiveObjectKind::Station,
            owner: Some("freesplit".into()),
            position: glam::Vec3::ZERO,
            sector_macro: "sa".into(),
            zone_macro: None,
            trade_offers: vec![],
            display_name_ref: None,
            production_module_macro: Some("prod_gen_microchips_macro".into()),
            is_wharf: false,
            is_shipyard: false,
            is_trade: false,
            name_index: None,
            job: None,
            is_wreck: false,
        }];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut next = 1u32;
        let zp = HashMap::new();
        let world = merge(vec![records], Some(&sm), &zp, &mut fs, &mut next);
        assert_eq!(
            world.production_modules.get(&0x77).map(String::as_str),
            Some("prod_gen_microchips_macro")
        );
    }

    #[test]
    fn wharf_shipyard_flags_propagated_to_world() {
        let records = vec![
            EntityRecord {
                id: 0xA1,
                parent_id: None,
                macro_name: "station_arg_wharf".into(),
                code: None,
                kind: LiveObjectKind::Station,
                owner: Some("argon".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sa".into(),
                zone_macro: None,
                trade_offers: vec![],
                display_name_ref: None,
                production_module_macro: None,
                is_wharf: true,
                is_shipyard: false,
                is_trade: false,
                name_index: None,
                job: None,
                is_wreck: false,
            },
            EntityRecord {
                id: 0xA2,
                parent_id: None,
                macro_name: "station_arg_shipyard".into(),
                code: None,
                kind: LiveObjectKind::Station,
                owner: Some("argon".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sa".into(),
                zone_macro: None,
                trade_offers: vec![],
                display_name_ref: None,
                production_module_macro: None,
                is_wharf: false,
                is_shipyard: true,
                is_trade: false,
                name_index: None,
                job: None,
                is_wreck: false,
            },
        ];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut next = 1u32;
        let zp = HashMap::new();
        let world = merge(vec![records], Some(&sm), &zp, &mut fs, &mut next);
        assert!(world.wharf_stations.contains(&0xA1));
        assert!(!world.shipyard_stations.contains(&0xA1));
        assert!(world.shipyard_stations.contains(&0xA2));
    }

    #[test]
    fn wreck_flag_propagated_to_world() {
        let records = vec![
            EntityRecord {
                id: 0xDE,
                parent_id: None,
                macro_name: "ship_xen_xl_destroyer_01_a_macro".into(),
                code: None,
                kind: LiveObjectKind::ShipExtraLarge,
                owner: Some("xenon".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sa".into(),
                zone_macro: None,
                trade_offers: vec![],
                display_name_ref: None,
                production_module_macro: None,
                is_wharf: false,
                is_shipyard: false,
                is_trade: false,
                name_index: None,
                job: None,
                is_wreck: true,
            },
            EntityRecord {
                id: 0xA1,
                parent_id: None,
                macro_name: "ship_arg_s_scout_01_a_macro".into(),
                code: None,
                kind: LiveObjectKind::ShipSmall,
                owner: Some("argon".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sa".into(),
                zone_macro: None,
                trade_offers: vec![],
                display_name_ref: None,
                production_module_macro: None,
                is_wharf: false,
                is_shipyard: false,
                is_trade: false,
                name_index: None,
                job: None,
                is_wreck: false,
            },
        ];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut next = 1u32;
        let zp = HashMap::new();
        let world = merge(vec![records], Some(&sm), &zp, &mut fs, &mut next);
        assert!(world.wreck_entities.contains(&0xDE));
        assert!(!world.wreck_entities.contains(&0xA1));
    }

    #[test]
    fn display_name_refs_propagated_to_world() {
        let records = vec![EntityRecord {
            id: 0x55,
            parent_id: None,
            macro_name: "ship_par_l_trans_container_03_a_macro".into(),
            code: Some("AKV-484".into()),
            kind: LiveObjectKind::ShipLarge,
            owner: Some("alliance".into()),
            position: glam::Vec3::ZERO,
            sector_macro: "sa".into(),
            zone_macro: None,
            trade_offers: vec![],
            display_name_ref: Some("{20101,122701}".into()),
            production_module_macro: None,
            is_wharf: false,
            is_shipyard: false,
            is_trade: false,
            name_index: None,
            job: None,
            is_wreck: false,
        }];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut next = 1u32;
        let zp = HashMap::new();
        let world = merge(vec![records], Some(&sm), &zp, &mut fs, &mut next);
        assert_eq!(
            world.display_name_refs.get(&0x55).map(String::as_str),
            Some("{20101,122701}")
        );
    }
}
