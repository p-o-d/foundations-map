# CLAUDE.md

Guidance for Claude Code (claude.ai/code) working in this repo.

## Project

`foundations-map` — Interactive 2D universe map + 3D sector view for X4 Foundations (space sim). Rust workspace, edition 2024, cross-platform (Linux Wayland + Windows). 47 tests across 6 suites.

## Commands

```bash
cargo build                    # compile all crates
cargo run                      # build + run the app
cargo test                     # run all tests
cargo test <name>              # single test by name substring
cargo clippy                   # lint
cargo fmt                      # format
```

## Workspace

```
crates/
  map-domain/   # Pure data model: Sector, Cluster, Universe, Connection,
                # StaticObject, ViewMode, ids (SectorId/ClusterId/ObjectId/FactionId)
  map-io/       # X4 file parsing: cat/dat archives, XML, game-path detection
  map-app/      # egui UI: 2D map, 3D sector view (wgpu), side panel, theme
```

**map-domain** — no I/O, no egui. Pure types.
**map-io** — reads cat/dat archives + multiple XML formats to build Universe + StaticObjects.
**map-app** — binary `foundations-map`, wgpu backend via eframe 0.34.2.

## Data Loading (`map-io`)

`parse_galaxy_from_game(game_dir)` enumerates every `maps/xu_ep2_universe/*.xml` across the main game and all five DLC extensions, then classifies by filename suffix:

| Suffix | Parser fn | Produces |
|---|---|---|
| `galaxy.xml` | `parse_cluster_positions_xml` | cluster center positions (incl. DLC `<diff>/<add>` overlays) |
| `clusters.xml` | `parse_sector_placements_xml` + `parse_sechighway_connections_xml` | sector→cluster map; entry/exit zone pairs for superhighways |
| `zones.xml` | `parse_gate_connections_xml` + `parse_gate_positions_xml` + `parse_non_gate_objects_xml` | gate sector pairs; per-gate position + quaternion → euler; asteroids/anomalies in zones |
| `sectors.xml` | `parse_superhighway_zones_xml` | SHCon zone positions (one per superhighway endpoint) |

Plus single-path reads (via `read_all_game_files` for DLC merge):
- `libraries/mapdefaults.xml` — `(macro → (pageId, textId))` for sector + cluster names
- `t/0001-l044.xml` — translation table; pages **20003** (cluster names) and **20004** (sector names)
- `libraries/god.xml` — fixed `<object>` placements (wormholes, landmarks, debris) + 538 `<station>` entries (130 have fixed `<position>`; rest are procedural)

**Final loaded:** 144 sectors, 119 clusters, ~448 gates, ~221 stations, ~67 god objects, ~103 superhighway endpoints, ~28 inter-sector superhighway pairs. Single-sector clusters still listed in `Universe.clusters` for sector-membership lookup; rendered cluster hex skipped.

### cat/dat archive format

- `XX.cat` = ASCII index, one line per file: `internal/path size unix_ts md5hash`
- `XX.dat` = raw concatenated bytes; offset = cumulative sum of preceding sizes
- Main: `01.cat`..`08.cat`. DLC: `extensions/ego_dlc_*/ext_NN.cat`
- `cat_reader::list_files_matching(prefix, suffix)` enumerates all archives; used for universe XML scan

### DLC overlay quirks

- DLC `galaxy.xml` patches main via `<diff><add sel="..."><connection.../></add></diff>` — parser flips `in_galaxy = true` on `<diff>` or `<add>`
- DLC mapdefaults / zones files use distinct paths (`dlc_terran_*.xml`); all merged via `list_files_matching`
- DLC macro names sometimes lowercase (`cluster_709_sector001_macro`) vs main mixed case; all lookup keys `.to_lowercase()`
- Cluster names live on translation page **20003**, not 20004

## Sector Layout — the conceptual trick

X4's `clusters.xml` stores sector offsets in metres for in-game travel (often 100+ megametres apart). These are **not** map positions. The galaxy map renders sectors hexagonally inside their parent cluster.

We do the same:
- `Sector.map_position` = cluster center (galaxy.xml position ÷ 1,000,000) — all sectors in a cluster share this
- `Sector.cluster_id` + `index_in_cluster` + `cluster_sector_count` carry layout info
- `MapView` computes per-sector screen offset at render time using `hex_offset_pixels(idx, total, hex_r * 2)` — scales with hex_r so spacing stays right at any zoom

`Cluster.radius` field is unused now; cluster hex radius derived from `(sector_layout_r + hex_r) / 0.866 + 4` with 0.85× shrink for 2-sector clusters and 0.8× overall.

## Static Objects (`StaticObject`)

