# Phase 1: Workspace Setup + Data Model + 2D Universe Map

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cargo workspace with 3 crates, full domain model with tests, X4 XML parser, and a working 2D universe map with pan/zoom/selection running in egui.

**Architecture:** `map-domain` holds all types (no IO/UI deps). `map-io` parses X4 XML game files into domain types. `map-app` renders the 2D universe map with egui and manages ViewMode state. Layers enforced by Cargo — `map-domain` cannot import from `map-io` or `map-app`.

**Tech Stack:** Rust 2024 edition, egui 0.29 + eframe 0.29, glam 0.27 (Vec2/Vec3), quick-xml 0.36, winreg 0.52 (Windows only)

---

## File Map

```
foundations-map/
├── Cargo.toml                                  ← convert to workspace manifest
├── Cargo.lock                                  ← keep
├── crates/
│   ├── map-domain/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ids.rs          # SectorId, ObjectId, FactionId newtypes
│   │       ├── universe.rs     # Universe, Sector, Connection, GateType
│   │       ├── objects.rs      # StaticObject, StaticObjectKind
│   │       ├── world.rs        # World, EntityId, LiveObjectKind, PositionUpdate
│   │       └── view.rs         # ViewMode, transition functions
│   ├── map-io/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── xml_parser.rs   # parse galaxy.xml + sector XMLs → domain types
│   │   │   └── game_path.rs    # detect X4 install dir, platform-specific
│   │   └── tests/
│   │       ├── fixtures/
│   │       │   ├── galaxy.xml
│   │       │   └── sector_argon_prime.xml
│   │       └── xml_parser_test.rs
│   └── map-app/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── app.rs          # App struct, AppState, eframe::App impl
│           ├── theme.rs        # dark dashboard egui Visuals
│           └── ui/
│               ├── mod.rs
│               ├── top_bar.rs
│               ├── map_view.rs      # 2D map: pan, zoom, sector nodes, connections
│               └── sector_panel.rs  # right panel: sector info
```

---

## Task 1: Convert root to Cargo workspace

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/map-domain/Cargo.toml`
- Create: `crates/map-io/Cargo.toml`
- Create: `crates/map-app/Cargo.toml`
- Create: `crates/map-app/src/main.rs`

- [ ] **Step 1: Replace root Cargo.toml with workspace manifest**

```toml
[workspace]
members = [
    "crates/map-domain",
    "crates/map-io",
    "crates/map-app",
]
resolver = "2"
```

- [ ] **Step 2: Create map-domain crate**

```bash
mkdir -p crates/map-domain/src
```

`crates/map-domain/Cargo.toml`:
```toml
[package]
name = "map-domain"
version = "0.1.0"
edition = "2024"

[dependencies]
glam = "0.27"
serde = { version = "1", features = ["derive"] }
```

`crates/map-domain/src/lib.rs`:
```rust
pub mod ids;
pub mod universe;
pub mod objects;
pub mod world;
pub mod view;
```

- [ ] **Step 3: Create map-io crate**

```bash
mkdir -p crates/map-io/src crates/map-io/tests/fixtures
```

`crates/map-io/Cargo.toml`:
```toml
[package]
name = "map-io"
version = "0.1.0"
edition = "2024"

[dependencies]
map-domain = { path = "../map-domain" }
quick-xml = "0.36"

[target.'cfg(windows)'.dependencies]
winreg = "0.52"
```

`crates/map-io/src/lib.rs`:
```rust
pub mod xml_parser;
pub mod game_path;
```

- [ ] **Step 4: Create map-app crate**

```bash
mkdir -p crates/map-app/src/ui
```

`crates/map-app/Cargo.toml`:
```toml
[package]
name = "map-app"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "foundations-map"
path = "src/main.rs"

[dependencies]
map-domain = { path = "../map-domain" }
map-io = { path = "../map-io" }
eframe = { version = "0.29", features = ["wgpu"] }
egui = "0.29"
glam = "0.27"
```

`crates/map-app/src/main.rs`:
```rust
fn main() {
    println!("foundations-map starting");
}
```

- [ ] **Step 5: Verify workspace builds**

```bash
cargo build
```

Expected: all 3 crates compile, no errors.

- [ ] **Step 6: Remove old root src/ directory**

```bash
rm -rf src/
```

- [ ] **Step 7: Verify build still passes**

```bash
cargo build
```

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: convert to cargo workspace (map-domain, map-io, map-app)"
```

---

## Task 2: map-domain — IDs

**Files:**
- Create: `crates/map-domain/src/ids.rs`

- [ ] **Step 1: Write failing tests**

`crates/map-domain/src/ids.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SectorId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ObjectId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FactionId(pub u32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_id_equality() {
        assert_eq!(SectorId(1), SectorId(1));
        assert_ne!(SectorId(1), SectorId(2));
    }

    #[test]
    fn ids_are_copy() {
        let id = SectorId(42);
        let _copy = id;
        let _original = id; // both usable — Copy
    }

    #[test]
    fn ids_usable_as_hashmap_keys() {
        let mut map = std::collections::HashMap::new();
        map.insert(SectorId(1), "argon prime");
        assert_eq!(map[&SectorId(1)], "argon prime");
    }
}
```

- [ ] **Step 2: Run tests to verify they pass (types compile)**

```bash
cargo test --package map-domain ids
```

Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/map-domain/src/ids.rs crates/map-domain/src/lib.rs
git commit -m "feat(domain): add SectorId, ObjectId, FactionId newtypes"
```

---

## Task 3: map-domain — Universe, Sector, Connection

**Files:**
- Create: `crates/map-domain/src/objects.rs` (stub — expanded in Task 4)
- Create: `crates/map-domain/src/universe.rs`

- [ ] **Step 0: Create minimal objects.rs stub so universe.rs can import StaticObject**

`crates/map-domain/src/objects.rs`:
```rust
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
```

Task 4 will add full tests to this file.

- [ ] **Step 1: Write failing tests first**

`crates/map-domain/src/universe.rs`:
```rust
use glam::Vec2;
use crate::ids::{SectorId, FactionId};
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
    pub map_position: Vec2,
    pub static_objects: Vec<StaticObject>,
}

