# Phase 3 Polish — Stations, Live Entities, Faction Data

**Date:** 2026-05-18
**Status:** spec (approved, pre-plan)
**Trigger:** User observed too few stations visible in 3D / side panel after Phase 3 shipped. Investigation revealed: side panel lists only god.xml static stations (~221); save's 1392 live stations rendered only as small 16-px screen-space squares and never enumerated. Plus nested entities (subordinate stations, docked ships) dropped by save parser's `pending.is_none()` gate.

## Goals

1. Live (save-derived) stations + ships are first-class throughout UI: enumerable in side panel, rendered as 3D meshes, hover-labelled.
2. Drop god.xml `<station>` parser. Save is the authority on station state.
3. Capture nested entities (docked ships, subordinate stations) in save parser, with parent-child relations.
4. Side panel: categorise objects (static, stations, capitals, medium, small), navigate into a parent's docked children, back-button to parent.
5. Replace hardcoded 8-colour faction palette with game's own faction colours from `libraries/colors.xml`.
6. Display human-readable faction names + entity codes everywhere a faction or entity is shown.
7. Hover label in 3D shows code + human name + faction for the object under cursor (single label at a time).

## Non-Goals

- Composing absolute positions for docked entities (kept as parent-local offsets in v1).
- Live ship `nameindex` → game-procedural-name resolution (requires per-faction name pools; deferred).
- Persistent search index over live entities (Phase 4).
- Camera follow / lerp on selection (Phase 4).

## Architecture Summary

```
        ┌───────────────────────────────────────────────────────────┐
        │ map-io (load)                                              │
        │                                                            │
        │  libraries/factions.xml ──┐                                │
        │  libraries/colors.xml ────┼──► faction_parser ──► FactionDB │
        │  libraries/mapdefaults.xml ┘ (+ translation)               │
        │                                                            │
        │  maps/xu_ep2_universe/* ──► xml_parser ──► Universe         │
        │  libraries/god.xml ─────► (objects only)                   │
        │                                                            │
        │  save.xml.gz ──► save_parser/* ──► World + overrides       │
        │       (now: nested stack, parent_id, code)                 │
        └───────────────────────────────────────────────────────────┘
                                 │
                                 ▼
        ┌───────────────────────────────────────────────────────────┐
        │ map-domain                                                 │
        │  Universe.faction_table: FactionId → FactionMeta {name, color}│
        │  Universe.faction_strings: String → FactionId              │
        │  World.parents / children / codes                          │
        │  ViewMode::SectorView.selected_entity                      │
        └───────────────────────────────────────────────────────────┘
                                 │
                                 ▼
        ┌───────────────────────────────────────────────────────────┐
        │ map-app/ui                                                 │
        │  sector_view: GPU box/sphere per live entity (top-level)   │
        │               hover label over hovered target              │
        │  sector_panel: STATIC / STATIONS / CAPITALS / MEDIUM /     │
        │                SMALL collapsing categories + DOCKED list   │
        │                + back-to-parent button                     │
        │  map_view + sector_view + sector_panel: faction colours +  │
        │                                          human names       │
        └───────────────────────────────────────────────────────────┘
```

## Section 1 — Faction metadata pipeline

### New types (`map-domain`)

```rust
#[derive(Debug, Clone)]
pub struct FactionMeta {
    pub display_name: String,   // resolved via translation table
    pub color: [u8;4],          // RGBA from libraries/colors.xml
}

// Added fields on Universe:
pub faction_table: HashMap<FactionId, FactionMeta>,
pub faction_strings: HashMap<String, FactionId>,
```

### New module (`map-io`)

`crates/map-io/src/faction_parser.rs`:

```rust
pub struct FactionDef {
    pub name_textref: (u32, u32),   // (page, id) for translation lookup
    pub color_mapping: String,      // e.g. "faction_argon"
}

pub fn parse_factions_xml(text: &str) -> HashMap<String, FactionDef>;
pub fn parse_colors_xml(text: &str) -> (
    HashMap<String, [u8;4]>,        // color id → RGBA
    HashMap<String, String>,        // mapping id → color id (refs)
);
```

