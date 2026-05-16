# Phase 3 — Live Data — Implementation Plan

> **For agentic workers:** Execute via `superpowers:subagent-driven-development` or `superpowers:executing-plans`. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Read X4 save-game XML on startup + on save-file change. Show ships, station owners, sector factions, player snapshot in UI.

**Architecture:** Streaming `quick_xml` parser over `flate2` gzip decoder produces a `World` + `FactionOverrides`. File watcher reparses on modify. UI binds to the latest snapshot.

**Tech Stack:** Rust 2024, `quick_xml`, `flate2`, `notify`, `crossbeam-channel`, existing `eframe`/`egui`/`wgpu`.

**Spec:** `docs/superpowers/specs/2026-05-15-phase3-livedata.md`

---

### Task 1 — Add `SnapshotMeta` to map-domain

**Files:**
- Modify: `crates/map-domain/src/world.rs` — add struct
- Modify: `crates/map-domain/src/lib.rs` — re-export

- [ ] **Step 1: Add struct + tests**

```rust
// in world.rs
#[derive(Debug, Clone)]
pub struct SnapshotMeta {
    pub path: std::path::PathBuf,
    pub mtime: std::time::SystemTime,
    pub game_time_seconds: f32,
    pub player_money: u64,
    pub player_location_name: String,
}

#[cfg(test)]
mod meta_tests {
    use super::*;
    #[test]
    fn meta_construction() {
        let m = SnapshotMeta {
            path: "/tmp/save.xml.gz".into(),
            mtime: std::time::UNIX_EPOCH,
            game_time_seconds: 1734.285,
            player_money: 40000,
            player_location_name: "Argon Prime".into(),
        };
        assert_eq!(m.player_money, 40000);
    }
}
```

- [ ] **Step 2: Run `cargo test -p map-domain`**, expect new test passes.
- [ ] **Step 3: Commit** `feat(domain): add SnapshotMeta for save snapshots`

---

### Task 2 — Synthetic save fixture

**Files:**
- Create: `crates/map-io/tests/fixtures/mini_save.xml.gz`
- Create helper script `crates/map-io/tests/fixtures/make_mini_save.sh` (optional)

Hand-craft a 2-sector, 2-station, 3-ship save XML (10–50 lines uncompressed) and gzip it. Use it as the unit-test target.

- [ ] **Step 1:** Write the fixture XML (full content shown below).
- [ ] **Step 2:** `gzip` it into the fixtures dir.
- [ ] **Step 3:** Commit binary fixture + source XML.

Fixture XML (uncompressed):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<savegame>
  <info>
    <save name="#test" date="1761750881"/>
    <game version="800" build="580735" time="1734.285" start="x4ep1_gamestart_terran1"/>
    <player name="Test" location="{20004,10011}" money="40000"/>
  </info>
  <universe>
    <component class="galaxy" macro="xu_ep2_universe_macro">
      <component class="cluster" macro="cluster_01_macro">
        <component class="sector" macro="cluster_01_sector001_macro" owner="argon">
          <component class="zone">
            <component class="station" macro="station_arg_factory_01" owner="argon" id="[0x100]">
              <offset><position x="0" y="0" z="0"/></offset>
            </component>
            <component class="ship_l" macro="ship_arg_l_destroyer_01" owner="argon" id="[0x101]">
              <offset><position x="1000" y="0" z="2000"/></offset>
            </component>
          </component>
        </component>
      </component>
      <component class="cluster" macro="cluster_06_macro">
        <component class="sector" macro="cluster_06_sector001_macro" owner="teladi">
          <component class="zone">
            <component class="ship_s" macro="ship_tel_s_scout_01" owner="teladi" id="[0x102]">
              <offset><position x="500" y="50" z="-500"/></offset>
            </component>
            <component class="ship_m" macro="ship_tel_m_frigate_01" owner="teladi" id="[0x103]">
              <offset><position x="-200" y="0" z="100"/></offset>
            </component>
          </component>
        </component>
      </component>
    </component>
  </universe>
</savegame>
```

---

### Task 3 — Save parser skeleton

**Files:**
- Create: `crates/map-io/src/save_parser.rs`
- Modify: `crates/map-io/src/lib.rs` — `pub mod save_parser;`
- Modify: `crates/map-io/Cargo.toml` — add `flate2 = "1"`

- [ ] **Step 1: Write failing test**

```rust
// in save_parser.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini_save.xml.gz")
    }

    #[test]
    fn parse_mini_save_meta() {
        let (meta, _, _) = parse_save(&fixture_path()).unwrap();
        assert_eq!(meta.player_money, 40000);
        assert!((meta.game_time_seconds - 1734.285).abs() < 0.01);
    }
}
```

- [ ] **Step 2:** `cargo test -p map-io parse_mini_save_meta` — expect FAIL ("parse_save not found").

- [ ] **Step 3: Implement `parse_save`**

```rust
use std::path::Path;
use std::fs::File;
use std::io::BufReader;
use flate2::read::GzDecoder;
use quick_xml::Reader;
use quick_xml::events::Event;
use map_domain::world::{World, SnapshotMeta};
use map_domain::ids::SectorId;
use std::collections::HashMap;
use crate::ParseError;