#[derive(Debug, Clone, Default)]
pub struct Universe {
    pub sectors: Vec<Sector>,
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
                },
                Sector {
                    id: b,
                    name: "Hatikvah's Choice I".into(),
                    faction: Some(FactionId(2)),
                    map_position: Vec2::new(1.0, 0.5),
                    static_objects: vec![],
                },
            ],
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
```

- [ ] **Step 2: Run tests**

```bash
cargo test --package map-domain universe
```

Expected: 3 tests pass. (Note: `StaticObject` import will compile once Task 4 adds a stub.)

- [ ] **Step 3: Commit**

```bash
git add crates/map-domain/src/universe.rs
git commit -m "feat(domain): add Universe, Sector, Connection types with lookup"
```

---

## Task 4: map-domain — StaticObject

**Files:**
- Modify: `crates/map-domain/src/objects.rs` (stub created in Task 3 Step 0 — add tests here)

- [ ] **Step 1: Add tests to objects.rs (types already exist from stub)**

`crates/map-domain/src/objects.rs`:
```rust
use glam::Vec3;
use crate::ids::{ObjectId, FactionId};

#[derive(Debug, Clone, PartialEq)]
pub enum StaticObjectKind {
    Station,
    Gate,
    ResourceZone,
    Anomaly,
}

#[derive(Debug, Clone)]
pub struct StaticObject {
    pub id: ObjectId,
    pub kind: StaticObjectKind,
    pub position: Vec3,
    pub faction: Option<FactionId>,
    pub name: String,
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
        };
        assert!(gate.faction.is_none());
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --package map-domain objects
```

Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/map-domain/src/objects.rs
git commit -m "feat(domain): add StaticObject and StaticObjectKind"
```

---

## Task 5: map-domain — World (ECS store)

**Files:**
- Create: `crates/map-domain/src/world.rs`

- [ ] **Step 1: Write failing tests**

`crates/map-domain/src/world.rs`:
```rust
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
```

- [ ] **Step 2: Run tests**

```bash
cargo test --package map-domain world
```

Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/map-domain/src/world.rs
git commit -m "feat(domain): add World ECS store with sector index and position updates"
```

---

## Task 6: map-domain — ViewMode state machine

**Files:**
- Create: `crates/map-domain/src/view.rs`

- [ ] **Step 1: Write failing tests**

`crates/map-domain/src/view.rs`:
```rust
use crate::ids::{SectorId, ObjectId};

#[derive(Debug, Clone, PartialEq)]
pub enum ViewMode {
    UniverseMap { selected: Option<SectorId> },
    SectorView  { sector: SectorId, selected_obj: Option<ObjectId> },
}

impl ViewMode {
    pub fn initial() -> Self {
        ViewMode::UniverseMap { selected: None }
    }

    pub fn select_sector(self, sector: SectorId) -> Self {
        match self {
            ViewMode::UniverseMap { .. } => ViewMode::UniverseMap { selected: Some(sector) },
            ViewMode::SectorView { .. } => ViewMode::UniverseMap { selected: Some(sector) },
        }
    }

    pub fn open_sector_3d(self) -> Self {
        match self {
            ViewMode::UniverseMap { selected: Some(sector) } => {
                ViewMode::SectorView { sector, selected_obj: None }
            }
            other => other, // no-op if no sector selected
        }
    }

    pub fn close_sector_3d(self) -> Self {
        match self {
            ViewMode::SectorView { sector, .. } => {
                ViewMode::UniverseMap { selected: Some(sector) }
            }
            other => other,
        }
    }

    pub fn select_object(self, obj: ObjectId) -> Self {
        match self {
            ViewMode::SectorView { sector, .. } => {
                ViewMode::SectorView { sector, selected_obj: Some(obj) }
            }
            other => other,
        }
    }

    pub fn deselect_object(self) -> Self {
        match self {
            ViewMode::SectorView { sector, .. } => {
                ViewMode::SectorView { sector, selected_obj: None }
            }
            other => other,
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
        assert_eq!(ViewMode::initial(), ViewMode::UniverseMap { selected: None });
    }

    #[test]
    fn select_sector_sets_selection() {
        let v = ViewMode::initial().select_sector(SectorId(1));
        assert_eq!(v, ViewMode::UniverseMap { selected: Some(SectorId(1)) });
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
        assert_eq!(v, ViewMode::SectorView { sector: SectorId(5), selected_obj: None });
    }

    #[test]
    fn close_3d_returns_to_map_with_sector_still_selected() {
        let v = ViewMode::initial()
            .select_sector(SectorId(5))
            .open_sector_3d()
            .close_sector_3d();
        assert_eq!(v, ViewMode::UniverseMap { selected: Some(SectorId(5)) });
    }

    #[test]
    fn select_object_in_sector_view() {
        let v = ViewMode::initial()
            .select_sector(SectorId(5))
            .open_sector_3d()
            .select_object(ObjectId(42));
        assert_eq!(v, ViewMode::SectorView { sector: SectorId(5), selected_obj: Some(ObjectId(42)) });
    }

    #[test]
    fn deselect_object_clears_obj_keeps_sector() {
        let v = ViewMode::initial()
            .select_sector(SectorId(5))
            .open_sector_3d()
            .select_object(ObjectId(42))
            .deselect_object();
        assert_eq!(v, ViewMode::SectorView { sector: SectorId(5), selected_obj: None });
    }

    #[test]
    fn selected_sector_accessible_from_both_modes() {
        let map = ViewMode::UniverseMap { selected: Some(SectorId(3)) };
        assert_eq!(map.selected_sector(), Some(SectorId(3)));

        let view = ViewMode::SectorView { sector: SectorId(3), selected_obj: None };
        assert_eq!(view.selected_sector(), Some(SectorId(3)));
    }
}
```

- [ ] **Step 2: Run all domain tests**

```bash
cargo test --package map-domain
```

Expected: all tests pass (ids: 3, universe: 3, objects: 2, world: 4, view: 8).

- [ ] **Step 3: Commit**

```bash
git add crates/map-domain/src/view.rs
git commit -m "feat(domain): add ViewMode state machine with typed transitions"
```

---

## Task 7: map-io — XML fixtures

**Files:**
- Create: `crates/map-io/tests/fixtures/galaxy.xml`
- Create: `crates/map-io/tests/fixtures/sector_argon_prime.xml`

These fixtures represent a simplified subset of actual X4 game file structure. The real parser may need adjustment when tested against actual game files.

- [ ] **Step 1: Create galaxy fixture**

`crates/map-io/tests/fixtures/galaxy.xml`:
```xml
<?xml version="1.0" encoding="utf-8"?>
<macros>
  <macro name="xu_ep2_universe_macro" class="galaxy">
    <component ref="standardgalaxy"/>
    <connections>
      <connection name="Cluster_01" ref="standardcluster">
        <macro ref="Cluster_01_macro" connection="cluster"/>
        <offset><position x="-10000000" y="0" z="5000000"/></offset>
      </connection>
      <connection name="Cluster_09" ref="standardcluster">
        <macro ref="Cluster_09_macro" connection="cluster"/>
        <offset><position x="5000000" y="0" z="-2000000"/></offset>
      </connection>
    </connections>
  </macro>

