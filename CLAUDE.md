# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`foundations-map` — Interactive 2D map viewer for X4 Foundations (space sim), written in Rust. Cargo workspace, edition 2024, cross-platform (Linux Wayland + Windows).

## Commands

```bash
cargo build                    # compile all crates
cargo run                      # build + run the app
cargo test                     # run all tests (34 tests across 6 suites)
cargo test <name>              # run single test by name substring
cargo clippy                   # lint
cargo fmt                      # format
```

## Workspace Structure

```
crates/
  map-domain/   # Pure data model: Sector, Universe, Connection, GateType, ViewMode, ids
  map-io/       # X4 file parsing: cat/dat archives, XML, game path + locale detection
  map-app/      # egui UI: 2D map, sector panel, top bar, theme
```

**map-domain** — no I/O, no egui. Core types only.  
**map-io** — reads X4 cat/dat archives, parses 4 XML files to build Universe.  
**map-app** — binary `foundations-map`, wgpu backend via eframe 0.34.2.

## Key Architecture

### X4 Data Loading (`map-io`)

`parse_galaxy_from_game(game_dir)` reads 4 game files from cat/dat archives:
- `maps/xu_ep2_universe/galaxy.xml` — cluster positions (absolute)
- `maps/xu_ep2_universe/clusters.xml` — sector placements (relative to cluster)
- `libraries/mapdefaults.xml` — sector macro → `{page,text}` name refs (all sources: main + all 5 DLC extensions via `read_all_game_files`)
- `t/0001-l044.xml` — English translations, page 20004 = sector names

Name resolution order:
1. mapdefaults.xml lookup → translation table
2. Derived text ID: `cluster_num * 10000 + sector_num * 10 + 1` (covers sectors absent from mapdefaults, e.g. Cluster_33–50, 709–725)
3. Fallback: prettified macro name

**cat/dat format**: `XX.cat` = index (one path per line: `path size ts md5`), `XX.dat` = raw concatenated data. `search_cat` computes offsets by summing sizes. Extension DLCs use `extensions/ego_dlc_*/ext_NN.cat`.

**DLC macro case normalization**: DLC sector macros in mapdefaults use lowercase (`cluster_709_sector001_macro`) while clusters.xml uses mixed case. All name_refs keys stored lowercase, all lookups `.to_lowercase()`.

### egui App (`map-app`)

`App::ui()` (eframe 0.34.2 API — uses `fn ui`, not `fn update`) lays out three panels:
- `egui::Panel::top` → `TopBar`
- `egui::Panel::right` → `SectorPanel` (220px, not resizable)  
- `egui::CentralPanel` → `MapView`

**MapView** — pan/zoom 2D hex map:
- Hexes scale with zoom: `hex_r = (zoom * 3.0).clamp(12.0, 80.0)`
- Fit-to-view on first frame and when window grows; `min_zoom` = fit zoom
- Connections drawn in universe space (zoom-invariant obstacle routing via `route_ctrl_point`)
- Quadratic bezier curves avoid intermediate sectors; deterministic tiebreak by sector ID pair
- Superhighway connections: `GATE_GREEN`, width 2.5; standard: `Color32::from_rgb(80,95,150)`, width 1.0
- Faction colors: 8-color palette with alpha 60 via `faction_fill(FactionId)`

**Theme** (`theme.rs`): dark purple/navy palette. `apply()` calls `ctx.set_global_style()` (not `set_style`). Uses `CornerRadius` (not `Rounding`).

### egui 0.34.2 API Notes

- `eframe::App` requires `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`
- Panels: `egui::Panel::top/right`, `.show_inside(ui, …)` (not `TopBottomPanel`/`SidePanel`/`.show(&ctx, …)`)
- `CornerRadius::same(n)` — takes `u8`, not `f32`
- `ctx.set_global_style(style)` — not `set_style`

## Game Path Detection

`game_path::detect()` checks standard Steam paths on Linux (`~/.steam/steam/…`) and Windows registry. `game_path::detect_locale(game_dir)` parses Steam `localconfig.vdf` binary VDF blob to find the active language for app 392160 — currently unused (hardcoded to English `l044`).
