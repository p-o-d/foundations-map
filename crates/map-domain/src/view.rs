use crate::ids::{ObjectId, SectorId};
use crate::world::EntityId;

#[derive(Debug, Clone, PartialEq)]
pub enum ViewMode {
    UniverseMap {
        selected: Option<SectorId>,
    },
    SectorView {
        sector: SectorId,
        selected_obj: Option<ObjectId>,
        selected_entity: Option<EntityId>,
    },
}

impl ViewMode {
    pub fn initial() -> Self {
        ViewMode::UniverseMap { selected: None }
    }

    pub fn select_sector(self, sector: SectorId) -> Self {
        match self {
            ViewMode::UniverseMap { .. } => ViewMode::UniverseMap {
                selected: Some(sector),
            },
            ViewMode::SectorView { .. } => ViewMode::UniverseMap {
                selected: Some(sector),
            },
        }
    }

    pub fn open_sector_3d(self) -> Self {
        match self {
            ViewMode::UniverseMap {
                selected: Some(sector),
            } => ViewMode::SectorView {
                sector,
                selected_obj: None,
                selected_entity: None,
            },
            other => other, // no-op if no sector selected
        }
    }

    pub fn close_sector_3d(self) -> Self {
        match self {
            ViewMode::SectorView { sector, .. } => ViewMode::UniverseMap {
                selected: Some(sector),
            },
            other => other,
        }
    }

    pub fn select_object(self, obj: ObjectId) -> Self {
        match self {
            ViewMode::SectorView { sector, .. } => ViewMode::SectorView {
                sector,
                selected_obj: Some(obj),
                selected_entity: None,
            },
            other => other,
        }
    }

    pub fn select_entity(self, eid: EntityId) -> Self {
        match self {
            ViewMode::SectorView { sector, .. } => ViewMode::SectorView {
                sector,
                selected_obj: None,
                selected_entity: Some(eid),
            },
            other => other,
        }
    }

    pub fn deselect_object(self) -> Self {
        match self {
            ViewMode::SectorView {
                sector,
                selected_entity,
                ..
            } => ViewMode::SectorView {
                sector,
                selected_obj: None,
                selected_entity,
            },
            other => other,
        }
    }

    pub fn deselect_entity(self) -> Self {
        match self {
            ViewMode::SectorView {
                sector,
                selected_obj,
                ..
            } => ViewMode::SectorView {
                sector,
                selected_obj,
                selected_entity: None,
            },
            other => other,
        }
    }

    pub fn selected_entity(&self) -> Option<EntityId> {
        match self {
            ViewMode::SectorView { selected_entity, .. } => *selected_entity,
            _ => None,
        }
    }

    pub fn selected_sector(&self) -> Option<SectorId> {
        match self {
            ViewMode::UniverseMap { selected } => *selected,
            ViewMode::SectorView { sector, .. } => Some(*sector),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_universe_map_no_selection() {
        assert_eq!(
            ViewMode::initial(),
            ViewMode::UniverseMap { selected: None }
        );
    }

    #[test]
    fn select_sector_sets_selection() {
        let v = ViewMode::initial().select_sector(SectorId(1));
        assert_eq!(
            v,
            ViewMode::UniverseMap {
                selected: Some(SectorId(1))
            }
        );
    }

    #[test]
    fn open_3d_requires_selected_sector() {
        let v = ViewMode::initial().open_sector_3d();
        assert_eq!(v, ViewMode::initial()); // no-op
    }

    #[test]
    fn open_3d_with_selection_transitions_to_sector_view() {
        let v = ViewMode::initial()
            .select_sector(SectorId(5))
            .open_sector_3d();
        assert_eq!(
            v,
            ViewMode::SectorView {
                sector: SectorId(5),
                selected_obj: None,
                selected_entity: None,
            }
        );
    }

    #[test]
    fn close_3d_returns_to_map_with_sector_still_selected() {
        let v = ViewMode::initial()
            .select_sector(SectorId(5))
            .open_sector_3d()
            .close_sector_3d();
        assert_eq!(
            v,
            ViewMode::UniverseMap {
                selected: Some(SectorId(5))
            }
        );
    }

    #[test]
    fn select_object_in_sector_view() {
        let v = ViewMode::initial()
            .select_sector(SectorId(5))
            .open_sector_3d()
            .select_object(ObjectId(42));
        assert_eq!(
            v,
            ViewMode::SectorView {
                sector: SectorId(5),
                selected_obj: Some(ObjectId(42)),
                selected_entity: None,
            }
        );
    }

    #[test]
    fn deselect_object_clears_obj_keeps_sector() {
        let v = ViewMode::initial()
            .select_sector(SectorId(5))
            .open_sector_3d()
            .select_object(ObjectId(42))
            .deselect_object();
        assert_eq!(
            v,
            ViewMode::SectorView {
                sector: SectorId(5),
                selected_obj: None,
                selected_entity: None,
            }
        );
    }

    #[test]
    fn selected_sector_accessible_from_both_modes() {
        let map = ViewMode::UniverseMap {
            selected: Some(SectorId(3)),
        };
        assert_eq!(map.selected_sector(), Some(SectorId(3)));

        let view = ViewMode::SectorView {
            sector: SectorId(3),
            selected_obj: None,
            selected_entity: None,
        };
        assert_eq!(view.selected_sector(), Some(SectorId(3)));
    }

    #[test]
    fn select_entity_clears_selected_obj() {
        let v = ViewMode::SectorView {
            sector: SectorId(1),
            selected_obj: Some(ObjectId(99)),
            selected_entity: None,
        };
        let v = v.select_entity(42_u32);
        match v {
            ViewMode::SectorView {
                selected_obj,
                selected_entity,
                ..
            } => {
                assert_eq!(selected_obj, None);
                assert_eq!(selected_entity, Some(42));
            }
            _ => panic!(),
        }
    }
}