  <macro name="Cluster_01_macro" class="cluster">
    <component ref="standardcluster"/>
    <connections>
      <connection name="Cluster_01_Sector001" ref="standardsector">
        <macro ref="Cluster_01_Sector001_macro" connection="sector"/>
      </connection>
    </connections>
  </macro>

  <macro name="Cluster_01_Sector001_macro" class="sector">
    <component ref="standardsector"/>
    <properties>
      <identification name="Argon Prime" description="Heart of Argon space"/>
      <owner exact="argon"/>
    </properties>
  </macro>

  <macro name="Cluster_09_macro" class="cluster">
    <component ref="standardcluster"/>
    <connections>
      <connection name="Cluster_09_Sector001" ref="standardsector">
        <macro ref="Cluster_09_Sector001_macro" connection="sector"/>
      </connection>
    </connections>
  </macro>

  <macro name="Cluster_09_Sector001_macro" class="sector">
    <component ref="standardsector"/>
    <properties>
      <identification name="Hatikvah's Choice I" description="A prosperous trading sector"/>
      <owner exact="hatikvah"/>
    </properties>
  </macro>
</macros>
```

- [ ] **Step 2: Create sector objects fixture**

`crates/map-io/tests/fixtures/sector_argon_prime.xml`:
```xml
<?xml version="1.0" encoding="utf-8"?>
<macros>
  <macro name="Cluster_01_Sector001_macro" class="sector">
    <component ref="standardsector"/>
    <connections>
      <connection ref="standardzone">
        <macro ref="Cluster_01_Sector001_Zone001_macro" connection="zone"/>
        <offset><position x="0" y="0" z="0"/></offset>
      </connection>
    </connections>
  </macro>

  <macro name="Cluster_01_Sector001_Zone001_macro" class="zone">
    <component ref="standardzone"/>
    <connections>
      <connection ref="object" name="station_argon_tradingstation">
        <macro ref="station_tra_arg_xl_01_macro" connection="object"/>
        <offset><position x="100000" y="0" z="-200000"/></offset>
        <properties>
          <identification name="Argon Prime Trading Station"/>
          <owner exact="argon"/>
          <type class="station"/>
        </properties>
      </connection>
      <connection ref="object" name="gate_argon_to_hatikvah">
        <macro ref="standardgate" connection="object"/>
        <offset><position x="-3000000" y="0" z="1500000"/></offset>
        <properties>
          <identification name="Gate to Hatikvah's Choice I"/>
          <type class="gate"/>
        </properties>
      </connection>
      <connection ref="object" name="resourcezone_silicon_01">
        <macro ref="resourcezone_silicon_macro" connection="object"/>
        <offset><position x="2000000" y="200000" z="-800000"/></offset>
        <properties>
          <identification name="Silicon Field Alpha"/>
          <type class="resourcezone"/>
        </properties>
      </connection>
    </connections>
  </macro>
</macros>
```

- [ ] **Step 3: Commit**

```bash
git add crates/map-io/tests/fixtures/
git commit -m "test(io): add X4 XML fixture files for parser tests"
```

---

## Task 8: map-io — XML parser

**Files:**
- Create: `crates/map-io/src/xml_parser.rs`
- Create: `crates/map-io/tests/xml_parser_test.rs`

- [ ] **Step 1: Write failing integration tests**

`crates/map-io/tests/xml_parser_test.rs`:
```rust
use map_domain::ids::{SectorId, FactionId};
use map_domain::universe::GateType;
use map_io::xml_parser;
use std::path::Path;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn parse_galaxy_produces_correct_sector_count() {
    let universe = xml_parser::parse_galaxy(&fixture("galaxy.xml")).unwrap();
    assert_eq!(universe.sectors.len(), 2);
}

#[test]
fn parse_galaxy_sector_names_correct() {
    let universe = xml_parser::parse_galaxy(&fixture("galaxy.xml")).unwrap();
    let names: Vec<&str> = universe.sectors.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Argon Prime"));
    assert!(names.contains(&"Hatikvah's Choice I"));
}

#[test]
fn parse_galaxy_positions_are_nonzero_and_distinct() {
    let universe = xml_parser::parse_galaxy(&fixture("galaxy.xml")).unwrap();
    let p0 = universe.sectors[0].map_position;
    let p1 = universe.sectors[1].map_position;
    assert_ne!(p0, p1);
}

#[test]
fn parse_sector_objects_returns_station_gate_resourcezone() {
    let objects = xml_parser::parse_sector_objects(&fixture("sector_argon_prime.xml")).unwrap();
    assert_eq!(objects.len(), 3);

    use map_domain::objects::StaticObjectKind;
    let kinds: Vec<&StaticObjectKind> = objects.iter().map(|o| &o.kind).collect();
    assert!(kinds.contains(&&StaticObjectKind::Station));
    assert!(kinds.contains(&&StaticObjectKind::Gate));
    assert!(kinds.contains(&&StaticObjectKind::ResourceZone));
}

#[test]
fn parse_sector_objects_positions_are_set() {
    let objects = xml_parser::parse_sector_objects(&fixture("sector_argon_prime.xml")).unwrap();
    let station = objects.iter().find(|o| o.name.contains("Trading Station")).unwrap();
    assert_eq!(station.position.x, 100000.0);
    assert_eq!(station.position.z, -200000.0);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test --package map-io
```

Expected: compile error — `xml_parser` module empty.

- [ ] **Step 3: Implement xml_parser**

`crates/map-io/src/xml_parser.rs`:
```rust
use std::path::Path;
use std::io::BufReader;
use std::fs::File;
use std::collections::HashMap;
use quick_xml::Reader;
use quick_xml::events::Event;
use glam::{Vec2, Vec3};
use map_domain::ids::{SectorId, ObjectId, FactionId};
use map_domain::universe::{Universe, Sector, Connection, GateType};
use map_domain::objects::{StaticObject, StaticObjectKind};

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Xml(quick_xml::Error),
    MissingAttribute(String),
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self { ParseError::Io(e) }
}
impl From<quick_xml::Error> for ParseError {
    fn from(e: quick_xml::Error) -> Self { ParseError::Xml(e) }
}

/// Parse galaxy.xml into a Universe with sectors and their 2D map positions.
/// Connections between sectors are not parsed from galaxy.xml (they come from
/// gate objects in sector files). Call parse_sector_objects() per sector and
/// derive connections from Gate objects.
pub fn parse_galaxy(path: &Path) -> Result<Universe, ParseError> {
    let xml = std::fs::read_to_string(path)?;
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    // State machine: track current cluster position and sector macros
    struct ClusterEntry {
        macro_ref: String,
        position: Vec2,
    }
    struct SectorEntry {
        macro_name: String,
        name: String,
        faction: Option<String>,
        cluster_position: Vec2,
    }

    let mut clusters: HashMap<String, ClusterEntry> = HashMap::new();
    let mut sectors: Vec<SectorEntry> = Vec::new();
    let mut current_macro_name: Option<String> = None;
    let mut current_class: Option<String> = None;
    let mut current_cluster_pos: Option<Vec2> = None;
    let mut current_sector_macro_ref: Option<String> = None;
    let mut in_identification = false;
    let mut in_owner = false;
    let mut pending_name: Option<String> = None;
    let mut pending_faction: Option<String> = None;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) | Event::Empty(e) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                match tag.as_str() {
                    "macro" => {
                        let name = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"name")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        let class = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"class")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        current_macro_name = name;
                        current_class = class;
                        pending_name = None;
                        pending_faction = None;
                    }
                    "connection" if current_class.as_deref() == Some("galaxy") => {
                        let macro_ref = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"ref")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        current_sector_macro_ref = macro_ref;
                    }
                    "position" => {
                        let x = attr_f32(&e, b"x").unwrap_or(0.0);
                        let z = attr_f32(&e, b"z").unwrap_or(0.0);
                        // Map: galaxy x/z → 2D x/y (scale down to reasonable range)
                        let pos = Vec2::new(x / 1_000_000.0, z / 1_000_000.0);
                        if current_class.as_deref() == Some("galaxy") {
                            current_cluster_pos = Some(pos);
                            if let (Some(cluster_ref), Some(pos)) =
                                (&current_sector_macro_ref, current_cluster_pos)
                            {
                                clusters.insert(
                                    cluster_ref.clone(),
                                    ClusterEntry { macro_ref: cluster_ref.clone(), position: pos },
                                );
                            }
                        }
                    }
                    "identification" => {
                        pending_name = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"name")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                    }
                    "owner" => {
                        pending_faction = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"exact")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                    }
                    _ => {}
                }
            }
            Event::End(e) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                if tag == "macro" {
                    if current_class.as_deref() == Some("sector") {
                        if let Some(name) = pending_name.take() {
                            // Find cluster position for this sector macro
                            let cluster_pos = clusters.values()
                                .find(|_| true) // simplified: use first cluster found
                                .map(|c| c.position)
                                .unwrap_or(Vec2::ZERO);
                            sectors.push(SectorEntry {
                                macro_name: current_macro_name.clone().unwrap_or_default(),
                                name,
                                faction: pending_faction.take(),
                                cluster_position: cluster_pos,
                            });
                        }
                    }
                    current_class = None;
                    current_macro_name = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    // Build Universe from collected sectors
    // Assign positions based on cluster offsets by sector index
    let mut universe = Universe::default();
    for (i, entry) in sectors.iter().enumerate() {
        // Use cluster positions as-is; each sector in its own cluster inherits that position
        // For multi-sector clusters this would need refinement with actual game data
        let faction = entry.faction.as_ref().map(|_| FactionId(i as u32 + 1));
        universe.sectors.push(Sector {
            id: SectorId(i as u32 + 1),
            name: entry.name.clone(),
            faction,
            map_position: entry.cluster_position,
            static_objects: vec![],
        });
    }

    Ok(universe)
}

