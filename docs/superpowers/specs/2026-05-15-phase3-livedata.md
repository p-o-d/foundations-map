# Phase 3 — Live Data from Save-Game Snapshots

**Date:** 2026-05-15
**Pivot from:** original Phase 3 spec which assumed Alia5's `X4-rest-server` (stale since 2023, no recent releases, likely broken vs current X4 build 580735). Egosoft does not ship an official HTTP/REST API.

## Goal

Display ship and station state from the player's most recent X4 save, refreshing
automatically when the player saves the game. Combined with the existing static
data load, the app shows a much richer picture: actual ship positions, station
ownership, faction relations, player money/location, in-game time.

## Why save-game

| Approach | Verdict |
|---|---|
| HTTP API mod (Alia5) | Abandoned, brittle DLL injection |
| Build own DLL/LD_PRELOAD | Linux/Windows divergence, AV flags, version-fragile |
| Custom in-game Lua/MD mod | Player install friction; future option |
| Tail X4 log file | Sparse, events-only, not state |
| **Save XML parse** | Official format, no integration, full state, survives patches |

## Save format

- Path: `~/.config/EgoSoft/X4/<steamID>/save/{quicksave,save_NNN}.xml.gz`
- Compressed gzip; 30 MB compressed → ~300 MB raw XML
- Hierarchy:
  ```
  <savegame>
    <info>                     — version, time, player money + location, DLC list
    <universe>
      <factions>               — full faction list + relation matrix
      <component class="galaxy">
        <component class="cluster" macro="cluster_43_macro">
          <component class="sector" macro="cluster_43_sector001_macro" owner="teladi">
            <component class="zone">
              <component class="station" macro="..." owner="freesplit">
                <offset><position x y z/><rotation yaw/></offset>
                <construction>… modules …</construction>
              <component class="ship_l|m|s|xs" macro="..." owner="...">
                <offset><position…/></offset>
                <orders>…</orders>
  ```
- Sample counts on a mid-game save: 1392 stations, 6591 small ships, 3386 medium,
  836 large, 2273 active zones, plus faction relations and player state.

## Scope (Phase 3)

In-scope:

1. **Save parser** that streams the XML, builds:
   - `World` (live entities: ships + station instances with position, owner, kind, sector)
   - Per-sector faction override (from sector `owner` attribute)
   - Player state: money, location, in-game time
2. **File watcher** on the save directory; reparse on modify
3. **UI surfacing**:
   - 3D sector view shows ships in-sector as colored dots/shapes
   - 2D universe map shows per-sector ship count (small badge)
   - Top bar shows snapshot age ("3m ago") and manual refresh button
4. **Background-thread parse** so UI stays responsive during the 1–5 sec parse

Out-of-scope:

- Per-tick deltas (Phase 4 if a live data source materializes)
- Writing back to the save
- Ship orders / trades / economy figures
- Save selection UI (use most recent by mtime)

## Data Model Changes (`map-domain`)

`World` already exists but is unused; bring it in:

```rust
pub struct World {
    pub names:      HashMap<EntityId, String>,
    pub positions:  HashMap<EntityId, Vec3>,
    pub factions:   HashMap<EntityId, FactionId>,
    pub kinds:      HashMap<EntityId, LiveObjectKind>,  // ShipSmall/Medium/Large/XL/Station
    pub sectors:    HashMap<EntityId, SectorId>,
    pub sector_idx: HashMap<SectorId, Vec<EntityId>>,
}
```

Add a `SnapshotMeta`:

```rust
pub struct SnapshotMeta {
    pub path: PathBuf,
    pub mtime: SystemTime,
    pub game_time_seconds: f32,
    pub player_money: u64,
    pub player_location_name: String,
}
```

Add `App` field: `snapshot: Option<(SnapshotMeta, World)>`.

## map-io Changes

New file `crates/map-io/src/save_parser.rs`:

```rust
pub fn parse_save(path: &Path) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError>;
```

Implementation:

- Open file, wrap in `flate2::read::GzDecoder`, then `BufReader`
- Feed to `quick_xml::Reader` (streaming, no DOM)
- Maintain a stack of `(class, macro_lookup_to_sector_id)` while walking nested
  `<component>` elements
