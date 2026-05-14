use glam::Vec3;
use crate::ids::{ObjectId, FactionId};

#[derive(Debug, Clone, PartialEq)]
pub enum StaticObjectKind { Station, Gate, ResourceZone, Anomaly }

#[derive(Debug, Clone)]
pub struct StaticObject {
    pub id: ObjectId,
    pub kind: StaticObjectKind,
    pub position: Vec3,
    pub faction: Option<FactionId>,
    pub name: String,
}