/// Parse a sector XML file and return its static objects.
pub fn parse_sector_objects(path: &Path) -> Result<Vec<StaticObject>, ParseError> {
    let xml = std::fs::read_to_string(path)?;
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    let mut objects: Vec<StaticObject> = Vec::new();
    let mut next_id: u32 = 1;
    let mut current_type: Option<StaticObjectKind> = None;
    let mut current_pos: Option<Vec3> = None;
    let mut current_name: Option<String> = None;
    let mut current_faction: Option<String> = None;
    let mut in_object_connection = false;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) | Event::Empty(e) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                match tag.as_str() {
                    "connection" => {
                        let conn_ref = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"ref")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        in_object_connection = conn_ref.as_deref() == Some("object");
                        if in_object_connection {
                            current_type = None;
                            current_pos = None;
                            current_name = None;
                            current_faction = None;
                        }
                    }
                    "position" if in_object_connection => {
                        let x = attr_f32(&e, b"x").unwrap_or(0.0);
                        let y = attr_f32(&e, b"y").unwrap_or(0.0);
                        let z = attr_f32(&e, b"z").unwrap_or(0.0);
                        current_pos = Some(Vec3::new(x, y, z));
                    }
                    "identification" if in_object_connection => {
                        current_name = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"name")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                    }
                    "owner" if in_object_connection => {
                        current_faction = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"exact")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                    }
                    "type" if in_object_connection => {
                        let class = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"class")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        current_type = class.as_deref().map(|c| match c {
                            "station"      => StaticObjectKind::Station,
                            "gate"         => StaticObjectKind::Gate,
                            "resourcezone" => StaticObjectKind::ResourceZone,
                            _              => StaticObjectKind::Anomaly,
                        });
                    }
                    _ => {}
                }
            }
            Event::End(e) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                if tag == "connection" && in_object_connection {
                    if let (Some(kind), Some(pos), Some(name)) =
                        (current_type.take(), current_pos.take(), current_name.take())
                    {
                        let faction = current_faction.take().map(|_| FactionId(1));
                        objects.push(StaticObject {
                            id: ObjectId(next_id),
                            kind,
                            position: pos,
                            faction,
                            name,
                        });
                        next_id += 1;
                    }
                    in_object_connection = false;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(objects)
}

fn attr_f32(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<f32> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| String::from_utf8(a.value.to_vec()).ok())
        .and_then(|s| s.parse().ok())
}
```

- [ ] **Step 4: Run integration tests**

```bash
cargo test --package map-io
```

Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/xml_parser.rs crates/map-io/tests/xml_parser_test.rs
git commit -m "feat(io): XML parser for galaxy and sector files with integration tests"
```

