use crate::ids::{FactionId, ObjectId};
use glam::Vec3;

#[derive(Debug, Clone, PartialEq)]
pub enum StaticObjectKind {
    Station,
    Gate,
    ResourceZone,
    Anomaly,
    Highway,
}

#[derive(Debug, Clone)]
pub struct StaticObject {
    pub id: ObjectId,
    pub kind: StaticObjectKind,
    pub position: Vec3,
    pub faction: Option<FactionId>,
    pub name: String,
    /// Gate orientation (pitch, yaw, roll) in degrees from zones.xml.
    pub rotation: Option<(f32, f32, f32)>,
    /// Free-form key→value properties for display.
    pub details: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_object_construction() {
        let obj = StaticObject {
            id: ObjectId(1),
            kind: StaticObjectKind::Station,
            position: Vec3::new(100.0, 0.0, -200.0),
            faction: Some(FactionId(1)),
            name: "Argon Prime Trading Station".into(),
            rotation: None,
            details: vec![],
        };
        assert_eq!(obj.kind, StaticObjectKind::Station);
        assert_eq!(obj.position.x, 100.0);
    }

    #[test]
    fn gate_has_no_faction() {
        let gate = StaticObject {
            id: ObjectId(2),
            kind: StaticObjectKind::Gate,
            position: Vec3::ZERO,
            faction: None,
            name: "Gate → Hatikvah".into(),
            rotation: None,
            details: vec![],
        };
        assert!(gate.faction.is_none());
    }
}
