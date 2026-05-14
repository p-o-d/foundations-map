# X4 Foundations Interactive Map — Design Spec

**Date:** 2026-05-14  
**Status:** Approved  
**Stack:** Rust, egui, wgpu, winit  
**Platforms:** Linux (Wayland + X11), Windows

---

## Overview

Interactive map application for the game X4 Foundations. Shows the full universe as a 2D sector graph. Any sector can be explored in a true 3D view with live object data overlaid from the running game. Reference aesthetic: X3 map by Scorp, modernised to a dark dashboard style.

---

## Data Sources

Two data sources, both handled by `map-io`, both optional independently:

| Source | What it provides | How accessed |
|---|---|---|
| X4 game XML files | Universe layout, sector contents, static objects | Parsed at startup from game install dir |
| X4 HTTP API (mod) | Live ship positions, states, factions | Polled periodically; optional |

App is fully usable without the live API — static map remains functional. Live data enriches the sector 3D view when available.

---

## Workspace Structure

Cargo workspace with three crates enforcing strict layer separation:

```
foundations-map/
├── Cargo.toml                    (workspace)
├── crates/
│   ├── map-domain/               # pure data model — no IO, no UI
│   ├── map-io/                   # XML parsing, game path detection, HTTP client
│   └── map-app/                  # egui + wgpu presentation
└── docs/
```

**Dependency graph:**

```
map-domain  ←──  map-io  ←──  map-app
                                 │
                           egui + wgpu + winit
```

`map-domain` has zero UI or IO dependencies. It can be compiled, tested, and reused as a headless library. `map-app` cannot exist without the domain but the domain can exist without `map-app`.

---

## Data Model (`map-domain`)

### Static Universe

```rust
struct Universe {
    sectors: Vec<Sector>,
    connections: Vec<Connection>,
}

struct Sector {
    id: SectorId,
    name: String,
    faction: Option<FactionId>,
    map_position: Vec2,           // projected from X4 galaxy 3D coords (y discarded, x/z → 2D)
    static_objects: Vec<StaticObject>,
}

struct Connection {
    from: SectorId,
    to: SectorId,
    gate_type: GateType,          // StandardGate | Superhighway
}

struct StaticObject {
    id: ObjectId,
    kind: StaticObjectKind,       // Station | Gate | ResourceZone | Anomaly
    position: Vec3,
    faction: Option<FactionId>,
    name: String,
}
```

### Live Entity Store (ECS-like)

Sparse component store for live game objects (ships, live station states). Plain `HashMap`-per-component — no ECS crate dependency.

```rust
type EntityId = u32;

struct World {
    names:       HashMap<EntityId, String>,
    positions:   HashMap<EntityId, Vec3>,
    velocities:  HashMap<EntityId, Vec3>,
    factions:    HashMap<EntityId, FactionId>,
    kinds:       HashMap<EntityId, LiveObjectKind>,
    sectors:     HashMap<EntityId, SectorId>,
    // denormalised for fast O(1) sector queries
    sector_idx:  HashMap<SectorId, Vec<EntityId>>,
}
```

Systems are plain functions — no trait magic:

```rust
fn update_positions(world: &mut World, updates: &[PositionUpdate]);
fn entities_in_sector(world: &World, sector: SectorId) -> &[EntityId];
fn build_search_index(universe: &Universe, world: &World) -> SearchIndex;
```

### View State Machine

```rust
enum ViewMode {
    UniverseMap { selected: Option<SectorId> },
    SectorView   { sector: SectorId, selected_obj: Option<ObjectId> },
}
```

---

## Search (`map-domain`)

In-memory index, no external search dependency.

```rust
struct SearchEntry {
    id: ObjectId,
    name: String,
    kind: EntryKind,              // Sector | Station | Gate | Ship | ResourceZone
    sector: SectorId,
    faction: Option<FactionId>,
}

struct SearchIndex {
    entries: Vec<SearchEntry>,
}
```

Index rebuilt from `Universe` + `World` on startup and after each live data update cycle.

**Scoping by view mode:**

| View | Scope | Result click action |
|---|---|---|
| `UniverseMap` | All sectors + all objects | Pan map to sector, select it |
| `SectorView3D` | Current sector only | Select object in 3D, orbit camera to it |

Filters (additive, applied after text match): object kind, faction.

---

## UI Layout (`map-app`)

### State 1 — Universe Map (default)

```
┌─────────────────────────────────────────┬──────────────┐
│  FOUNDATIONS MAP        ⌕ Search...     │              │
├─────────────────────────────────────────┤  Right panel │
│                                         │              │
│         2D Universe Map                 │  SECTOR      │
│         (pan + zoom)                    │  <name>      │
│                                         │  <faction>   │
│    [sector nodes + gate connections]    │              │
│                                         │  CONNECTIONS │
│                                         │  → ...       │
│                                         │              │
│                                         │  LIVE DATA   │
│                                         │  ships (when │
│                                         │  API active) │
│                                         │              │
│                                         │ [OPEN 3D]    │
└─────────────────────────────────────────┴──────────────┘
```