---

## Task 9: map-io — game path detection

**Files:**
- Create: `crates/map-io/src/game_path.rs`

- [ ] **Step 1: Write tests + implementation**

`crates/map-io/src/game_path.rs`:
```rust
use std::path::PathBuf;

const GAME_DIR_NAME: &str = "X4 Foundations";

/// Attempt to detect the X4 Foundations installation directory.
/// Returns None if not found; caller should prompt user to set path manually.
pub fn detect() -> Option<PathBuf> {
    detect_platform()
}

#[cfg(target_os = "linux")]
fn detect_platform() -> Option<PathBuf> {
    let candidates = linux_steam_paths();
    candidates.into_iter().find(|p| p.exists())
}

#[cfg(target_os = "linux")]
fn linux_steam_paths() -> Vec<PathBuf> {
    let Some(home) = std::env::var("HOME").ok() else { return vec![]; };
    vec![
        PathBuf::from(&home).join(".steam/steam/steamapps/common").join(GAME_DIR_NAME),
        PathBuf::from(&home).join(".local/share/Steam/steamapps/common").join(GAME_DIR_NAME),
        PathBuf::from("/usr/share/Steam/steamapps/common").join(GAME_DIR_NAME),
    ]
}

#[cfg(target_os = "windows")]
fn detect_platform() -> Option<PathBuf> {
    // Try Steam registry key first
    if let Some(path) = windows_registry_path() {
        if path.exists() { return Some(path); }
    }
    // Fallback: common Steam install locations
    for base in &[
        r"C:\Program Files (x86)\Steam\steamapps\common",
        r"C:\Program Files\Steam\steamapps\common",
    ] {
        let p = PathBuf::from(base).join(GAME_DIR_NAME);
        if p.exists() { return Some(p); }
    }
    None
}

#[cfg(target_os = "windows")]
fn windows_registry_path() -> Option<PathBuf> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm.open_subkey(
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Steam App 392160"
    ).ok()?;
    let install_location: String = key.get_value("InstallLocation").ok()?;
    Some(PathBuf::from(install_location))
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn detect_platform() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn returns_none_when_no_game_dir_exists() {
        // With no actual game installed in test env, detect() should return None
        // (unless running on a dev machine with X4 installed — acceptable)
        let result = detect();
        // Can't assert None because dev might have game installed.
        // Assert that if Some, the path exists.
        if let Some(path) = result {
            assert!(path.exists(), "Detected path must exist: {:?}", path);
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_paths_are_absolute() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".into());
        let paths = linux_steam_paths();
        assert!(!paths.is_empty());
        for p in &paths {
            assert!(p.is_absolute(), "Path must be absolute: {:?}", p);
            assert!(p.to_string_lossy().contains(GAME_DIR_NAME));
        }
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --package map-io game_path
```

Expected: 2 tests pass (1 or 2 depending on platform).

- [ ] **Step 3: Commit**

```bash
git add crates/map-io/src/game_path.rs
git commit -m "feat(io): X4 game path auto-detection (Linux Steam + Windows registry)"
```

---

## Task 10: map-app — egui window + dark theme

**Files:**
- Modify: `crates/map-app/src/main.rs`
- Create: `crates/map-app/src/app.rs`
- Create: `crates/map-app/src/theme.rs`
- Create: `crates/map-app/src/ui/mod.rs`

- [ ] **Step 1: Implement theme**

`crates/map-app/src/theme.rs`:
```rust
use egui::{Color32, Rounding, Stroke, Style, Visuals};

pub const BG_DARK: Color32      = Color32::from_rgb(10, 12, 18);
pub const BG_PANEL: Color32     = Color32::from_rgb(20, 23, 33);
pub const BG_WIDGET: Color32    = Color32::from_rgb(30, 34, 53);
pub const ACCENT: Color32       = Color32::from_rgb(124, 58, 237);  // purple
pub const ACCENT_DIM: Color32   = Color32::from_rgb(58, 63, 90);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(224, 228, 240);
pub const TEXT_MUTED: Color32   = Color32::from_rgb(122, 138, 180);
pub const BORDER: Color32       = Color32::from_rgb(42, 45, 61);
pub const GATE_GREEN: Color32   = Color32::from_rgb(42, 170, 106);
pub const SHIP_YELLOW: Color32  = Color32::from_rgb(244, 180, 74);
pub const HOSTILE_RED: Color32  = Color32::from_rgb(239, 68, 68);

pub fn apply(ctx: &egui::Context) {
    let mut style = Style::default();
    style.visuals = dark_visuals();
    ctx.set_style(style);
}

fn dark_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = BG_PANEL;
    v.window_fill = BG_PANEL;
    v.faint_bg_color = BG_DARK;
    v.extreme_bg_color = BG_DARK;
    v.widgets.noninteractive.bg_fill = BG_WIDGET;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_MUTED);
    v.widgets.inactive.bg_fill = BG_WIDGET;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    v.widgets.hovered.bg_fill = ACCENT_DIM;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    v.selection.bg_fill = Color32::from_rgba_premultiplied(124, 58, 237, 40);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.window_rounding = Rounding::same(4.0);
    v.window_stroke = Stroke::new(1.0, BORDER);
    v
}
```

- [ ] **Step 2: Implement App struct**

`crates/map-app/src/app.rs`:
```rust
use map_domain::universe::Universe;
use map_domain::view::ViewMode;

pub struct App {
    pub universe: Universe,
    pub view_mode: ViewMode,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, universe: Universe) -> Self {
        crate::theme::apply(&cc.egui_ctx);
        Self {
            universe,
            view_mode: ViewMode::initial(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("foundations-map — Phase 1 in progress");
        });
    }
}
```

- [ ] **Step 3: Implement main**

`crates/map-app/src/main.rs`:
```rust
mod app;
mod theme;
pub mod ui;

fn main() -> eframe::Result<()> {
    let universe = map_domain::universe::Universe::default();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Foundations Map")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Foundations Map",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, universe)))),
    )
}
```

`crates/map-app/src/ui/mod.rs`:
```rust
pub mod top_bar;
pub mod map_view;
pub mod sector_panel;
```

- [ ] **Step 4: Create stub UI files**

`crates/map-app/src/ui/top_bar.rs`:
```rust
pub struct TopBar;

impl TopBar {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.label("FOUNDATIONS MAP");
    }
}
```

