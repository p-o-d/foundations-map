//! Stage 4: merge per-worker entity records into a single `World`.

use std::collections::HashMap;

use map_domain::ids::{FactionId, SectorId};
use map_domain::world::World;

use super::types::EntityRecord;

/// Combine all per-worker entity lists into a single `World`. Resolves each
/// record's `sector_macro` via `sector_macros` (drops records whose sector
/// isn't known). Assigns a fresh `FactionId` to each first-seen owner string.
pub fn merge(
    batches: Vec<Vec<EntityRecord>>,
    sector_macros: Option<&HashMap<String, SectorId>>,
) -> World {
    let mut world = World::new();
    let Some(sector_macros) = sector_macros else {
        return world;
    };

    let mut faction_ids: HashMap<String, FactionId> = HashMap::new();
    let mut next_faction_id: u32 = 1;

    for batch in batches {
        for r in batch {
            let Some(&sec_id) = sector_macros.get(&r.sector_macro) else {
                continue;
            };
            let faction = r.owner.map(|name| {
                *faction_ids.entry(name).or_insert_with(|| {
                    let id = FactionId(next_faction_id);
                    next_faction_id += 1;
                    id
                })
            });
            world.insert_entity(r.id, r.name, r.kind, faction, r.position, sec_id);
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
                name: "station_a".into(),
                kind: LiveObjectKind::Station,
                owner: Some("argon".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sa".into(),
            },
            EntityRecord {
                id: 0x11,
                name: "ship_a".into(),
                kind: LiveObjectKind::ShipSmall,
                owner: Some("argon".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sa".into(),
            },
            EntityRecord {
                id: 0x12,
                name: "ship_b".into(),
                kind: LiveObjectKind::ShipMedium,
                owner: Some("teladi".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sb".into(),
            },
        ];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        sm.insert("sb".into(), SectorId(2));

        let world = merge(vec![records], Some(&sm));
        assert_eq!(world.names.len(), 3);
        assert_eq!(world.entities_in_sector(SectorId(1)).len(), 2);
        assert_eq!(world.entities_in_sector(SectorId(2)).len(), 1);
        let argon = world.factions.get(&0x10).copied();
        let teladi = world.factions.get(&0x12).copied();
        assert!(argon.is_some());
        assert!(teladi.is_some());
        assert_ne!(argon, teladi);
    }

    #[test]
    fn unknown_sector_drops_entity() {
        let records = vec![EntityRecord {
            id: 0xFFFF,
            name: "x".into(),
            kind: LiveObjectKind::ShipSmall,
            owner: None,
            position: glam::Vec3::ZERO,
            sector_macro: "unknown".into(),
        }];
        let sm: HashMap<String, SectorId> = HashMap::new();
        let world = merge(vec![records], Some(&sm));
        assert!(world.names.is_empty());
    }

    #[test]
    fn no_sector_macros_drops_all() {
        let records = vec![EntityRecord {
            id: 1,
            name: "x".into(),
            kind: LiveObjectKind::Station,
            owner: None,
            position: glam::Vec3::ZERO,
            sector_macro: "anything".into(),
        }];
        let world = merge(vec![records], None);
        assert!(world.names.is_empty());
    }
}
