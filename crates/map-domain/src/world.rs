use crate::ids::{FactionId, SectorId};
use glam::Vec3;
use std::collections::{HashMap, HashSet};

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
    pub names: HashMap<EntityId, String>,
    pub positions: HashMap<EntityId, Vec3>,
    pub velocities: HashMap<EntityId, Vec3>,
    pub factions: HashMap<EntityId, FactionId>,
    pub kinds: HashMap<EntityId, LiveObjectKind>,
    pub sectors: HashMap<EntityId, SectorId>,
    /// Denormalised: all entities currently in a sector. Kept in sync by update_positions.
    pub sector_idx: HashMap<SectorId, Vec<EntityId>>,
    pub parents: HashMap<EntityId, EntityId>,
    pub children: HashMap<EntityId, Vec<EntityId>>,
    pub codes: HashMap<EntityId, String>,
    pub trade_offers: HashMap<EntityId, Vec<TradeOffer>>,
    /// Raw `name=` / `basename=` value from the save. Either a `{page,id}` ref,
    /// a compound form `{p,t} ({p,t})`, or a literal string (player-renamed ships).
    /// Resolved at display time so it picks up the current locale.
    pub display_name_refs: HashMap<EntityId, String>,
    /// Per-station production-module macro (lowercase). Captured from the
    /// first `<entry macro="prod_*"/>` inside `<construction><sequence>`.
    /// Resolved at display time so the station shows e.g. "Microchip
    /// Production" instead of the generic basename "Factory".
    pub production_modules: HashMap<EntityId, String>,
    /// Stations carrying a wharf build-module (builds S/M ships). Drives the
    /// "Wharfs" map filter.
    pub wharf_stations: HashSet<EntityId>,
    /// Stations carrying a shipyard build-module (builds L/XL ships / carriers).
    /// Drives the "Shipyards" map filter.
    pub shipyard_stations: HashSet<EntityId>,
    /// Per-entity `nameindex` attribute from the save (e.g. 1, 2, …). The game
    /// renders this as a trailing roman numeral on generated names ("… I").
    /// Absent for entities the game didn't number.
    pub name_index: HashMap<EntityId, u32>,
    /// Per-ship `job` id from the save (lowercase, e.g.
    /// "argon_construction_vessel_xl_focused"). NPC ships are prefixed in-game
    /// with their job's display name; resolved via `Universe.job_names`.
    pub entity_jobs: HashMap<EntityId, String>,
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
        parent: Option<EntityId>,
        code: Option<String>,
    ) {
        self.names.insert(id, name);
        self.kinds.insert(id, kind);
        if let Some(f) = faction {
            self.factions.insert(id, f);
        }
        self.positions.insert(id, position);
        self.sectors.insert(id, sector);
        self.sector_idx.entry(sector).or_default().push(id);
        if let Some(p) = parent {
            self.parents.insert(id, p);
            self.children.entry(p).or_default().push(id);
        }
        if let Some(c) = code {
            self.codes.insert(id, c);
        }
    }

    pub fn parent_of(&self, id: EntityId) -> Option<EntityId> {
        self.parents.get(&id).copied()
    }

    pub fn children_of(&self, id: EntityId) -> &[EntityId] {
        self.children.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn trade_offers_of(&self, id: EntityId) -> &[TradeOffer] {
        self.trade_offers.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn entities_in_sector(&self, sector: SectorId) -> &[EntityId] {
        self.sector_idx
            .get(&sector)
            .map(Vec::as_slice)
            .unwrap_or(&[])
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
            self.sector_idx
                .entry(upd.sector)
                .or_default()
                .push(upd.entity);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeDirection {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeOffer {
    pub ware_id: String,
    pub direction: TradeDirection,
    pub price: i64,
    pub amount: i64,
    pub desired: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sector_a() -> SectorId {
        SectorId(1)
    }
    fn sector_b() -> SectorId {
        SectorId(2)
    }

    fn populated_world() -> World {
        let mut w = World::new();
        w.insert_entity(
            1,
            "Fighter Alpha".into(),
            LiveObjectKind::ShipSmall,
            Some(FactionId(1)),
            Vec3::new(100.0, 0.0, 200.0),
            sector_a(),
            None,
            None,
        );
        w.insert_entity(
            2,
            "Freighter Beta".into(),
            LiveObjectKind::ShipLarge,
            Some(FactionId(1)),
            Vec3::new(-500.0, 100.0, 0.0),
            sector_a(),
            None,
            None,
        );
        w.insert_entity(
            3,
            "Xenon Scout".into(),
            LiveObjectKind::ShipSmall,
            None,
            Vec3::new(0.0, 0.0, 0.0),
            sector_b(),
            None,
            None,
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

    #[test]
    fn parent_child_links_track_correctly() {
        let mut w = World::new();
        w.insert_entity(
            1,
            "station".into(),
            LiveObjectKind::Station,
            None,
            Vec3::ZERO,
            sector_a(),
            None,
            Some("YIB-1".into()),
        );
        w.insert_entity(
            2,
            "drone".into(),
            LiveObjectKind::ShipSmall,
            None,
            Vec3::ZERO,
            sector_a(),
            Some(1),
            None,
        );
        assert_eq!(w.parent_of(2), Some(1));
        assert_eq!(w.children_of(1), &[2]);
        assert_eq!(w.parent_of(1), None);
        assert_eq!(w.codes.get(&1).map(String::as_str), Some("YIB-1"));
    }

    #[test]
    fn display_name_refs_round_trip() {
        let mut w = World::new();
        w.display_name_refs.insert(0x10, "{20101,122701}".into());
        w.display_name_refs.insert(0x11, "My Best Ship".into());
        assert_eq!(
            w.display_name_refs.get(&0x10).map(String::as_str),
            Some("{20101,122701}")
        );
        assert_eq!(
            w.display_name_refs.get(&0x11).map(String::as_str),
            Some("My Best Ship")
        );
        assert!(w.display_name_refs.get(&0x99).is_none());
    }

    #[test]
    fn production_modules_round_trip() {
        let mut w = World::new();
        w.production_modules
            .insert(0x100, "prod_gen_microchips_macro".into());
        assert_eq!(
            w.production_modules.get(&0x100).map(String::as_str),
            Some("prod_gen_microchips_macro")
        );
    }

    #[test]
    fn trade_offers_can_be_inserted_and_looked_up() {
        use crate::world::{TradeDirection, TradeOffer};
        let mut w = World::new();
        w.trade_offers.insert(
            42,
            vec![
                TradeOffer {
                    ware_id: "energycells".into(),
                    direction: TradeDirection::Buy,
                    price: 1092,
                    amount: 0,
                    desired: 1200,
                },
                TradeOffer {
                    ware_id: "medicalsupplies".into(),
                    direction: TradeDirection::Sell,
                    price: 7174,
                    amount: 6299,
                    desired: 6299,
                },
            ],
        );
        let offers = w.trade_offers_of(42);
        assert_eq!(offers.len(), 2);
        assert_eq!(offers[0].direction, TradeDirection::Buy);
        assert_eq!(offers[1].ware_id, "medicalsupplies");
        assert!(w.trade_offers_of(999).is_empty());
    }
}

/// Metadata for a single X4 save-game snapshot.
#[derive(Debug, Clone)]
pub struct SnapshotMeta {
    pub path: std::path::PathBuf,
    pub mtime: std::time::SystemTime,
    pub game_time_seconds: f32,
    pub player_money: u64,
    pub player_location_name: String,
    /// `<game version="X" build="Y">` joined as `"X.Y"`. Empty if unknown.
    pub game_version: String,
}

#[cfg(test)]
mod snapshot_meta_tests {
    use super::SnapshotMeta;

    #[test]
    fn snapshot_meta_construction() {
        let m = SnapshotMeta {
            path: "/tmp/save.xml.gz".into(),
            mtime: std::time::UNIX_EPOCH,
            game_time_seconds: 1734.285,
            player_money: 40000,
            player_location_name: "Argon Prime".into(),
            game_version: "800.580735".into(),
        };
        assert_eq!(m.player_money, 40000);
        assert!((m.game_time_seconds - 1734.285).abs() < 1e-3);
    }
}