```rust
pub struct StaticObject {
    pub id: ObjectId,             // unique; ranges 10k=gates, 20k=non-gate zone objs,
                                  // 30k=god objects, 40k=superhighway endpoints, 50k=stations
    pub kind: StaticObjectKind,   // Station | Gate | ResourceZone | Anomaly | Highway
    pub position: Vec3,           // km (metres ÷ 1000)
    pub faction: Option<FactionId>,
    pub name: String,
    pub rotation: Option<(f32,f32,f32)>,  // pitch/yaw/roll degrees (from quaternion)
    pub details: Vec<(String,String)>,    // free-form kv for side-panel display
}
```

`Highway` kind = superhighway entry/exit point. Determined at parse time via clusters.xml `<entrypoint>`/`<exitpoint>` mapping; stored as `details["Direction"]` = "Outbound" or "Inbound". Rendered single-arrow (outbound or inbound) in 3D; bidirectional standard gates get two arrows.

## 3D Sector View (`map-app::ui::sector_view`)

- wgpu pipeline (`renderer::gpu::GpuScene`): one bind group, dynamic uniform buffer (256-byte stride, 128 obj cap), no depth attachment
- Meshes: box, sphere, ring (`renderer::mesh`)
- Orbit camera (`renderer::camera::OrbitCamera`): spherical coords, `fit_all` always centers `target = Vec3::ZERO`, computes distance from max radius
- **Gates + highways are NOT drawn via GPU**; filter excludes them. They render as 2D screen-space overlay in `draw_gates_2d`: 3D ring sampled as 32-segment polyline projected to screen, plus arrow line(s) on Y=0 horizontal plane pointing toward origin
- World-per-pixel scaling: `dist * fov_factor / view_height` so circle + arrow stay at constant pixel size on zoom
- Axis arrows (E/W/Up/Dn/N/S) drawn from world origin, length = `camera.distance * 0.15`

## 2D Map (`map-app::ui::map_view`)

- Hexes scale with zoom: `hex_r = (zoom * 3.0).clamp(12.0, 80.0)`
- Pan/zoom; fit-to-view on first frame and window growth
- Connections drawn in universe space; obstacle-avoiding bezier control point (`route_ctrl_point`) with deterministic tiebreak
- **Bidirectional superhighways** drawn once (skipped when `from.0 > to.0`) as solid green line
- **One-way superhighways** drawn as animated dashed line (`draw_dotted_flow`); 18 dashes, phase advances at 0.05 cycles/sec; uses `ui.input(|i| i.time)` + `request_repaint`
- Cluster hexes drawn behind sector hexes; faint fill `RGBA(60,70,110,25)`, name label above

## Side Panel (`map-app::ui::sector_panel`)

- 220 px right panel; `ScrollArea::vertical()` between fixed header + bottom button
- UniverseMap view → CONNECTIONS list (neighbours with `→` or `⇒` prefix)
- SectorView view → OBJECTS list + SELECTED detail (name, type label, position, faction, rotation, all `obj.details` kv lines)
- Selected object highlighted in `theme::ACCENT`

## egui 0.34.2 quirks

- `eframe::App` uses `fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame)` — **not** `update`
- Panels: `egui::Panel::top/right`, call `.show_inside(ui, ...)`
- `CornerRadius::same(n)` takes `u8`
- `ctx.set_global_style(style)` — not `set_style`
- wgpu paint callback: `eframe::egui_wgpu::Callback::new_paint_callback(rect, MyCallback)`; resources stored via `rs.renderer.write().callback_resources.insert(scene)`

## Game Path Detection (`game_path`)

- `detect()`: standard Steam paths on Linux (`~/.steam/steam/...`) + Windows registry
- `detect_locale(game_dir)`: parses Steam `localconfig.vdf` binary VDF for app 392160 active language — currently unused (hard-coded English `l044`)

## Phase Status

| Phase | Status | Notes |
|---|---|---|
| 1 — 2D Universe Map | ✅ done | DLC sectors, faction-colored hexes, connections, clusters |
| 2 — 3D Sector View | ✅ done + bonus | Gates as 2D overlays, stations from god.xml, full property panel |
| 3 — Live Data | 🔍 spec rewrite | HTTP API spec was stale; current plan: parse save-game XML (see `docs/superpowers/specs/2026-05-15-phase3-livedata.md`) |
| 4 — Search + Polish | ⏸ later | Search index over 144 sectors + 743 static objects |

## Conventions

- TDD: tests live with their crate. `map-domain` pure unit tests; `map-io` integration tests against fixtures; renderer verified visually per phase.
- Single-source data — `Universe`, `StaticObject` populated once on app start; mutate via well-defined paths only.
- Commits: Conventional Commits (`feat(scope):`, `fix(scope):`, etc.). Co-Authored-By Claude trailer.
- Worktree artifacts under `.claude/worktrees/` are gitignored intent — always check `git status` before `git add -A`.