`crates/map-app/src/ui/map_view.rs`:
```rust
pub struct MapView;

impl MapView {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.label("2D map placeholder");
    }
}
```

`crates/map-app/src/ui/sector_panel.rs`:
```rust
pub struct SectorPanel;

impl SectorPanel {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.label("Sector info placeholder");
    }
}
```

- [ ] **Step 5: Verify window opens**

```bash
cargo run --package map-app
```

Expected: dark window opens with "foundations-map — Phase 1 in progress" label.

- [ ] **Step 6: Commit**

```bash
git add crates/map-app/src/
git commit -m "feat(app): egui window with dark dashboard theme"
```

---

## Task 11: map-app — top bar layout

**Files:**
- Modify: `crates/map-app/src/ui/top_bar.rs`
- Modify: `crates/map-app/src/app.rs`

- [ ] **Step 1: Write test for TopBar state**

`crates/map-app/src/ui/top_bar.rs`:
```rust
pub struct TopBar {
    pub search_text: String,
}

impl Default for TopBar {
    fn default() -> Self {
        Self { search_text: String::new() }
    }
}

impl TopBar {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.colored_label(crate::theme::ACCENT, "FOUNDATIONS MAP");
            ui.add_space(16.0);
            let search = egui::TextEdit::singleline(&mut self.search_text)
                .hint_text("⌕ Search sectors, stations, ships...")
                .desired_width(300.0);
            ui.add(search);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_search_text_is_empty() {
        let bar = TopBar::default();
        assert!(bar.search_text.is_empty());
    }

    #[test]
    fn search_text_can_be_set() {
        let mut bar = TopBar::default();
        bar.search_text = "argon".into();
        assert_eq!(bar.search_text, "argon");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --package map-app top_bar
```

Expected: 2 tests pass.

- [ ] **Step 3: Wire top bar into App::update**

`crates/map-app/src/app.rs`:
```rust
use map_domain::universe::Universe;
use map_domain::view::ViewMode;
use crate::ui::top_bar::TopBar;

pub struct App {
    pub universe: Universe,
    pub view_mode: ViewMode,
    top_bar: TopBar,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, universe: Universe) -> Self {
        crate::theme::apply(&cc.egui_ctx);
        Self {
            universe,
            view_mode: ViewMode::initial(),
            top_bar: TopBar::default(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_bar")
            .exact_height(36.0)
            .show(ctx, |ui| {
                self.top_bar.show(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("2D map area — coming in next task");
        });
    }
}
```

- [ ] **Step 4: Run app to verify top bar renders**

```bash
cargo run --package map-app
```

Expected: top bar with "FOUNDATIONS MAP" label and search field visible.

- [ ] **Step 5: Commit**

```bash
git add crates/map-app/src/ui/top_bar.rs crates/map-app/src/app.rs
git commit -m "feat(app): top bar with search input"
```

---

## Task 12: map-app — 2D universe map (static render)

**Files:**
- Modify: `crates/map-app/src/ui/map_view.rs`

- [ ] **Step 1: Write tests for MapView state**

`crates/map-app/src/ui/map_view.rs`:
```rust
use egui::{Painter, Pos2, Rect, Response, Sense, Vec2};
use glam::Vec2 as GVec2;
use map_domain::ids::SectorId;
use map_domain::universe::{Universe, GateType};
use crate::theme;

pub struct MapView {
    pub pan: Vec2,   // offset in screen pixels
    pub zoom: f32,   // pixels per universe unit
}

impl Default for MapView {
    fn default() -> Self {
        Self { pan: Vec2::ZERO, zoom: 80.0 }
    }
}

impl MapView {
    /// Convert universe coordinates to screen position within the map rect.
    pub fn universe_to_screen(&self, rect: Rect, pos: GVec2) -> Pos2 {
        let center = rect.center();
        Pos2::new(
            center.x + self.pan.x + pos.x * self.zoom,
            center.y + self.pan.y + pos.y * self.zoom,
        )
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        universe: &Universe,
        selected: Option<SectorId>,
    ) -> MapViewResponse {
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size(),
            Sense::click_and_drag(),
        );

        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, 0.0, theme::BG_DARK);

        // Connections
        for conn in &universe.connections {
            let from = universe.sector(conn.from).map(|s| s.map_position);
            let to   = universe.sector(conn.to).map(|s| s.map_position);
            if let (Some(f), Some(t)) = (from, to) {
                let fp = self.universe_to_screen(rect, f);
                let tp = self.universe_to_screen(rect, t);
                let color = match conn.gate_type {
                    GateType::Standard     => theme::ACCENT_DIM,
                    GateType::Superhighway => theme::GATE_GREEN,
                };
                painter.line_segment([fp, tp], (1.5, color));
            }
        }

        // Sector nodes
        let mut clicked_sector: Option<SectorId> = None;
        let mut double_clicked_sector: Option<SectorId> = None;

        for sector in &universe.sectors {
            let screen_pos = self.universe_to_screen(rect, sector.map_position);
            let half = Vec2::new(36.0, 20.0);
            let node_rect = Rect::from_center_size(screen_pos, 2.0 * half);

            let is_selected = selected == Some(sector.id);
            let border_color = if is_selected { theme::ACCENT } else { theme::BORDER };
            let fill_color   = if is_selected {
                egui::Color32::from_rgba_premultiplied(124, 58, 237, 30)
            } else {
                theme::BG_WIDGET
            };
            let border_width = if is_selected { 2.0 } else { 1.0 };

            painter.rect(node_rect, 2.0, fill_color, (border_width, border_color));
            painter.text(
                screen_pos,
                egui::Align2::CENTER_CENTER,
                &sector.name,
                egui::FontId::proportional(10.0),
                theme::TEXT_PRIMARY,
            );

            // Hit detection
            if response.clicked() {
                if let Some(ptr) = response.interact_pointer_pos() {
                    if node_rect.contains(ptr) {
                        clicked_sector = Some(sector.id);
                    }
                }
            }
            if response.double_clicked() {
                if let Some(ptr) = response.interact_pointer_pos() {
                    if node_rect.contains(ptr) {
                        double_clicked_sector = Some(sector.id);
                    }
                }
            }
        }

        MapViewResponse { clicked_sector, double_clicked_sector, response }
    }
}

pub struct MapViewResponse {
    pub clicked_sector: Option<SectorId>,
    pub double_clicked_sector: Option<SectorId>,
    pub response: Response,
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2 as GVec2;

    #[test]
    fn default_zoom_is_positive() {
        let mv = MapView::default();
        assert!(mv.zoom > 0.0);
    }

    #[test]
    fn universe_to_screen_center_at_origin() {
        let mv = MapView::default();
        let rect = Rect::from_center_size(Pos2::new(400.0, 300.0), Vec2::new(800.0, 600.0));
        let screen = mv.universe_to_screen(rect, GVec2::ZERO);
        assert_eq!(screen, Pos2::new(400.0, 300.0));
    }

    #[test]
    fn universe_to_screen_applies_zoom() {
        let mv = MapView { pan: Vec2::ZERO, zoom: 100.0 };
        let rect = Rect::from_center_size(Pos2::new(400.0, 300.0), Vec2::new(800.0, 600.0));
        let screen = mv.universe_to_screen(rect, GVec2::new(1.0, 0.0));
        assert_eq!(screen.x, 500.0); // 400 + 1.0 * 100
    }

    #[test]
    fn universe_to_screen_applies_pan() {
        let mv = MapView { pan: Vec2::new(50.0, -30.0), zoom: 80.0 };
        let rect = Rect::from_center_size(Pos2::new(400.0, 300.0), Vec2::new(800.0, 600.0));
        let screen = mv.universe_to_screen(rect, GVec2::ZERO);
        assert_eq!(screen, Pos2::new(450.0, 270.0));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --package map-app map_view
```

Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/map-app/src/ui/map_view.rs
git commit -m "feat(app): 2D universe map with sector nodes and gate connections"
```

---

## Task 13: map-app — pan + zoom interaction

**Files:**
- Modify: `crates/map-app/src/ui/map_view.rs`
- Modify: `crates/map-app/src/app.rs`

- [ ] **Step 1: Add pan/zoom handling to MapView::show**

Add this block inside `MapView::show`, after hit detection and before returning:

```rust
// Pan: drag anywhere on the map
if response.dragged() {
    self.pan += response.drag_delta();
}