pub type FactionOverrides = HashMap<SectorId, String>;

pub fn parse_save(path: &Path) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError> {
    let file = File::open(path).map_err(ParseError::Io)?;
    let mtime = file.metadata().ok().and_then(|m| m.modified().ok())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let gz = GzDecoder::new(file);
    let mut reader = Reader::from_reader(BufReader::new(gz));
    reader.config_mut().trim_text(true);

    let mut meta = SnapshotMeta {
        path: path.to_path_buf(),
        mtime,
        game_time_seconds: 0.0,
        player_money: 0,
        player_location_name: String::new(),
    };
    let world = World::new();
    let overrides = FactionOverrides::new();
    let mut buf = Vec::new();

    // Initial pass: read <info> block only for meta (parser exits info tree).
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) => match e.name().as_ref() {
                b"game" => {
                    if let Some(t) = attr_value(e, b"time") {
                        meta.game_time_seconds = t.parse().unwrap_or(0.0);
                    }
                }
                b"player" => {
                    if let Some(m) = attr_value(e, b"money") {
                        meta.player_money = m.parse().unwrap_or(0);
                    }
                    if let Some(loc) = attr_value(e, b"location") {
                        meta.player_location_name = loc;
                    }
                }
                _ => {}
            },
            Ok(Event::End(ref e)) if e.name().as_ref() == b"info" => break,
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }

    // Universe pass left for Task 4
    Ok((meta, world, overrides))
}

fn attr_value(e: &quick_xml::events::BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().filter_map(Result::ok)
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}
```

- [ ] **Step 4:** Re-run test, expect PASS for meta only.
- [ ] **Step 5: Commit** `feat(io): save_parser stub reads SnapshotMeta from save xml`

---

### Task 4 — Walk universe tree, collect entities

**Files:**
- Modify: `crates/map-io/src/save_parser.rs`

- [ ] **Step 1: Add failing tests for entity + faction extraction**

```rust
#[test]
fn parse_mini_save_entities() {
    let (_, world, _) = parse_save(&fixture_path()).unwrap();
    // 2 ships + 1 station + 2 ships = 5 entities expected? Actually: 1 station + 1 ship_l in sector A;
    // 1 ship_s + 1 ship_m in sector B = 4 entities total
    assert_eq!(world.names.len(), 4);
}

#[test]
fn parse_mini_save_factions() {
    let (_, _, overrides) = parse_save(&fixture_path()).unwrap();
    // 2 sector faction overrides expected
    assert_eq!(overrides.len(), 2);
}
```

- [ ] **Step 2:** Run — FAIL.

- [ ] **Step 3: Extend parser**

After the `<info>` exit, continue reading and maintain a depth-aware stack:

```rust
let mut current_sector_macro: Option<String> = None;
let mut sector_stack_depth: Vec<u32> = Vec::new();  // depth pop counter
let mut entity_macro: Option<(String, String, String, String)> = None; // (class, macro, owner, id)
let mut entity_pos: Option<(f32, f32, f32)> = None;

loop {
    match reader.read_event_into(&mut buf) {
        Ok(Event::Start(ref e)) if e.name().as_ref() == b"component" => {
            let class = attr_value(e, b"class").unwrap_or_default();
            let macro_ = attr_value(e, b"macro").unwrap_or_default();
            if class == "sector" {
                current_sector_macro = Some(macro_.clone());
                if let Some(owner) = attr_value(e, b"owner") {
                    // Save into overrides (need sector ID lookup; deferred to integration)
                    // ...
                }
            } else if matches!(class.as_str(), "station" | "ship_xs" | "ship_s" | "ship_m" | "ship_l" | "ship_xl") {
                entity_macro = Some((class, macro_, attr_value(e, b"owner").unwrap_or_default(), attr_value(e, b"id").unwrap_or_default()));
                entity_pos = None;
            }
        }
        Ok(Event::Empty(ref e)) if e.name().as_ref() == b"position" => {
            let x = attr_value(e, b"x").and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let y = attr_value(e, b"y").and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let z = attr_value(e, b"z").and_then(|s| s.parse().ok()).unwrap_or(0.0);
            entity_pos = Some((x, y, z));
        }
        Ok(Event::End(ref e)) if e.name().as_ref() == b"component" => {
            if let Some((class, macro_, owner, id)) = entity_macro.take() {
                if let Some((x, y, z)) = entity_pos.take() {
                    // Insert into world; convert id to EntityId; map class to LiveObjectKind
                    // ...
                }
            }
        }
        Ok(Event::Eof) => break,
        _ => {}
    }
    buf.clear();
}
```

Notes:
- `EntityId` from hex string `[0x100]` → parse `u32::from_str_radix(&hex[3..hex.len()-1], 16)`
- Class → `LiveObjectKind`: `station→Station`, `ship_s→ShipSmall`, etc.
- Faction extraction returns `(SectorId, String)` requires the `macro_to_id` lookup; pass it in as a parameter OR return raw macro strings and let the caller resolve

For now, return `FactionOverrides = HashMap<String /* sector_macro_lowercase */, String /* faction_name */>` to keep the parser standalone.

- [ ] **Step 4:** Iterate until tests pass.
- [ ] **Step 5: Commit** `feat(io): save_parser walks universe tree, extracts ships+stations+faction owners`

---

### Task 5 — Wire save-parsing into the app

**Files:**
- Modify: `crates/map-app/src/app.rs` — add `snapshot: Option<(SnapshotMeta, World)>` field
- Modify: `crates/map-app/src/main.rs` — initial load attempt at startup
- Modify: `crates/map-app/src/ui/top_bar.rs` — snapshot indicator + refresh button

- [ ] **Step 1: Add the field, attempt sync load at startup**

```rust
let save_dir = dirs::config_dir()
    .map(|c| c.join("EgoSoft/X4"))
    .and_then(|d| std::fs::read_dir(d).ok())
    .and_then(|mut iter| iter.next().and_then(|e| e.ok()))
    .map(|e| e.path().join("save"));