### Pipeline at startup (in `xml_parser::parse_galaxy_from_game`)

1. `read_all_game_files("libraries/factions.xml")` → collect main + 5 DLC files.
2. `read_all_game_files("libraries/colors.xml")` likewise.
3. Build per-DLC `HashMap<String, FactionDef>`; merge (DLC last wins).
4. Parse colors: build (color-id → RGBA) + (mapping-id → color-id-ref); merge.
5. Resolve `faction_<id>` mapping → color-id → RGBA per faction string.
6. Resolve `name_textref` via existing translation table → display name.
7. Assign sequential `FactionId(1..N)` per faction string; populate `Universe.faction_table` + `faction_strings`.

### Save-parser integration

`save_parser::merge::merge` extended:

```rust
pub fn merge(
    batches: Vec<Vec<EntityRecord>>,
    sector_macros: Option<&HashMap<String, SectorId>>,
    faction_strings: &mut HashMap<String, FactionId>,     // NEW: shared, may grow
    next_faction_id: &mut u32,
) -> World;
```

Caller passes the already-populated `Universe.faction_strings`. Save factions not in the table (rare, e.g. mod faction) get newly minted IDs and a stub `FactionMeta { display_name: <raw_string>, color: grey }`.

`apply_faction_overrides` in `main.rs` reuses the same shared maps — no duplicate ID allocation between snapshot and static load.

### Tests

- `parse_factions_xml_extracts_id_name_color`: fixture with 2 faction entries.
- `parse_colors_xml_resolves_faction_mappings`: fixture with mapping → color chain.
- `faction_table_built_with_resolved_name_and_color`: integration on real files.

## Section 2 — Save parser nested capture + parent-child tracking

### Data-model changes

```rust
// crates/map-io/src/save_parser/types.rs
pub struct EntityRecord {
    pub id: u32,
    pub parent_id: Option<u32>,    // NEW
    pub macro_name: String,        // renamed from `name`
    pub code: Option<String>,      // NEW
    pub kind: LiveObjectKind,
    pub owner: Option<String>,
    pub position: Vec3,
    pub sector_macro: String,
}

// crates/map-domain/src/world.rs
pub struct World {
    // existing maps...
    pub parents:  HashMap<EntityId, EntityId>,
    pub children: HashMap<EntityId, Vec<EntityId>>,
    pub codes:    HashMap<EntityId, String>,
}
```

`World::insert_entity` extended: `parent: Option<EntityId>`, `code: Option<String>`.
New helpers: `World::parent_of(id) -> Option<EntityId>`, `World::children_of(id) -> &[EntityId]`.

### Parser change (`sector_chunk.rs`)

Replace `pending: Option<Pending>` with `stack: Vec<Pending>`.

State per pending:
```rust
struct Pending {
    open_depth: u32,
    id: u32,
    parent_id: Option<u32>,
    macro_name: String,
    code: Option<String>,
    kind: LiveObjectKind,
    owner: Option<String>,
    position: Option<Vec3>,
}
```

Algorithm:

- `<component class=...>` start:
  - `comp_depth += 1`
  - If class matches ship/station → push new `Pending { open_depth: comp_depth, parent_id: stack.last().map(|p| p.id), ... }`.
