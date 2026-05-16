# X4 Foundations Map — Design Spec

**Last revised:** 2026-05-15 (post Phase 2)

## Overview

A standalone Rust desktop app that visualizes X4 Foundations' universe and lets the player inspect sectors at any zoom level — from full galaxy to individual gates inside one sector. Reads game data from cat/dat archives at startup; in Phase 3 will read save-game snapshots for live(-ish) ship + station state.

## Data Sources

Two sources, loaded independently:

**Static (cat/dat archives)** — universe topology and fixed placements:
- `maps/xu_ep2_universe/galaxy.xml` — cluster positions
- `maps/xu_ep2_universe/clusters.xml` — sector→cluster + intra-cluster superhighway connections
- `maps/xu_ep2_universe/sectors.xml` — superhighway endpoint zones (SHCon)
- `maps/xu_ep2_universe/zones.xml` — gate positions/quaternions + ref="asteroids" objects
- `libraries/mapdefaults.xml` — macro → translation page+text id
- `libraries/god.xml` — fixed objects + station spawn rules
- `t/0001-l044.xml` — English translation table (pages 20003 cluster, 20004 sector)
- Every DLC ships its own copies under the same `maps/xu_ep2_universe/` directory (with `dlc_*` prefixes) and overlays `galaxy.xml` via XML `<diff>/<add>` patches; all merged at load.

**Dynamic (save files)** — Phase 3:
- `~/.config/EgoSoft/X4/<id>/save/{quicksave,save_NNN}.xml.gz`
- Gzipped XML, ~30 MB compressed / ~300 MB raw
- Full universe state: 1300+ stations, 10k+ ships, faction relations, player money/location, time

## Workspace Structure

```
crates/
  map-domain/   — Types + invariants. No I/O. No egui.
  map-io/       — cat/dat reader, XML parsers, game-path detect, (Phase 3) save reader
  map-app/      — egui app + wgpu renderer (binary: foundations-map)
```

## Data Model (`map-domain`)

```rust
// Hierarchy (loaded from static data)
pub struct Universe {
    pub sectors:     Vec<Sector>,
    pub clusters:    Vec<Cluster>,
    pub connections: Vec<Connection>,
}

pub struct Cluster { id: ClusterId, name, map_position: Vec2, radius: f32 }
pub struct Sector  {
    id: SectorId, name, faction: Option<FactionId>, map_position: Vec2,
    static_objects: Vec<StaticObject>,
    cluster_id: Option<ClusterId>, index_in_cluster: u32, cluster_sector_count: u32,
}
pub struct Connection { from: SectorId, to: SectorId, gate_type: GateType }
pub enum GateType { Standard, Superhighway }

pub struct StaticObject {
    id: ObjectId, kind: StaticObjectKind, position: Vec3, faction: Option<FactionId>,
    name: String, rotation: Option<(f32,f32,f32)>, details: Vec<(String,String)>,
}
pub enum StaticObjectKind { Station, Gate, ResourceZone, Anomaly, Highway }

// View state
pub enum ViewMode {
    UniverseMap { selected: Option<SectorId> },
    SectorView  { sector: SectorId, selected_obj: Option<ObjectId> },
}

// Live entity store — populated in Phase 3 from save files
pub struct World { /* names, positions, velocities, factions, kinds, sectors, sector_idx */ }
```

`details: Vec<(String,String)>` is a free-form kv bag so each object kind can expose
different metadata (Race/Owner/Type/Gamestart for stations, Direction/Destination for
highways, Macro for god objects) without changing the domain on every iteration.

## Sector Layout Trick

X4 sectors within a cluster can be 100+ map units apart in the in-game travel sense
(`clusters.xml` offsets). On the **map**, X4 ignores those and lays out sectors
hexagonally inside the parent cluster.

We do the same: every sector's `map_position` = cluster center. Per-sector layout
offset is computed at render time as a hex pattern (1/2/3 sector special cases,
circle fallback for 4+), scaled by `hex_r` so spacing stays correct at every zoom level.
`Sector.{cluster_id, index_in_cluster, cluster_sector_count}` carries the layout state.

## Static Object Loading

Numeric ranges keep object IDs from colliding across sources:

| Range | Source | Count today |
|---|---|---|
| 10k+ | gates (zones.xml) | 448 |
| 20k+ | non-gate zone objects (asteroids) | 7 |
| 30k+ | god.xml fixed objects (wormholes, landmarks, debris) | 67 |
| 40k+ | superhighway connection zones (sectors.xml SHCon) | 103 |
| 50k+ | god.xml stations with `<position>` | 221 |

Rotation in zones.xml is stored as quaternion (`qx qy qz qw`); converted to euler
(pitch, yaw, roll) at parse time using `glam::EulerRot::YXZ`.

## View State Machine (`map-domain`)