- On `class="sector"` → push current `SectorId` looked up from `macro_to_id`
- On `class="station"` or `class="ship_*"` → record entity (id from `id="[0xNNN]"`),
  position from inner `<offset><position/>`, owner from `owner` attr, kind from class
- On `class="galaxy"` parent attrs → grab `time` if present
- Return `World` + faction overrides per sector

`FactionOverrides`: `HashMap<SectorId, FactionId>` — applied to existing `Universe`
after parse.

Dependencies added to `map-io/Cargo.toml`:

```toml
flate2 = "1"
notify = "8"
```

(`quick_xml` already present.)

## File Watcher

New module `crates/map-io/src/save_watcher.rs`:

- Spawn a `notify::recommended_watcher` on `~/.config/EgoSoft/X4/*/save/` (autodetect
  the steamID subdir)
- Debounce: collapse rapid modify events; trigger reparse 1 s after last event
- Send `WatcherEvent::NewSnapshot(PathBuf)` over a `crossbeam_channel::Sender`

## UI changes (`map-app`)

- `TopBar` gets a snapshot indicator + refresh button:
  ```
  ┌──────────────────────────────────────┐
  │ ⟳  Snapshot: 14:23  (3m ago) | …     │
  ```
- `MapView` overlays a small "N ships" badge on each sector hex (count from `World`)
- `SectorView3D` adds a "Live entities" toggle (default on); when on, draws each
  ship in `World::entities_in_sector(current_sector)` as a colored shape (color =
  faction palette)
- `App::ui` polls a channel from the watcher; replaces `snapshot` field on new data;
  triggers `ctx.request_repaint()`

Parse runs on `std::thread::spawn`; result delivered via channel. Loading toast in
top-bar while parsing.

## Failure modes

| Failure | Behavior |
|---|---|
| No save dir exists yet | Snapshot disabled; warning toast on startup |
| Save parse fails mid-stream | Keep previous snapshot; log error in top-bar |
| Save references a sector macro we didn't load | Entity dropped, count incremented in error stat |
| Save bigger than expected (>500 MB) | Hard cap parse time at 30 s; abort, log |

## Acceptance Criteria

- [ ] `cargo test` passes (including a synthetic save fixture test)
- [ ] App starts with existing static data even if no save exists
- [ ] When a save is present, snapshot loads automatically within 5 s
- [ ] Snapshot age and player money visible in top bar
- [ ] Per-sector ship counts visible as badges on 2D map (hover tooltip optional)
- [ ] Opening 3D sector view shows ships as colored dots
- [ ] Saving the game in X4 triggers a reparse within 2 s of file modify
- [ ] Manual refresh button forces immediate reparse
- [ ] UI does not freeze during parse (background thread + channel)
- [ ] Memory usage stays under 1 GB after parse (full universe World is small;
      transient XML is discarded after parse)

## Open design questions

1. **EntityId scheme** — save uses hex pointers (`[0x2174add]`). Hash to u32?
   Use truncated value? Risk of collisions across save loads.
2. **Sector faction conflict** — save's `owner="teladi"` should override the
   static (currently always None) faction. Confirm string→FactionId mapping;
   probably build a `faction_name_to_id: HashMap<String, FactionId>` at first
   sight and stabilize across snapshots.
3. **DLC entity macros** — ships/stations spawned by DLC content. Macro lookup
   for naming may need translation table extension beyond what we already load.
4. **Save format versioning** — current build 580735. If a future build changes
   schema, parser may break silently. Detect via `<game version="800" build="…"/>`
   in `<info>`; warn if differs widely.
5. **Live ship orientation** — store the rotation (yaw only, in most saves) so 3D
   renders can show heading. Y-up convention same as static.

## Out-of-scope for this phase, deferred to Phase 4+

- Search index updates from World
- Camera follow / lerp toward ship of interest
- Heatmap by ship density per sector
- Trade / economy / production overlays
- Mod-supplied data (any X4 mod that writes extra files)