let snapshot = save_dir.and_then(|d| latest_save(&d)).and_then(|p| map_io::save_parser::parse_save(&p).ok());
```

- [ ] **Step 2: Render top-bar indicator**

If `snapshot.is_some()`, show `"Snapshot: <fmt mtime>  (<age> ago)"`. Else "No save loaded".

- [ ] **Step 3: Apply faction overrides to universe**

After parse, iterate overrides and update `Sector.faction` via a `FactionId` table (build at first sight of an unknown faction name).

- [ ] **Step 4: Commit** `feat(app): load most recent save on startup; show snapshot meta in top bar`

---

### Task 6 — Background-thread parse + channel

**Files:**
- Modify: `crates/map-app/src/app.rs`
- Modify: `crates/map-app/Cargo.toml` — add `crossbeam-channel = "0.5"`

- [ ] **Step 1:** Move parse off the main thread. On startup, spawn parser thread; receive `(SnapshotMeta, World, FactionOverrides)` over a `crossbeam_channel`.
- [ ] **Step 2:** In `ui()`, drain the channel non-blocking; update `App::snapshot`; request repaint.
- [ ] **Step 3:** Add manual refresh button that triggers another parse on the same thread pool (use a one-shot job).
- [ ] **Step 4: Commit** `feat(app): parse save on background thread; surface via channel`

---

### Task 7 — File watcher

**Files:**
- Create: `crates/map-io/src/save_watcher.rs`
- Modify: `crates/map-io/Cargo.toml` — add `notify = "8"`

- [ ] **Step 1:** Module exposes `pub fn watch_save_dir(dir, tx: Sender<PathBuf>) -> Result<RecommendedWatcher>`.
- [ ] **Step 2:** Debounce: collect events for 1 s after last modify, then send the latest path.
- [ ] **Step 3:** App spawns watcher at startup; converts watcher events into parse jobs via the channel from Task 6.
- [ ] **Step 4: Commit** `feat(io): notify-based save dir watcher`

---

### Task 8 — Render ships in 3D sector view

**Files:**
- Modify: `crates/map-app/src/ui/sector_view.rs`

- [ ] **Step 1:** Receive `Option<&World>` parameter from `App::ui`.
- [ ] **Step 2:** Iterate `world.entities_in_sector(current_sector)`; project each to screen via the existing camera; draw a small filled triangle (size ∝ `LiveObjectKind`: small=4px, medium=6px, large=10px, xl=14px) tinted by faction color.
- [ ] **Step 3:** Skip ships outside the current visible area (cheap early-out on `clip.w <= 0.0`).
- [ ] **Step 4: Commit** `feat(3d): render live ships from save snapshot`

---

### Task 9 — Per-sector ship count badge on 2D map

**Files:**
- Modify: `crates/map-app/src/ui/map_view.rs`

- [ ] **Step 1:** Receive `Option<&World>`; build per-sector count by iterating `world.sector_idx`.
- [ ] **Step 2:** Above each sector hex, draw a small rounded rect with the count if > 0; muted color for player-irrelevant counts, faction color when sector has one dominant owner.
- [ ] **Step 3: Commit** `feat(map): per-sector ship count badge from snapshot`

---

### Task 10 — Polish + handoff

- [ ] **Step 1:** Snapshot-age label refreshes every frame (show "now" / "2m ago" / "1h ago").
- [ ] **Step 2:** Empty-state copy in side panel when no snapshot loaded ("Save the game in X4 to see live data").
- [ ] **Step 3:** Run full `cargo test`; expect all pass plus new save_parser tests.
- [ ] **Step 4: Manual smoke test:** open app, verify count matches debug log; save in X4; verify reparse within 2 s.
- [ ] **Step 5:** Update `CLAUDE.md` Phase Status row from 🔍 → ✅.
- [ ] **Step 6: Commit** `feat: phase 3 complete — live data via save-game snapshots`

---

## Out-of-scope (Phase 4)

- Per-tick deltas / mod-driven HTTP server
- Save selector UI for multiple saves
- Per-ship orders / trade routes
- Search index updates from `World`
- Camera follow-ship animation