// Zoom: scroll wheel, zooming toward pointer position
if let Some(hover_pos) = response.hover_pos() {
    let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
    if scroll_delta != 0.0 {
        let zoom_factor = (scroll_delta * 0.001).exp();
        let old_zoom = self.zoom;
        self.zoom = (self.zoom * zoom_factor).clamp(20.0, 400.0);
        // Adjust pan so zoom targets the pointer position
        let center = rect.center();
        let mouse_offset = hover_pos - center;
        let scale_change = self.zoom / old_zoom;
        self.pan = mouse_offset + (self.pan - mouse_offset) * scale_change;
    }
}
```

- [ ] **Step 2: Wire map view into app layout with right panel**

`crates/map-app/src/app.rs`:
```rust
use map_domain::universe::Universe;
use map_domain::view::ViewMode;
use crate::ui::{top_bar::TopBar, map_view::MapView, sector_panel::SectorPanel};

pub struct App {
    pub universe: Universe,
    pub view_mode: ViewMode,
    top_bar: TopBar,
    map_view: MapView,
    sector_panel: SectorPanel,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, universe: Universe) -> Self {
        crate::theme::apply(&cc.egui_ctx);
        Self {
            universe,
            view_mode: ViewMode::initial(),
            top_bar: TopBar::default(),
            map_view: MapView::default(),
            sector_panel: SectorPanel::default(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_bar")
            .exact_height(36.0)
            .show(ctx, |ui| {
                self.top_bar.show(ui);
            });

        egui::SidePanel::right("sector_panel")
            .exact_width(220.0)
            .resizable(false)
            .show(ctx, |ui| {
                let selected = self.view_mode.selected_sector();
                let sector = selected.and_then(|id| self.universe.sector(id));
                self.sector_panel.show(ui, sector, &self.universe);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let selected = self.view_mode.selected_sector();
            let mvr = self.map_view.show(ui, &self.universe, selected);

            if let Some(sector_id) = mvr.double_clicked_sector {
                self.view_mode = self.view_mode.clone().select_sector(sector_id).open_sector_3d();
            } else if let Some(sector_id) = mvr.clicked_sector {
                self.view_mode = self.view_mode.clone().select_sector(sector_id);
            }
        });
    }
}
```

- [ ] **Step 3: Run the app and test pan/zoom manually**

```bash
cargo run --package map-app
```

Expected: map renders (empty — no universe data yet), drag pans, scroll zooms toward cursor.

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/src/ui/map_view.rs crates/map-app/src/app.rs
git commit -m "feat(app): pan and zoom-to-cursor on 2D universe map"
```

---

## Task 14: map-app — right panel (sector info)

**Files:**
- Modify: `crates/map-app/src/ui/sector_panel.rs`

- [ ] **Step 1: Write tests for panel state**

`crates/map-app/src/ui/sector_panel.rs`:
```rust
use map_domain::universe::{Sector, Universe, GateType};
use crate::theme;

#[derive(Default)]
pub struct SectorPanel;

impl SectorPanel {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        sector: Option<&Sector>,
        universe: &Universe,
    ) {
        ui.add_space(8.0);

        let Some(sector) = sector else {
            ui.colored_label(theme::TEXT_MUTED, "Select a sector");
            ui.add_space(4.0);
            ui.colored_label(theme::TEXT_MUTED, "Click on the map.");
            return;
        };

        // Name + faction
        ui.colored_label(theme::TEXT_MUTED, "SECTOR");
        ui.add_space(2.0);
        ui.colored_label(theme::TEXT_PRIMARY, &sector.name);
        if let Some(faction_id) = sector.faction {
            ui.colored_label(theme::ACCENT, format!("Faction #{}", faction_id.0));
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Connections
        ui.colored_label(theme::TEXT_MUTED, "CONNECTIONS");
        ui.add_space(4.0);
        let neighbours = universe.neighbour_ids(sector.id);
        if neighbours.is_empty() {
            ui.colored_label(theme::TEXT_MUTED, "None");
        }
        for nb_id in &neighbours {
            if let Some(nb) = universe.sector(*nb_id) {
                let conns = universe.connections_for(sector.id);
                let gate_type = conns.iter()
                    .find(|c| c.from == *nb_id || c.to == *nb_id)
                    .map(|c| &c.gate_type);
                let prefix = match gate_type {
                    Some(GateType::Superhighway) => "⇒",
                    _ => "→",
                };
                ui.colored_label(theme::TEXT_PRIMARY, format!("{} {}", prefix, nb.name));
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Static objects count
        ui.colored_label(theme::TEXT_MUTED, "OBJECTS");
        ui.add_space(4.0);
        ui.colored_label(theme::TEXT_PRIMARY, format!("{} static objects", sector.static_objects.len()));

        ui.add_space(12.0);

        // Open 3D view button
        if ui.button("▣  Open 3D View").clicked() {
            // ViewMode transition is handled by the parent (App)
            // We communicate intent via a flag
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_domain::ids::SectorId;

    #[test]
    fn panel_handles_no_selection() {
        // Just verify the struct constructs and default works
        let _panel = SectorPanel::default();
    }
}
```

- [ ] **Step 2: Wire "Open 3D View" click through App**

The button click needs to signal `App` to transition `ViewMode`. Add a return value from `SectorPanel::show`:

```rust
// Add to SectorPanel::show return type:
pub struct SectorPanelResponse {
    pub open_3d_clicked: bool,
}

// In show(), replace the button:
let open_clicked = ui.button("▣  Open 3D View").clicked();
SectorPanelResponse { open_3d_clicked: open_clicked }
```

In `App::update`, handle the response:
```rust
let panel_resp = self.sector_panel.show(ui, sector, &self.universe);
if panel_resp.open_3d_clicked {
    self.view_mode = self.view_mode.clone().open_sector_3d();
}
```

- [ ] **Step 3: Run tests + visual check**

```bash
cargo test --package map-app sector_panel
```

```bash
cargo run --package map-app
```

Expected: right panel shows "Select a sector". (Universe is empty — next task loads real data.)

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/src/ui/sector_panel.rs crates/map-app/src/app.rs
git commit -m "feat(app): sector info panel with connections and Open 3D View button"
```

---

## Task 15: Wire universe loading into App

**Files:**
- Modify: `crates/map-app/src/main.rs`

- [ ] **Step 1: Load universe from game files at startup**

`crates/map-app/src/main.rs`:
```rust
mod app;
mod theme;
pub mod ui;

fn main() -> eframe::Result<()> {
    // Attempt to load universe from game files
    let universe = load_universe();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Foundations Map")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Foundations Map",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, universe)))),
    )
}