```
                 select_sector            open_sector_3d
UniverseMap { None } ───────→ UniverseMap { Some(s) } ───────→ SectorView { sector: s, selected_obj: None }
                                              ↑                              │
                                              │ close_sector_3d              │ select_object(o)
                                              └──────────────────────────────│
                                                                             ↓
                                                              SectorView { sector: s, selected_obj: Some(o) }
                                                                             │
                                                                             │ deselect_object (Escape)
                                                                             ↓
                                                              SectorView { sector: s, selected_obj: None }
```

State transitions are pure functions on `ViewMode`. All UI events feed through them.

## Search (Phase 4, deferred)

Universe-wide index over: sector name, cluster name, static object name, station owner,
gate destination. Suffix-array or simple fuzzy matcher. Result types differentiated by
icon. Sector-scoped search filters to selected sector only.

## UI Layout (`map-app`)

```
┌────────────────────────────────────────────────────┬─────────────┐
│  TopBar — 36 px (search, mode toggle, time clock)   │             │
├────────────────────────────────────────────────────┤  SectorPanel│
│                                                    │  220 px     │
│   CentralPanel:                                    │  scrollable │
│     UniverseMap   OR   SectorView3D                │             │
│                                                    │             │
└────────────────────────────────────────────────────┴─────────────┘
```

**UniverseMap:** hex-tiled 2D, pan/zoom, fit-on-resize, faction-colored sectors,
cluster background hexes, animated dashed one-way superhighways.

**SectorView3D:** wgpu scene inside a paint callback, orbit camera, gates rendered
as screen-space circles + arrows (constant pixel size), other objects as 3D meshes,
6 axis arrows from world origin, ✕ in header to close.

**SectorPanel:** scrollable. Universe view → connections list. Sector view → objects list + selected-object detail (name, type, position, faction, rotation, all `details` kv).

## 3D Renderer (`map-app/renderer`)

- wgpu via `egui_wgpu::CallbackTrait`
- One pipeline, one bind group, dynamic uniform buffer (256-byte stride × 128 slots)
- No depth attachment (egui paint callback constraint); painter's algorithm via draw order
- Meshes generated at startup: box, sphere, ring
- `OrbitCamera`: spherical coords, target locked to world origin, FOV 60°, near 0.1 km, far 2,000,000 km

## Cross-Platform

Linux (Wayland primary) + Windows. Steam library detection on both. eframe handles
wgpu surface init. Native dialog for manual game-path override (Phase 4).

## Phased Implementation

### Phase 1 — Data + 2D Universe Map ✅
Workspace skeleton, cat/dat reader, all four universe XML parsers, mapdefaults
+ translations, faction colors, sector panel, top bar, theme. Completed.

### Phase 2 — 3D Sector View ✅
wgpu pipeline, orbit camera, static objects, object selection, side-panel detail.
Completed plus bonus: stations from god.xml, superhighway endpoints, cluster hexes
(via synthesized hex layout), animated dashed one-way superhighways, full property
list in panel. See retrospective `2026-05-15-phase2-retro.md`.

### Phase 3 — Live Data (save-game snapshots)
**Pivot from original spec.** The HTTP API mod referenced earlier (Alia5's
`X4-rest-server`) has been inactive since April 2023 and likely doesn't work with
current game builds. Egosoft does not ship an official live API.

New approach: parse the save game XML.

- New module `map-io::save_parser` — streaming `quick_xml::Reader` over a `flate2`
  gzip decoder; walks `<savegame><universe><component class="galaxy">…` tree;
  populates `World` (ships, stations) and updates `Sector.faction` from the
  per-sector `owner` attribute
- File watcher (`notify` crate) on `~/.config/EgoSoft/X4/<id>/save/`; reparses on
  modify. Manual refresh button as fallback
- Top-bar indicator: "Snapshot: 2026-05-15 14:23  (3m ago)"
- Live ships render in 3D sector view; in 2D map a per-sector ship count badge
- Acceptance: parsing user's quicksave under 5 seconds on dev machine; UI stays
  responsive (parse on background thread, channel completed `World` back)

Spec: `docs/superpowers/specs/2026-05-15-phase3-livedata.md` (next).

### Phase 4 — Search + Polish
Universe + sector-scoped search, camera lerp on selection, smooth pan/zoom inertia,
manual game-path override dialog, CI distribution builds (Linux deb + Windows zip).

## TDD Rules

- `map-domain`: pure unit tests; all state transitions covered headlessly
- `map-io`: integration tests against XML fixtures in `crates/map-io/tests/fixtures/`;
  save parser will need a small synthetic save fixture or a heavily trimmed real one
- `map-app`: state-machine tests on `ViewMode`; camera math unit tests; renderer
  verified visually per phase
- Never mock `map-domain` types — use real values in tests
- Phase does not advance until its tests pass

## Open Questions

- Save-file size (~300 MB raw XML): worth pre-filtering during stream to skip
  obviously irrelevant subtrees (engine state, NPC dialogue, etc.)?
- Locale: hard-coded `l044` (English) for now. Detect from Steam VDF later.
- Faction colors: 8-color palette is reused mod 8 — fine while < 8 factions visible
  on screen; revisit if Phase 3 surfaces all 20+ factions on universe map.