- `<offset>` start: track `offset_depth: Option<u32> = Some(comp_depth)`.
- `<offset>` end: `offset_depth = None`.
- `<position>` empty (in offset):
  - Fill `stack.last_mut().position` ONLY IF `stack.last().open_depth + 1 == offset_depth.unwrap()` AND `position.is_none()`.
  - (Prevents nested entity's offset from being misattributed to parent.)
- `</component>` end:
  - If `stack.last().open_depth == comp_depth` → pop, emit EntityRecord.
  - `comp_depth -= 1`.

### Tests

- `nested_ship_inside_station_emits_two_records_with_parent_link`
- `three_level_nesting_station_carrier_drone_yields_chain`
- `nested_position_does_not_overwrite_parent_position`

## Section 3 — Drop god.xml `<station>` parsing

### Code removals

- Call site `crates/map-io/src/xml_parser.rs:332-355` (god station load loop).
- Function `parse_god_stations_xml` (≈200 LOC, `xml_parser.rs:1239+`).
- Tests asserting god station counts.
- ObjectId range `50_000+` no longer used.

### Keep

- `parse_god_xml` (god `<object>`: wormholes, landmarks, debris, ObjectId 30k+).
- `StaticObjectKind::Station` enum variant (still used for the rare case god `<object>` emits a station; verify during impl — if not, also drop).

### Static object ranges after

| Range | Source | Count |
|---|---|---|
| 10k+ | gates (zones.xml) | 448 |
| 20k+ | non-gate zone objects | 7 |
| 30k+ | god objects | 67 |
| 40k+ | superhighway endpoints | 103 |

### CLAUDE.md updates

- Remove "538 station entries", "221 god.xml stations", "50k+ stations".
- Adjust final tally line.

## Section 4 — Side panel: categories + live entities + parent/child nav

### ViewMode extension

```rust
SectorView {
    sector: SectorId,
    selected_obj: Option<ObjectId>,
    selected_entity: Option<EntityId>,   // NEW; mutually exclusive with selected_obj
}
```

`view.rs` adds:
- `pub fn select_entity(self, EntityId) -> Self` (clears selected_obj).
- `pub fn select_object(self, ObjectId) -> Self` (clears selected_entity — extend existing).
- `pub fn deselect_entity(self) -> Self`.

### Panel layout (3D mode)

```
← Universe

SECTOR
<Name>
● Argon Federation         (faction color dot)

────────────────

▾ STATIC OBJECTS (12)
  ◯ Gate to Argon Prime
  ⇒ Highway → Hatikvah
  ✦ Anomaly XYZ
  ...

▾ STATIONS (47)
  ◼ YIB-942 — FRF Medical Supplies            ● Free Families
  ◼ MLJ-593 — FRF Medical Supplies            ● Free Families
  ...

▾ CAPITALS (8)     [L + XL]
  ▲ ARG-001 — Argon Heavy Destroyer           ● Argon Federation
  ...

▾ MEDIUM (23)
  ▲ ARG-042 — ARG Frigate                     ● Argon Federation
  ...

▾ SMALL (134)
  ▴ ARG-101 — Scout                           ● Argon Federation
  ...

──────────────── (when entity selected)

SELECTED
◼ YIB-942 — FRF Medical Supplies
Type: Station
Owner: Free Families       (faction-colored)
Position: x.x y.y z.z km
Sector: <Name>

▾ DOCKED (5)
  ▴ ship_xs_cargodrone_01                     ● Free Families
  ...

[ Open 3D View ]
```

When selected entity has a parent:

```
SELECTED
▴ ship_xs_cargodrone_01
[ ← Back to FRF Medical Supplies ]
Type: Small Ship (docked)
Owner: Free Families
Position: x.x y.y z.z km (offset from parent)
```

### Row format

`<icon> <code> — <human_name>     ● <faction_display>`

Fallbacks:
- code only: `<icon> <code>     ● <faction>`
- human_name only: `<icon> <human_name>     ● <faction>`
- neither: `<icon> <macro_stripped>     ● <faction>` (macro → lower → strip `_macro` → spaces → title-case)

Icons:
- Station: `◼`
- ShipExtraLarge / ShipLarge: `▲`
- ShipMedium / ShipSmall: `▴`

### Category filters

| Group | Source | Filter |
|---|---|---|
| STATIC OBJECTS | `sector.static_objects` | all kinds |
| STATIONS | World live in sector | `kind == Station && parent.is_none()` |
| CAPITALS | live | `(ShipXL \| ShipLarge) && parent.is_none()` |
| MEDIUM | live | `ShipMedium && parent.is_none()` |
| SMALL | live | `(ShipSmall \| ShipXS) && parent.is_none()` |

DOCKED list of a selected entity = `world.children_of(eid)` ordered by kind (Stations → Capitals → Medium → Small).

### SectorPanelResponse changes

```rust
pub struct SectorPanelResponse {
    pub open_3d_clicked: bool,
    pub back_to_map_clicked: bool,
    pub object_clicked: Option<ObjectId>,
    pub entity_clicked: Option<EntityId>,     // NEW
    pub back_to_parent_clicked: bool,         // NEW
}
```

### Implementation notes

- `SectorPanel::show` accepts `world: Option<&World>`.
- `egui::CollapsingHeader::new("STATIONS").default_open(true)` per category.
- Each row a clickable `Label`; click sets `entity_clicked`.

## Section 5 — 3D GPU rendering of live entities

### Mesh + scale per LiveObjectKind

| Kind | MeshKind | Scale (world units) |
|---|---|---|
| Station | Box | 4.0 |
| ShipExtraLarge | Box | 2.5 |
| ShipLarge | Box | 1.5 |
| ShipMedium | Sphere | 1.0 |
| ShipSmall | Sphere | 0.5 |

Faction colour from `Universe.faction_table`. Unowned = grey (`[128,128,128,255]`).

### Selection highlight

`selected_entity == Some(eid)` → tint `[1.0, 0.8, 0.1, 1.0]` (matches static yellow).

### Render filter

`build_draw_calls` iterates:
1. Static objects (gate / highway excluded — drawn 2D as today).
2. `world.entities_in_sector(sector.id)` filtered to `world.parents.get(&eid).is_none()` (top-level only — docked children invisible in scene).

### Picking

Extend pick into single function returning enum:

```rust
enum ClickedTarget { Static(ObjectId), Entity(EntityId) }
fn pick_target(ptr: Pos2, rect: Rect, camera: &OrbitCamera, sector: &Sector, world: Option<&World>) -> Option<ClickedTarget>;
```

20-px screen radius for both. Live entity wins on ties (drawn on top).

### GPU uniform buffer cap

- Current 128 slots insufficient (sectors with 500+ live entities).
- Raise to **2048** = 512 KB uniform buffer.
- Add cap-check; on overflow, draw the first 2048 and log a one-time warning `[render] sector X exceeded GPU draw cap (N of M shown)`.
- Verify wgpu device limit at startup; fall back to chunked passes if device caps below 512 KB.

### Drop 2D markers

- `draw_live_ships` 2D path removed entirely.

### Render order

Static → live entities → gates (2D) → hover label → axes. No depth attachment; painter's algorithm.

## Section 6 — Faction colours + human-readable names everywhere

### Replace local PALETTE definitions

- Delete `crates/map-app/src/ui/sector_view.rs:475 faction_color` + `PALETTE`.
- Delete `crates/map-app/src/ui/map_view.rs:28 PALETTE` + helper.

### Single helper (`map-app/src/colors.rs`, new)

```rust
pub fn faction_color(universe: &Universe, id: FactionId) -> egui::Color32 {
    universe.faction_table.get(&id)
        .map(|m| egui::Color32::from_rgba_unmultiplied(m.color[0], m.color[1], m.color[2], m.color[3]))
        .unwrap_or(theme::TEXT_MUTED)
}

pub fn faction_name<'a>(universe: &'a Universe, id: FactionId) -> &'a str {
    universe.faction_table.get(&id).map(|m| m.display_name.as_str()).unwrap_or("Unknown")
}
```

### Sites updated

| Site | Before | After |
|---|---|---|
| `map_view::sector_hex_color` | `PALETTE[fid % 8]` | `faction_color(universe, fid)` |
| `map_view` ship count badge | muted grey | per-sector dominant faction colour (mode of sector_idx kinds) |
| `sector_view::build_draw_calls` (live tint) | `PALETTE[fid % 8]` | `faction_color` |
| `sector_panel` faction line | `"Faction #N"` | `faction_name(...)` + colored dot |
| `sector_panel` live entity owner | not shown | shown w/ colored dot |
| `top_bar` player location label | raw `"{20004,4000011}"` | resolved sector name where mapdefaults pageid 20004 covers it |

### Human-readable entity names

Capture `code` attribute in save parser → `EntityRecord.code`, then `World.codes`.

Display priority (panel + hover label):
1. `<code>` if present.
2. Else macro → mapdefaults lookup → translation table (if entry exists).
3. Else macro stripped: lowercase → strip `_macro` → underscores to spaces → title-case.

`pub fn entity_display(universe: &Universe, world: &World, eid: EntityId) -> (Option<&str>, Option<String>)` — returns `(code, human_name)`.

## Section 7 — Hover label in 3D scene

### Goal

Single label appears next to object under cursor (any kind: static + live). Applies to whole 3D scene.

### Implementation

`SectorView3D::show`:

```rust
let hovered = canvas_resp.hover_pos()
    .and_then(|pos| pick_target(pos, view_rect, camera, sector, world));
```

After GPU pass + gates + axes:

```rust
if let Some(target) = hovered {
    draw_hover_label(painter, view_rect, camera, sector, world, universe, target);
}
```

### Label content

| Target | Lines |
|---|---|
| Static object | line 1: `<name>` (ACCENT); line 2: `Type: <kind>` (muted) |
| Live entity (no code, no name) | line 1: `<macro_stripped>` (ACCENT); line 2: `Type: <kind>` (muted); line 3: `<faction_display>` (faction color) |
| Live entity (with code) | line 1: `<code>` (ACCENT bold); line 2: `<human_name>` (TEXT_PRIMARY) — if available; line 3: `<faction_display>` (faction color) |

### Anchor / styling

- Anchor: 8 px right of, 4 px above object's screen point.
- Background: `Color32::from_rgba_unmultiplied(0, 0, 0, 200)` rounded rect (padding 4 px).
- Font: `FontId::proportional(11.0)`.

### Hover precedence

- If hovering an object: label = that one.
- If `hovered.is_none()`: no label.

Selection (click) is unchanged.

## Phase order

1. Faction parser + Universe.faction_table (Section 1) — unlocks all colour/name work.
2. Drop god stations (Section 3) — small, independent.
3. Save parser stack + parent + code (Section 2) — independent.
4. World extension (parents/children/codes) + 3D GPU render of live (Section 5) — depends on 1+2.
5. Sector panel categories + navigation (Section 4) — depends on 1+2+5.
6. Faction colours + names everywhere (Section 6) — depends on 1.
7. Hover label (Section 7) — depends on 4+5+6 only for full content; can ship in any order after sector_view restructure.

## Acceptance Criteria

- [ ] `parse_factions_xml` extracts 30+ factions across main+DLC; each has resolved display name + colour matching in-game palette.
- [ ] god.xml stations no longer appear in `Universe.sectors[*].static_objects`; tally line in startup log shows new ranges only.
- [ ] Save parser emits ≥ 12 600 entity records on user's quicksave (recovers ≥ 390 previously-dropped nested entities) — verify `[parse] entities=N` log.
- [ ] Every captured docked ship / subordinate station has a non-None `parent_id`; `World.children_of(parent)` returns them.
- [ ] Side panel in 3D mode shows STATIC / STATIONS / CAPITALS / MEDIUM / SMALL categories with counts that sum to the sector's entity total.
- [ ] Selecting a parent entity opens DOCKED list of children; selecting a child shows `← Back to <parent>` button that returns to parent.
- [ ] 3D scene renders every top-level live entity as a coloured GPU mesh (Box for station / Sphere for ship), tinted by faction colour from game files.
- [ ] Hovering any 3D object (static or live) shows a label with code + human name + faction; no labels when not hovering.
- [ ] No local PALETTE constants remain in `map-app/src/ui/`; all faction colour lookups route through `colors::faction_color`.
- [ ] All 53 existing tests still pass; new tests added per section pass.
- [ ] CLAUDE.md Data Loading section updated to reflect new counts.

## Open Risks

- **GPU cap of 2048:** some sectors may still exceed. Mitigation: log and truncate; fix in follow-up if user reports.
- **Faction overrides with new factions from save:** unknown factions get a stub `FactionMeta` (grey, raw string name). Acceptable — surfacing in side panel as raw string is better than dropping.
- **Mapdefaults entries for ship/station macros:** unknown coverage. If sparse, most entities show macro-stripped name. Acceptable v1.
- **Translation table page coverage:** faction name pages (20203 etc.) may not all be in `t/0001-l044.xml`. Logger warns once per missing page-id.