- Double-click sector → opens 3D view (primary trigger)
- "OPEN 3D VIEW" button in right panel → secondary trigger
- Right panel shows selected sector info: name, faction, connections, live ship count

### State 2 — 3D Sector View

```
┌──────────────────────────────────────────────────────────┐
│  FOUNDATIONS MAP              ⌕ Search (sector-scoped)   │
├──────────────────────────────────────────────────────────┤
│                 ╔══════════════════════╗   ┌───────────┐ │
│  [dimmed map]   ║  ← Universe          ║   │ SELECTED  │ │
│  [in bg]        ║  Argon Prime    ✕ ⤢ ║   │ <object>  │ │
│                 ║                      ║   │           │ │
│                 ║   3D sector view     ║   │ SECTOR    │ │
│                 ║   (80% excl. panel)  ║   │ (collapsed│ │
│                 ║                      ║   │  summary) │ │
│                 ║   rotate · zoom      ║   │           │ │
│                 ║                      ║   │ OBJECTS   │ │
│                 ╚══════════════════════╝   │ • Station │ │
│                                            │ • Gate    │ │
│                                            │ • Ships   │ │
└────────────────────────────────────────────┴───────────┘ │
```

- 3D panel: centered, 80% of window width excluding right panel
- Map visible but dimmed behind 3D panel; not interactive while 3D open
- Right panel switches: sector info → collapsed sector summary + sector objects list
- Clicking object in 3D list or 3D view: selects it, panel shows object detail, camera orbits to it
- `Escape`: deselect object + reset camera to fit-all-sector view
- `✕` or `← Universe`: close 3D panel, return to map view
- Resize handle on 3D panel edge (default 80%, user-adjustable)

---

## 3D Renderer (`map-app/renderer`)

**Backend:** wgpu (Vulkan on Linux, DX12 on Windows, auto-selected)

**Camera:** orbit-only (no pan)

```rust
struct OrbitCamera {
    target: Vec3,       // sector center by default; selected object position when object selected
    distance: f32,      // clamped: [min_fit_all, max_zoom_in]
    yaw: f32,
    pitch: f32,         // clamped to avoid gimbal flip
}
```

- Default: `target = sector_center`, distance = fits all sector contents in view
- On object select: `target` transitions to object position (smooth lerp)
- `Escape`: `target` resets to sector center, distance resets to fit-all
- Rendered to wgpu texture → displayed inside egui `Image` widget

**Object representation (Phase 1–2):**
- Stations: box mesh, faction colour tint, label billboard
- Gates: torus/ring mesh, green tint
- Ships: small sphere or simplified ship silhouette (live data only)
- Resource zones: transparent sphere, desaturated green

---

## Cross-Platform

| Concern | Linux | Windows |
|---|---|---|
| Window/input | winit (`wayland` + `wayland-dlopen` features) | winit default |
| GPU backend | Vulkan (wgpu auto-selects) | DX12 → Vulkan fallback |
| Game path | `~/.steam/steam/steamapps/common/X4 Foundations` | Registry `Steam App 392160` → fallback `Program Files` |
| Distribution | `.tar.gz` or AppImage | `.exe` (static) or NSIS installer |

Paths always handled via `std::path::PathBuf` — no hardcoded separators.

---

## Phased Implementation Plan

### Phase 1 — Data + 2D Universe Map
- Cargo workspace setup (3 crates)
- `map-domain`: Universe, Sector, Connection, StaticObject types + unit tests
- `map-io`: X4 XML parser with fixture-based integration tests; game path detection
- `map-app`: egui window, 2D map view (pan + zoom), sector selection, right panel (sector info)

### Phase 2 — 3D Sector View
- wgpu renderer integrated into egui via texture
- Orbit camera (rotate + zoom, fit-all default)
- Static objects rendered (stations, gates, resource zones)
- Object selection in 3D → right panel detail
- `Escape` / close panel behaviour

### Phase 3 — Live Data
- `map-io`: X4 HTTP API client, periodic polling, `World` updates
- Live ships appear in 3D sector view
- Search index rebuilt on live data update
- Connection status indicator (live / offline)

### Phase 4 — Search + Polish
- Full search implementation (universe-scoped + sector-scoped)
- UI polish: smooth pan/zoom, camera lerp, loading states
- Auto game-path detection with manual override fallback
- Distribution builds (Linux + Windows CI)

---

## TDD Rules

- `map-domain`: pure unit tests — all domain logic tested headlessly
- `map-io`: integration tests against XML fixtures in `crates/map-io/tests/fixtures/`; HTTP client tested with mock server (`mockito`)
- `map-app`: `AppState` + `ViewMode` state machine tested as pure functions; camera math unit tested; renderer not unit tested (verified visually per phase)
- Never mock `map-domain` types — always use real domain values in tests
- Phase does not advance until its tests pass
