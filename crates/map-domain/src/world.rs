use std::collections::HashMap;
use glam::Vec3;
use crate::ids::{SectorId, FactionId};

pub type EntityId = u32;

#[derive(Debug, Clone, PartialEq)]
pub enum LiveObjectKind {
    ShipSmall,
    ShipMedium,
    ShipLarge,
    ShipExtraLarge,
    Station,
}

#[derive(Debug, Clone)]
pub struct PositionUpdate {
    pub entity: EntityId,
    pub position: Vec3,
    pub sector: SectorId,
}

#[derive(Debug, Default)]
pub struct World {
    pub names:      HashMap<EntityId, String>,
    pub positions:  HashMap<EntityId, Vec3>,
    pub velocities: HashMap<EntityId, Vec3>,
    pub factions:   HashMap<EntityId, FactionId>,
    pub kinds:      HashMap<EntityId, LiveObjectKind>,
    pub sectors:    HashMap<EntityId, SectorId>,
    /// Denormalised: all entities currently in a sector. Kept in sync by update_positions.
    pub sector_idx: HashMap<SectorId, Vec<EntityId>>,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_entity(
        &mut self,
        id: EntityId,
        name: String,
        kind: LiveObjectKind,
        faction: Option<FactionId>,
        position: Vec3,
        sector: SectorId,
    ) {
        self.names.insert(id, name);
        self.kinds.insert(id, kind);
        if let Some(f) = faction {
            self.factions.insert(id, f);
        }
        self.positions.insert(id, position);
        self.sectors.insert(id, sector);
        self.sector_idx.entry(sector).or_default().push(id);
    }

    pub fn entities_in_sector(&self, sector: SectorId) -> &[EntityId] {
        self.sector_idx.get(&sector).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn update_positions(&mut self, updates: &[PositionUpdate]) {
        for upd in updates {
            let old_sector = self.sectors.get(&upd.entity).copied();

            // Remove from old sector index
            if let Some(old) = old_sector {
                if let Some(list) = self.sector_idx.get_mut(&old) {
                    list.retain(|&e| e != upd.entity);
                }
            }

            self.positions.insert(upd.entity, upd.position);
            self.sectors.insert(upd.entity, upd.sector);
            self.sector_idx.entry(upd.sector).or_default().push(upd.entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sector_a() -> SectorId { SectorId(1) }
    fn sector_b() -> SectorId { SectorId(2) }

    fn populated_world() -> World {
        let mut w = World::new();
        w.insert_entity(
            1, "Fighter Alpha".into(), LiveObjectKind::ShipSmall,
            Some(FactionId(1)), Vec3::new(100.0, 0.0, 200.0), sector_a(),
        );
        w.insert_entity(
            2, "Freighter Beta".into(), LiveObjectKind::ShipLarge,
            Some(FactionId(1)), Vec3::new(-500.0, 100.0, 0.0), sector_a(),
        );
        w.insert_entity(
            3, "Xenon Scout".into(), LiveObjectKind::ShipSmall,
            None, Vec3::new(0.0, 0.0, 0.0), sector_b(),
        );
        w
    }

    #[test]
    fn entities_in_sector_returns_correct_set() {
        let w = populated_world();
        let in_a = w.entities_in_sector(sector_a());
        assert_eq!(in_a.len(), 2);
        assert!(in_a.contains(&1));
        assert!(in_a.contains(&2));
        let in_b = w.entities_in_sector(sector_b());
        assert_eq!(in_b.len(), 1);
        assert!(in_b.contains(&3));
    }

    #[test]
    fn empty_sector_returns_empty_slice() {
        let w = World::new();
        assert_eq!(w.entities_in_sector(SectorId(99)).len(), 0);
    }

    #[test]
    fn update_positions_moves_entity_between_sectors() {
        let mut w = populated_world();
        w.update_positions(&[PositionUpdate {
            entity: 1,
            position: Vec3::new(0.0, 0.0, 0.0),
            sector: sector_b(),
        }]);
        assert_eq!(w.entities_in_sector(sector_a()).len(), 1); // only entity 2 remains
        assert!(!w.entities_in_sector(sector_a()).contains(&1));
        let in_b = w.entities_in_sector(sector_b());
        assert!(in_b.contains(&1));
        assert!(in_b.contains(&3));
    }

    #[test]
    fn update_positions_within_same_sector() {
        let mut w = populated_world();
        w.update_positions(&[PositionUpdate {
            entity: 1,
            position: Vec3::new(999.0, 0.0, 0.0),
            sector: sector_a(),
        }]);
        assert_eq!(w.positions[&1], Vec3::new(999.0, 0.0, 0.0));
        assert_eq!(w.entities_in_sector(sector_a()).len(), 2);
    }
}
