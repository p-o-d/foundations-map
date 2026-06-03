//! Map filter modes for the 2D universe view.
//!
//! Each non-`Normal` mode greys out sectors that do not match and lists the
//! matching items in a hover tooltip. This module is pure: it computes which
//! sectors match and what hits they contain; name/colour resolution happens in
//! the app layer.

use std::collections::HashMap;

use crate::ids::SectorId;
use crate::resources::{SectorResource, combine_resources};
use crate::universe::Universe;
use crate::world::{EntityId, LiveObjectKind, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapFilterMode {
    Normal,
    Wharfs,
    Shipyards,
    Resources,
    FreeShips,
}

impl MapFilterMode {
    pub fn label(&self) -> &'static str {
        match self {
            MapFilterMode::Normal => "Normal",
            MapFilterMode::Wharfs => "Wharfs",
            MapFilterMode::Shipyards => "Shipyards",
            MapFilterMode::Resources => "Resources",
            MapFilterMode::FreeShips => "Free ships",
        }
    }

    pub fn all() -> [MapFilterMode; 5] {
        [
            MapFilterMode::Normal,
            MapFilterMode::Wharfs,
            MapFilterMode::Shipyards,
            MapFilterMode::Resources,
            MapFilterMode::FreeShips,
        ]
    }
}

/// One matching item inside a sector. `Entity` hits (wharf/shipyard/free-ship)
/// carry just the id — the app resolves name, code and faction. `Resource`
/// hits are already combined per ware.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterHit {
    Entity(EntityId),
    Resource(SectorResource),
}

/// Compute matched sectors → hits for `mode`. `Normal` returns an empty map,
/// which the caller treats as "no filter" (no greying, no tooltips).
///
/// Wharfs/Shipyards/FreeShips need live `world` data; without it they return
/// empty. Resources are static and work from `universe` alone.
pub fn matched_sectors(
    mode: MapFilterMode,
    universe: &Universe,
    world: Option<&World>,
) -> HashMap<SectorId, Vec<FilterHit>> {
    let mut out: HashMap<SectorId, Vec<FilterHit>> = HashMap::new();
    match mode {
        MapFilterMode::Normal => {}
        MapFilterMode::Wharfs | MapFilterMode::Shipyards => {
            let Some(world) = world else { return out };
            let set = if mode == MapFilterMode::Wharfs {
                &world.wharf_stations
            } else {
                &world.shipyard_stations
            };
            for &eid in set {
                if let Some(&sector) = world.sectors.get(&eid) {
                    out.entry(sector).or_default().push(FilterHit::Entity(eid));
                }
            }
        }
        MapFilterMode::Resources => {
            for sector in &universe.sectors {
                if let Some(areas) = universe.sector_resources.get(&sector.id) {
                    if areas.is_empty() {
                        continue;
                    }
                    let hits = combine_resources(areas)
                        .into_iter()
                        .map(FilterHit::Resource)
                        .collect();
                    out.insert(sector.id, hits);
                }
            }
        }
        MapFilterMode::FreeShips => {
            let Some(world) = world else { return out };
            let Some(&ownerless) = universe.faction_strings.get("ownerless") else {
                return out;
            };
            for (&eid, &fid) in &world.factions {
                if fid != ownerless {
                    continue;
                }
                match world.kinds.get(&eid) {
                    Some(LiveObjectKind::Station) | None => continue,
                    Some(_) => {}
                }
                if let Some(&sector) = world.sectors.get(&eid) {
                    out.entry(sector).or_default().push(FilterHit::Entity(eid));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FactionId;
    use crate::resources::ResourceTier;
    use glam::Vec3;

    fn uni() -> Universe {
        let mut u = Universe::default();
        u.sectors.push(crate::universe::Sector {
            id: SectorId(1),
            name: "S1".into(),
            faction: None,
            map_position: glam::Vec2::ZERO,
            static_objects: vec![],
            cluster_id: None,
            index_in_cluster: 0,
            cluster_sector_count: 1,
        });
        u
    }

    #[test]
    fn normal_matches_nothing() {
        let u = uni();
        assert!(matched_sectors(MapFilterMode::Normal, &u, None).is_empty());
    }

    #[test]
    fn resources_from_universe_only() {
        let mut u = uni();
        u.sector_resources.insert(
            SectorId(1),
            vec![
                SectorResource {
                    ware: "ore".into(),
                    tier: ResourceTier::Low,
                    amount: 2,
                },
                SectorResource {
                    ware: "ore".into(),
                    tier: ResourceTier::High,
                    amount: 1,
                },
            ],
        );
        let m = matched_sectors(MapFilterMode::Resources, &u, None);
        assert_eq!(m.len(), 1);
        let hits = &m[&SectorId(1)];
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0],
            FilterHit::Resource(SectorResource {
                ware: "ore".into(),
                tier: ResourceTier::High,
                amount: 3,
            })
        );
    }

    #[test]
    fn wharfs_and_free_ships_use_world() {
        let mut u = uni();
        u.faction_strings.insert("ownerless".into(), FactionId(9));
        let mut w = World::new();
        // A wharf station in sector 1.
        w.insert_entity(
            10,
            "Wharf".into(),
            LiveObjectKind::Station,
            None,
            Vec3::ZERO,
            SectorId(1),
            None,
            None,
        );
        w.wharf_stations.insert(10);
        // A free (ownerless) ship in sector 1.
        w.insert_entity(
            11,
            "Drifter".into(),
            LiveObjectKind::ShipSmall,
            Some(FactionId(9)),
            Vec3::ZERO,
            SectorId(1),
            None,
            None,
        );
        // An owned ship — must not match free-ships.
        w.insert_entity(
            12,
            "Owned".into(),
            LiveObjectKind::ShipSmall,
            Some(FactionId(1)),
            Vec3::ZERO,
            SectorId(1),
            None,
            None,
        );

        let wharfs = matched_sectors(MapFilterMode::Wharfs, &u, Some(&w));
        assert_eq!(wharfs[&SectorId(1)], vec![FilterHit::Entity(10)]);

        let free = matched_sectors(MapFilterMode::FreeShips, &u, Some(&w));
        assert_eq!(free[&SectorId(1)], vec![FilterHit::Entity(11)]);

        // No world ⇒ no live matches.
        assert!(matched_sectors(MapFilterMode::Wharfs, &u, None).is_empty());
    }
}