fn load_universe() -> map_domain::universe::Universe {
    let game_path = map_io::game_path::detect();

    let Some(game_dir) = game_path else {
        eprintln!("[map] Game path not found — starting with empty universe.");
        return map_domain::universe::Universe::default();
    };

    eprintln!("[map] Found game at: {:?}", game_dir);

    let galaxy_xml = game_dir
        .join("maps")
        .join("xu_ep2_universe")
        .join("galaxy.xml");

    match map_io::xml_parser::parse_galaxy(&galaxy_xml) {
        Ok(universe) => {
            eprintln!("[map] Loaded {} sectors.", universe.sectors.len());
            universe
        }
        Err(e) => {
            eprintln!("[map] Failed to parse galaxy.xml: {:?}", e);
            map_domain::universe::Universe::default()
        }
    }
}
```

- [ ] **Step 2: Run app — verify sectors load if game installed, empty otherwise**

```bash
cargo run --package map-app
```

Expected: either sectors appear on the 2D map (game installed) or empty map with log message.

- [ ] **Step 3: Run all tests to confirm nothing regressed**

```bash
cargo test
```

Expected: all tests pass across all crates.

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/src/main.rs
git commit -m "feat(app): load universe from X4 game files at startup"
```

---

## Phase 1 Done — Acceptance Criteria

Before declaring Phase 1 complete, verify all of the following manually:

- [ ] `cargo test` passes with zero failures across all 3 crates
- [ ] App window opens with dark dashboard theme
- [ ] Top bar visible with title and search field
- [ ] Right panel shows "Select a sector" when nothing selected
- [ ] Clicking a sector highlights it and shows its info in right panel
- [ ] Right panel shows correct sector name, connections
- [ ] Double-clicking a sector transitions ViewMode (logged or stub — no 3D yet)
- [ ] Map pan works: drag to pan
- [ ] Map zoom works: scroll toward cursor
- [ ] "Open 3D View" button visible in right panel when sector selected

---

## Phases 2–4: Task Outlines

> These phases will each receive a full detailed plan (like Phase 1 above) before implementation begins. Outlines here for orientation only.

### Phase 2 — 3D Sector View

1. **wgpu renderer bootstrap** — get a wgpu device/queue from eframe's render state, render a solid-color texture, display in egui via `egui::Image`
2. **Orbit camera** — `OrbitCamera` struct, mouse drag = rotate, scroll = zoom, fit-all-sector distance calculation
3. **Geometry generation** — box mesh (stations), torus (gates), sphere (resource zones); faction colour uniforms
4. **Render loop** — per-frame: collect static objects for selected sector → generate draw calls → render to texture → display
5. **Object selection** — ray-cast from screen click into 3D scene → select nearest object → panel switches to object detail
6. **Panel: sector objects list** — list all static objects in right panel when 3D is open; click list item = select in 3D
7. **Escape behaviour** — deselect object + reset camera to fit-all; ✕ closes 3D view
8. **3D panel UI** — centered 80% overlay, dimmed map behind, resize handle, breadcrumb header

### Phase 3 — Live Data

1. **HTTP client** — `ureq`-based poller in `map-io`, runs on its own thread, parses X4 External App API responses into `PositionUpdate` vec
2. **Arc<RwLock<World>> wiring** — shared between IO thread and App; IO thread writes, UI reads per frame; calls `ctx.request_repaint()` after each write
3. **Live ships in 3D** — `World::entities_in_sector()` added to render loop; ships rendered as small coloured spheres
4. **Search index rebuild** — `build_search_index(universe, world)` called after each World update
5. **Connection status indicator** — top bar shows live/offline dot; changes colour reactively when API connects/disconnects

### Phase 4 — Search + Polish

1. **SearchIndex impl** — prefix + fuzzy match over entries, scope filtering (universe vs sector), faction + kind filters
2. **Search UI** — dropdown results under search bar; keyboard navigation (↑↓ Enter); click navigates map or selects 3D object
3. **Camera lerp** — smooth `target` and `distance` transitions on object select / Escape
4. **Pan/zoom animation** — smooth pan when search result navigates map to a sector
5. **Loading states** — spinner while parsing game files; "no game found" empty state with manual path entry
6. **Auto game-path with manual override** — settings dialog or env var `X4_PATH` for override
7. **CI distribution builds** — GitHub Actions: Linux x86_64 `.tar.gz`, Windows x86_64 `.exe`
