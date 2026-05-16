use glam::Vec2;
use crate::ids::{SectorId, FactionId, ClusterId};
use crate::objects::StaticObject;

#[derive(Debug, Clone, PartialEq)]
pub enum GateType {
    Standard,
    Superhighway,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub from: SectorId,
    pub to: SectorId,
    pub gate_type: GateType,
}

#[derive(Debug, Clone)]
pub struct Sector {
    pub id: SectorId,
    pub name: String,
    pub faction: Option<FactionId>,
    /// Projected from X4 galaxy 3D coords: galaxy x/z → map x/y, y discarded.
    /// This is the *cluster center* — per-sector layout offset is applied by the
    /// renderer in pixel space so it scales with `hex_r` rather than zoom.
    pub map_position: Vec2,
    pub static_objects: Vec<StaticObject>,
    pub cluster_id: Option<ClusterId>,
    pub index_in_cluster: u32,
    pub cluster_sector_count: u32,
}

#[derive(Debug, Clone)]
pub struct Cluster {
    pub id: ClusterId,
    pub name: String,
    /// Cluster center in map space (same units as Sector.map_position).
    pub map_position: Vec2,
    /// Encompassing radius in map units — covers all sectors in this cluster plus a margin.
    pub radius: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Universe {
    pub sectors: Vec<Sector>,
    pub clusters: Vec<Cluster>,
    pub connections: Vec<Connection>,
}

impl Universe {
    pub fn sector(&self, id: SectorId) -> Option<&Sector> {
        self.sectors.iter().find(|s| s.id == id)
    }

    pub fn connections_for(&self, id: SectorId) -> Vec<&Connection> {
        self.connections
            .iter()
            .filter(|c| c.from == id || c.to == id)
            .collect()
    }

    pub fn neighbour_ids(&self, id: SectorId) -> Vec<SectorId> {
        self.connections_for(id)
            .iter()
            .map(|c| if c.from == id { c.to } else { c.from })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_universe() -> Universe {
        let a = SectorId(1);
        let b = SectorId(2);
        Universe {
            sectors: vec![
                Sector {
                    id: a,
                    name: "Argon Prime".into(),
                    faction: Some(FactionId(1)),
                    map_position: Vec2::new(0.0, 0.0),
                    static_objects: vec![],
                    cluster_id: None,
                    index_in_cluster: 0,
                    cluster_sector_count: 1,
                },
                Sector {
                    id: b,
                    name: "Hatikvah's Choice I".into(),
                    faction: Some(FactionId(2)),
                    map_position: Vec2::new(1.0, 0.5),
                    static_objects: vec![],
                    cluster_id: None,
                    index_in_cluster: 0,
                    cluster_sector_count: 1,
                },
            ],
            clusters: vec![],
            connections: vec![Connection {
                from: a,
                to: b,
                gate_type: GateType::Standard,
            }],
        }
    }

    #[test]
    fn sector_lookup_by_id() {
        let u = make_universe();
        assert_eq!(u.sector(SectorId(1)).unwrap().name, "Argon Prime");
        assert!(u.sector(SectorId(99)).is_none());
    }

    #[test]
    fn connections_for_returns_both_sides() {
        let u = make_universe();
        assert_eq!(u.connections_for(SectorId(1)).len(), 1);
        assert_eq!(u.connections_for(SectorId(2)).len(), 1);
        assert_eq!(u.connections_for(SectorId(99)).len(), 0);
    }

    #[test]
    fn neighbour_ids_from_both_directions() {
        let u = make_universe();
        assert_eq!(u.neighbour_ids(SectorId(1)), vec![SectorId(2)]);
        assert_eq!(u.neighbour_ids(SectorId(2)), vec![SectorId(1)]);
    }
}
