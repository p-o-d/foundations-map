# Phase 2 Retrospective — 3D Sector View + Static Data

**Date:** 2026-05-15
**Branch:** master
**Status:** Phase 2 complete + bonus scope

## What was built

| Layer | Output |
|---|---|
| `map-domain` | `StaticObject { kind, position, faction, name, rotation, details }`; `ViewMode` state machine; `Connection { from, to, gate_type }` with `Superhighway` variant |
| `map-io` | Multi-file XML scanner across main + DLC cats; parsers for `galaxy`, `clusters`, `sectors`, `zones`, `god`, `sechighways`; quaternion → euler conversion; DLC `<diff>/<add>` overlay support |
| `map-app/renderer` | wgpu `GpuScene` with dynamic uniform buffer (256-byte stride, 128 obj cap); orbit camera with `fit_all`; box/sphere/ring meshes |
| `map-app/ui` | `MapView` (2D hex universe, animated dashed one-way superhighways), `SectorView3D` (3D scene + 2D screen-space gates), `SectorPanel` (scrollable detail with kv property list), `TopBar`, `theme` |

## Data loaded (final tally)

```
[map] Universe XML files: 37 (galaxy/clusters/zones/sectors merged)
[map] Gate objects loaded: 448
[map] Non-gate objects loaded: 7
[map] God objects loaded: 67
[map] God stations loaded: 221
[map] Superhighway connections (sector pairs): 51
[map] Superhighway connections loaded: 103
[map] Loaded 144 sectors.
```

Tests: 47 passing across 6 suites.

## Key insights from X4 docs (this session)

- **Hierarchy:** Galaxy → Cluster → Sector → Zone. Zones are points + variable radius regions within sectors.
- **Travel hierarchy:** Inter-cluster = jumpgates / orbital accelerators; intra-cluster = superhighways (`sechighways`); intra-sector = zonehighways.
- **Units:** metres (÷1000 = km); coordinates X right, Y up, Z forward; rotation via quaternion.
- **DLC:** ships own `maps/xu_ep2_universe/dlc_*_*.xml` + uses `<diff><add>` patches over main `galaxy.xml`.
- **SHCon naming:** vanilla uses even = entrance, odd = exit (we treat both as superhighway endpoints; direction comes from clusters.xml `<entrypoint>`/`<exitpoint>` mapping).
- **mapdefaults.xml:** lookups for `(pageId, textId)` translation, all five DLCs ship their own.

These match what we discovered empirically while parsing.

## Architecture decisions worth remembering

- **2D screen-space gates** instead of GPU mesh: gates filtered out of `build_draw_calls`; drawn in `draw_gates_2d` as projected polylines + arrows at constant pixel size. Reason: ring meshes don't scale visually well; 2D gives constant size + correct orientation perception.
- **Connection model is undirected at storage but directed at render:** `Connection { from, to }` for one-way superhighways; `MapView` deduplicates bidirectional pairs by `(from.0 > to.0)`.
- **Stations have positions only when fixed:** god.xml `<station>` parser skips procedural ones (no `<position>` element).
- **Quaternion→euler at parse time:** lossy round-trip is acceptable for visualization; keeps domain model glam-Quat-free.
- **`details: Vec<(String, String)>`:** kv bag in `StaticObject` lets each object kind expose different metadata without domain churn. Trade-off: stringly-typed, no schema.

## Code-quality findings (from reviewer)

| Severity | Location | Issue | Status |
|---|---|---|---|
| 🟡 risk | sector_view.rs:283 | superhighway detection via `name.starts_with("superhighway")` | **fixed in this session** (new `Highway` kind) |
| 🟡 risk | xml_parser.rs | `parse_galaxy_from_game` orchestrates 10+ parsers, mutates many HashMaps | deferred |
| 🟡 risk | xml_parser.rs | ObjectId ranges 10k/20k/30k/40k/50k could collide if counts grow | acceptable now (max counter 221 ≪ 10k spacing) |
| 🟡 risk | xml_parser.rs | DLC case normalization duplicated at 3+ sites | deferred |
| 🔵 nit | sector_panel.rs | inconsistent key names across object types ("Type" vs "Macro") | acceptable, easy to fix later |
| 🔵 nit | xml_parser.rs:1128 | god stations parser has many mutable refs in handler | deferred |

## Notes for future phases

- **Phase 3 (live data):** needs X4 HTTP API mod or REST endpoint. `World` store already exists in `map-domain` but unused.
- **Phase 4 (polish):** search index across all 144 sectors + 743 static objects; camera lerp; CI builds.
- **Open data gaps:** dynamic stations from economy not loaded (538 god.xml entries, 130 had positions, 408 procedural). Resource asteroids spawned at runtime from `regionobjectgroups.xml` — not parseable as fixed.
