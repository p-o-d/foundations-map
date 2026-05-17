# Phase 3 Polish — Station Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make live (save-derived) ships + stations first-class throughout the app — enumerated in side panel with categories + parent/child navigation, rendered as GPU meshes in 3D scene, hover-labelled with code + human name + faction; drop redundant god.xml stations; replace hardcoded faction palette with game's own colours; capture nested entities (docked ships, subordinate stations) in save parser.

**Architecture:** Static load (`map-io`) gains a `faction_parser` reading `libraries/factions.xml` + `libraries/colors.xml` to populate `Universe.faction_table: HashMap<FactionId, FactionMeta>`. Save parser (`save_parser/sector_chunk.rs`) replaces single-pending-entity gate with a `Vec<Pending>` stack, capturing parent_id + code attribute. `World` gains `parents`, `children`, `codes` maps. 3D `sector_view` renders every top-level live entity via the existing GPU pipeline (uniform buffer cap raised 128 → 2048). Sector panel restructured into collapsing categories with click-through navigation. Hover label drawn over object under cursor.

**Tech Stack:** Rust 2024, `quick_xml` (existing), `glam`, `egui` 0.34.2, `wgpu` via `eframe::egui_wgpu`. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-05-18-station-fixes-design.md`

---

## File Structure

**Created:**
- `crates/map-io/src/faction_parser.rs` — parse_factions_xml + parse_colors_xml.
- `crates/map-io/tests/fixtures/factions_mini.xml` — hand-crafted faction fixture (2 factions).
- `crates/map-io/tests/fixtures/colors_mini.xml` — matching color fixture.
- `crates/map-app/src/colors.rs` — `faction_color` + `faction_name` helpers used by all UI modules.

**Modified:**
- `crates/map-domain/src/universe.rs` — adds `FactionMeta`, `Universe.faction_table`, `Universe.faction_strings`.
- `crates/map-domain/src/world.rs` — adds `parents`, `children`, `codes`; extends `insert_entity` signature.
- `crates/map-domain/src/view.rs` — `ViewMode::SectorView` adds `selected_entity: Option<EntityId>` + transitions.
- `crates/map-domain/src/lib.rs` — re-exports `FactionMeta` if needed.
- `crates/map-io/src/lib.rs` — `pub mod faction_parser;`.
- `crates/map-io/src/xml_parser.rs` — wires faction load; drops god station load + parser.
- `crates/map-io/src/save_parser/types.rs` — `EntityRecord` adds `parent_id`, `code`; renames `name` → `macro_name`.
- `crates/map-io/src/save_parser/sector_chunk.rs` — stack-based capture, parent_id, code, position-depth gate.
- `crates/map-io/src/save_parser/merge.rs` — accepts shared `faction_strings: &mut HashMap<String, FactionId>` + `next_faction_id: &mut u32`; populates parents/children/codes.
- `crates/map-io/src/save_parser/mod.rs` — orchestrator threads new params.
- `crates/map-app/src/main.rs` — passes faction tables in; updates `apply_faction_overrides`.
- `crates/map-app/src/app.rs` — `selected_entity` plumbing; passes world to panel.
- `crates/map-app/src/ui/sector_panel.rs` — restructured: collapsing categories, live-entity rows, DOCKED list, back-to-parent.
- `crates/map-app/src/ui/sector_view.rs` — `build_draw_calls` extended w/ world; `pick_target` returns `ClickedTarget` enum; hover label; PALETTE removed; `draw_live_ships` deleted.
- `crates/map-app/src/ui/map_view.rs` — PALETTE removed; uses `colors::faction_color`; ship-count badge tinted by dominant faction.
- `crates/map-app/src/renderer/gpu.rs` — `MAX_OBJECTS: 128 → 2048`.
- `CLAUDE.md` — Data Loading + counts updated.

**Deleted (within `xml_parser.rs`):**
- `parse_god_stations_xml` function (~200 LOC, lines ~1239–1450).
- Its call site (~lines 332–355).
- Its `#[cfg(test)]` tests.

---

### Task 1: Add `FactionMeta` + Universe fields

**Files:**
- Modify: `crates/map-domain/src/universe.rs`
- Modify: `crates/map-domain/src/lib.rs`

- [ ] **Step 1: Add struct + Universe fields**

Edit `crates/map-domain/src/universe.rs`. After the `use` lines, before `pub enum GateType`, add:

```rust
#[derive(Debug, Clone)]
pub struct FactionMeta {
    pub display_name: String,
    pub color: [u8; 4],
}
```

In the existing `pub struct Universe` block, after `connections`, add:

```rust
    /// Lowercase X4 faction id (e.g. "argon") → FactionId. Built at static load.
    pub faction_strings: std::collections::HashMap<String, crate::ids::FactionId>,
    /// FactionId → resolved display name + game palette colour.
    pub faction_table: std::collections::HashMap<crate::ids::FactionId, FactionMeta>,
```

In the `impl Default for Universe { fn default() -> Self {` (or `Default` derive — check actual code; if derived, the new fields' `HashMap::new()` defaults are fine — `Default` works).

- [ ] **Step 2: Add a test**

Append to `crates/map-domain/src/universe.rs` inside the existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn faction_table_holds_meta() {
        let mut u = Universe::default();
        u.faction_strings.insert("argon".into(), FactionId(1));
        u.faction_table.insert(
            FactionId(1),
            FactionMeta { display_name: "Argon Federation".into(), color: [50, 120, 255, 255] },
        );
        assert_eq!(u.faction_strings.get("argon"), Some(&FactionId(1)));
        assert_eq!(u.faction_table.get(&FactionId(1)).unwrap().display_name, "Argon Federation");
    }
```

- [ ] **Step 3: Build + test**

Run:
```bash
cargo test -p map-domain --lib universe::tests::faction_table_holds_meta 2>&1 | tail -5
```
Expected: `1 passed`.

- [ ] **Step 4: Re-export from lib**

Edit `crates/map-domain/src/lib.rs`. The file is small; check whether `universe::FactionMeta` needs explicit re-export. If `lib.rs` has `pub use universe::*;`, no change. If selective re-exports, add `FactionMeta` alongside `Universe`. After edit, re-run `cargo build -p map-domain`.

- [ ] **Step 5: Commit**

```bash
git add crates/map-domain/src/universe.rs crates/map-domain/src/lib.rs
git commit -m "feat(domain): FactionMeta + Universe.faction_table/faction_strings"
```

---

### Task 2: Faction parser — fixture + `parse_factions_xml`

**Files:**
- Create: `crates/map-io/tests/fixtures/factions_mini.xml`
- Create: `crates/map-io/src/faction_parser.rs`
- Modify: `crates/map-io/src/lib.rs`

- [ ] **Step 1: Write the fixture**

Create `crates/map-io/tests/fixtures/factions_mini.xml`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<factions xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="factions.xsd">
  <faction id="argon" name="{20203,201}" description="{20203,202}" />
  <faction id="teladi" name="{20203,1801}" description="{20203,1802}" />
</factions>
```

- [ ] **Step 2: Add module hook in lib.rs**

Edit `crates/map-io/src/lib.rs`. Add:

```rust
pub mod faction_parser;
```

- [ ] **Step 3: Write the failing test in `faction_parser.rs`**

Create `crates/map-io/src/faction_parser.rs`:

```rust
//! Parse `libraries/factions.xml` + `libraries/colors.xml` to build a per-faction
//! mapping of display-name translation refs and palette colour.

use std::collections::HashMap;

/// One faction entry as read from `libraries/factions.xml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactionDef {
    /// Translation reference: (page_id, text_id).
    pub name_textref: (u32, u32),
    /// The mapping id used in `libraries/colors.xml`, e.g. "faction_argon".
    pub color_mapping: String,
}

/// Parse the XML body of a `libraries/factions.xml` file and return a map of
/// lowercase faction-id → FactionDef.
pub fn parse_factions_xml(xml: &str) -> HashMap<String, FactionDef> {
    // Implementation in Step 5.
    let _ = xml;
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_factions() -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/factions_mini.xml"),
        )
        .unwrap()
    }

    #[test]
    fn parse_factions_extracts_id_and_name_textref() {
        let map = parse_factions_xml(&fixture_factions());
        assert_eq!(map.len(), 2);
        let argon = map.get("argon").expect("argon present");
        assert_eq!(argon.name_textref, (20203, 201));
        assert_eq!(argon.color_mapping, "faction_argon");

        let teladi = map.get("teladi").expect("teladi present");
        assert_eq!(teladi.name_textref, (20203, 1801));
        assert_eq!(teladi.color_mapping, "faction_teladi");
    }
}
```

- [ ] **Step 4: Run — expect FAIL**

```bash
cargo test -p map-io --lib faction_parser::tests::parse_factions_extracts_id_and_name_textref 2>&1 | tail -5
```
Expected: assertion failure (`map.len()` is 0, not 2).

- [ ] **Step 5: Implement `parse_factions_xml`**

Replace the body in `faction_parser.rs`:

```rust
pub fn parse_factions_xml(xml: &str) -> HashMap<String, FactionDef> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = HashMap::new();

    fn parse_textref(s: &str) -> Option<(u32, u32)> {
        // Format: "{20203,201}" or "{20203, 201}".
        let inner = s.trim().strip_prefix('{')?.strip_suffix('}')?;
        let mut parts = inner.split(',');
        let p = parts.next()?.trim().parse().ok()?;
        let t = parts.next()?.trim().parse().ok()?;
        Some((p, t))
    }

    fn handle_faction(
        e: &quick_xml::events::BytesStart<'_>,
        out: &mut HashMap<String, FactionDef>,
    ) {
        let mut id_opt: Option<String> = None;
        let mut name_opt: Option<(u32, u32)> = None;
        for attr in e.attributes().filter_map(Result::ok) {
            let key = attr.key.as_ref();
            let val = String::from_utf8_lossy(&attr.value).into_owned();
            match key {
                b"id" => id_opt = Some(val.to_lowercase()),
                b"name" => name_opt = parse_textref(&val),
                _ => {}
            }
        }
        if let (Some(id), Some(textref)) = (id_opt, name_opt) {
            let color_mapping = format!("faction_{}", id);
            out.insert(id, FactionDef { name_textref: textref, color_mapping });
        }
    }

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) if e.name().as_ref() == b"faction" => {
                handle_faction(e, &mut out);
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}
```

- [ ] **Step 6: Run — expect PASS**

```bash
cargo test -p map-io --lib faction_parser::tests::parse_factions_extracts_id_and_name_textref 2>&1 | tail -5
```
Expected: `1 passed`.

- [ ] **Step 7: Commit**

```bash
git add crates/map-io/src/faction_parser.rs \
        crates/map-io/src/lib.rs \
        crates/map-io/tests/fixtures/factions_mini.xml
git commit -m "feat(io): faction_parser reads libraries/factions.xml"
```

---

### Task 3: Colors parser + mapping resolution

**Files:**
- Create: `crates/map-io/tests/fixtures/colors_mini.xml`
- Modify: `crates/map-io/src/faction_parser.rs`

- [ ] **Step 1: Write the fixture**

Create `crates/map-io/tests/fixtures/colors_mini.xml`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<colormap xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="colors.xsd">
  <colors>
    <color id="azure_dark" r="40" g="100" b="180" a="220"/>
    <color id="yellow_dark" r="180" g="160" b="40" a="220"/>
    <color id="grey_192" r="192" g="192" b="192" a="255"/>
  </colors>
  <mappings>
    <mapping id="faction_argon" ref="azure_dark"/>
    <mapping id="faction_teladi" ref="yellow_dark"/>
    <mapping id="faction_ownerless" ref="grey_192"/>
  </mappings>
</colormap>
```

- [ ] **Step 2: Add the failing test**

Append to the `#[cfg(test)] mod tests` block in `crates/map-io/src/faction_parser.rs`:

```rust
    fn fixture_colors() -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/colors_mini.xml"),
        )
        .unwrap()
    }

    #[test]
    fn parse_colors_resolves_mapping_chain() {
        let (colors, mappings) = parse_colors_xml(&fixture_colors());
        assert_eq!(colors.get("azure_dark"), Some(&[40, 100, 180, 220]));
        assert_eq!(mappings.get("faction_argon"), Some(&"azure_dark".to_string()));

        let resolved = resolve_faction_color("faction_argon", &colors, &mappings);
        assert_eq!(resolved, Some([40, 100, 180, 220]));

        let missing = resolve_faction_color("faction_unknown", &colors, &mappings);
        assert_eq!(missing, None);
    }
```

- [ ] **Step 3: Run — expect FAIL** (functions not defined)

```bash
cargo test -p map-io --lib faction_parser::tests::parse_colors_resolves_mapping_chain 2>&1 | tail -5
```

- [ ] **Step 4: Implement `parse_colors_xml` + `resolve_faction_color`**

Append to `faction_parser.rs` (above the `#[cfg(test)] mod tests` line):

```rust
/// Parse `libraries/colors.xml`. Returns two maps:
/// 1. color id → RGBA bytes (e.g. "azure_dark" → [40,100,180,220])
/// 2. mapping id → color-ref id (e.g. "faction_argon" → "azure_dark")
pub fn parse_colors_xml(xml: &str) -> (HashMap<String, [u8; 4]>, HashMap<String, String>) {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut colors = HashMap::new();
    let mut mappings = HashMap::new();

    fn attr_str(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
        e.attributes().filter_map(Result::ok)
            .find(|a| a.key.as_ref() == name)
            .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
    }
    fn attr_u8(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<u8> {
        attr_str(e, name)?.parse().ok()
    }

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                match e.name().as_ref() {
                    b"color" => {
                        if let Some(id) = attr_str(e, b"id") {
                            let r = attr_u8(e, b"r").unwrap_or(0);
                            let g = attr_u8(e, b"g").unwrap_or(0);
                            let b = attr_u8(e, b"b").unwrap_or(0);
                            let a = attr_u8(e, b"a").unwrap_or(255);
                            colors.insert(id, [r, g, b, a]);
                        }
                    }
                    b"mapping" => {
                        if let (Some(id), Some(rf)) = (attr_str(e, b"id"), attr_str(e, b"ref")) {
                            mappings.insert(id, rf);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    (colors, mappings)
}

/// Resolve a faction's mapping id to its RGBA. None if either the mapping or
/// the referenced colour entry is absent.
pub fn resolve_faction_color(
    mapping_id: &str,
    colors: &HashMap<String, [u8; 4]>,
    mappings: &HashMap<String, String>,
) -> Option<[u8; 4]> {
    let color_id = mappings.get(mapping_id)?;
    colors.get(color_id).copied()
}
```

- [ ] **Step 5: Run — expect PASS**

```bash
cargo test -p map-io --lib faction_parser::tests 2>&1 | tail -5
```
Expected: `2 passed`.

- [ ] **Step 6: Commit**

```bash
git add crates/map-io/src/faction_parser.rs \
        crates/map-io/tests/fixtures/colors_mini.xml
git commit -m "feat(io): parse colors.xml + resolve faction color via mapping"
```

---

### Task 4: Wire factions/colors into Universe at static load

**Files:**
- Modify: `crates/map-io/src/xml_parser.rs`
- Modify: `crates/map-domain/src/ids.rs` (only if `FactionId` is missing `Copy/Hash/Eq/PartialEq` — check, likely already derived)

- [ ] **Step 1: Confirm FactionId derives**

Run:
```bash
grep -n "FactionId" crates/map-domain/src/ids.rs
```
Expected: a derive line containing `Copy, Clone, Debug, Eq, PartialEq, Hash` (or equivalent). If `Hash` or `Eq` is missing, add them — `Universe.faction_table` keys on `FactionId`.

- [ ] **Step 2: In `xml_parser.rs`, add the faction-load block**

Locate the section near the top of `parse_galaxy_from_game` where `mapdefaults` + translation are loaded (search for `mapdefaults.xml`). Just after the translation table is built, add:

```rust
    // ---- Faction metadata: name + color from libraries/factions.xml + colors.xml.
    let mut faction_defs: std::collections::HashMap<String, crate::faction_parser::FactionDef> =
        std::collections::HashMap::new();
    for (_path, data) in
        crate::cat_reader::read_all_game_files(game_dir, "libraries/factions.xml")
    {
        let text = String::from_utf8_lossy(&data);
        for (k, v) in crate::faction_parser::parse_factions_xml(&text) {
            faction_defs.entry(k).or_insert(v); // first occurrence wins; main usually comes first
        }
    }
    let mut colors_map: std::collections::HashMap<String, [u8; 4]> =
        std::collections::HashMap::new();
    let mut mappings_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (_path, data) in
        crate::cat_reader::read_all_game_files(game_dir, "libraries/colors.xml")
    {
        let text = String::from_utf8_lossy(&data);
        let (c, m) = crate::faction_parser::parse_colors_xml(&text);
        colors_map.extend(c);
        mappings_map.extend(m);
    }
    eprintln!(
        "[map] Faction defs: {}, colors: {}, mappings: {}",
        faction_defs.len(), colors_map.len(), mappings_map.len()
    );
```

- [ ] **Step 3: After the universe is built, populate `faction_table` + `faction_strings`**

At the end of `parse_galaxy_from_game`, just before the final `Ok(universe)` (or `Ok(Universe { ... })`) line, insert:

```rust
    // Assign sequential FactionIds and populate the faction table.
    let mut next_id: u32 = 1;
    for (faction_id_str, def) in &faction_defs {
        let fid = map_domain::ids::FactionId(next_id);
        next_id += 1;
        let display_name = translation
            .get(&def.name_textref)
            .cloned()
            .unwrap_or_else(|| faction_id_str.clone());
        let color = crate::faction_parser::resolve_faction_color(
            &def.color_mapping, &colors_map, &mappings_map,
        )
        .unwrap_or([192, 192, 192, 255]);
        universe.faction_strings.insert(faction_id_str.clone(), fid);
        universe
            .faction_table
            .insert(fid, map_domain::universe::FactionMeta { display_name, color });
    }
    eprintln!(
        "[map] Built faction table: {} factions",
        universe.faction_table.len()
    );
```

(Names of local vars `universe`, `translation` come from existing code — if they differ, adjust. `translation` is the existing `HashMap<(u32,u32), String>`.)

- [ ] **Step 4: Build + run**

```bash
cargo build 2>&1 | grep "^error" | head -5
```
Expected: no errors. If `translation` map type differs, fix the lookup signature here.

- [ ] **Step 5: Smoke-test against real game**

```bash
timeout 15 cargo run --release 2>&1 | grep -E "^\[map\] (Faction|Built)" | head -5
```
Expected: lines like `[map] Faction defs: 30+`, `[map] Built faction table: 30+ factions`.

- [ ] **Step 6: Commit**

```bash
git add crates/map-io/src/xml_parser.rs crates/map-domain/src/ids.rs
git commit -m "feat(io): wire faction_parser + colors.xml into Universe at load"
```

---

### Task 5: Drop god.xml `<station>` parsing

**Files:**
- Modify: `crates/map-io/src/xml_parser.rs`

- [ ] **Step 1: Locate the call site**

Search for the god station load block:
```bash
grep -n "parse_god_stations_xml\|50_000 + station_counter\|God stations loaded" crates/map-io/src/xml_parser.rs
```
You should see (lines may differ):
- The function definition `fn parse_god_stations_xml(...)` (~line 1239 onward).
- The call site that iterates results and inserts into a sector (~lines 332–355).
- The log line `[map] God stations loaded: …`.

- [ ] **Step 2: Delete the call site**

In `crates/map-io/src/xml_parser.rs`, remove the entire block that begins:

```rust
    // Parse god.xml stations (main + all DLC extensions).
    let mut station_counter = 0u32;
    for (_path, god_data) in crate::cat_reader::read_all_game_files(game_dir, "libraries/god.xml") {
        ...
    }
    eprintln!("[map] God stations loaded: {}", station_counter);
```

…and any nested `for (sec_lower, pos, rot, name, details) in parse_god_stations_xml(&god_str) { ... }` body.

- [ ] **Step 3: Delete the function**

Remove the entire `fn parse_god_stations_xml` and any `#[cfg(test)]` tests that exercise it (search for `parse_god_stations_xml` in `xml_parser.rs` and remove call sites in tests too).

- [ ] **Step 4: Drop the unused 50k+ ObjectId range comment**

If any inline comment mentions `50_000+` or `50k+`, remove or correct it.

- [ ] **Step 5: Build + test**

```bash
cargo build 2>&1 | grep "^error" | head -5
```
Expected: no errors.

```bash
cargo test -p map-io 2>&1 | tail -5
```
Expected: existing tests still pass; any test that asserted god station counts must be removed in this task too.

- [ ] **Step 6: Smoke-test**

```bash
timeout 15 cargo run --release 2>&1 | grep "^\[map\]" | head -15
```
Expected: no `God stations loaded` line anymore. Other counts unchanged. Sectors should still have gates / wormholes (god objects).

- [ ] **Step 7: Commit**

```bash
git add crates/map-io/src/xml_parser.rs
git commit -m "feat(io): drop god.xml <station> parsing — save is authoritative"
```

---

### Task 6: EntityRecord adds parent_id + code; renames name → macro_name

**Files:**
- Modify: `crates/map-io/src/save_parser/types.rs`

- [ ] **Step 1: Update struct**

Replace `EntityRecord` in `crates/map-io/src/save_parser/types.rs` with:

```rust
#[derive(Debug, Clone)]
pub struct EntityRecord {
    pub id: u32,
    pub parent_id: Option<u32>,
    pub macro_name: String,
    pub code: Option<String>,
    pub kind: LiveObjectKind,
    pub owner: Option<String>,
    pub position: glam::Vec3,
    pub sector_macro: String,
}
```

- [ ] **Step 2: Fix the existing inline tests**

In the same file's `#[cfg(test)] mod tests`, update `entity_record_constructs`:

```rust
    #[test]
    fn entity_record_constructs() {
        let e = EntityRecord {
            id: 0x100,
            parent_id: None,
            macro_name: "station_arg_factory_01".into(),
            code: Some("YIB-942".into()),
            kind: LiveObjectKind::Station,
            owner: Some("argon".into()),
            position: glam::Vec3::new(0.0, 0.0, 0.0),
            sector_macro: "cluster_01_sector001_macro".into(),
        };
        assert_eq!(e.id, 0x100);
        assert_eq!(e.parent_id, None);
        assert_eq!(e.code.as_deref(), Some("YIB-942"));
        assert_eq!(e.owner.as_deref(), Some("argon"));
    }
```

- [ ] **Step 3: Build (other files using `.name` field will fail — expected)**

```bash
cargo build -p map-io 2>&1 | grep "^error" | head -5
```

Expected errors in `sector_chunk.rs` + `merge.rs` referring to `.name` no longer existing. Note them; they're fixed in Tasks 8 and 9.

- [ ] **Step 4: Make sector_chunk.rs + merge.rs compile** (minimal renames only — full stack logic comes in Task 8)

In `crates/map-io/src/save_parser/sector_chunk.rs`:
- Change every `name:` field initialiser and every `.name` field access on EntityRecord to `macro_name`. Locate via:
  ```bash
  grep -n '\bname\b' crates/map-io/src/save_parser/sector_chunk.rs
  ```
- Update the `Pending` struct similarly (`name` → `macro_name`).
- In each `out.push(EntityRecord { ... })` add `parent_id: None,` and `code: None,` fields (full logic comes in Task 8; for now `None` keeps it compiling and Task 5's tests passing).

In `crates/map-io/src/save_parser/merge.rs`:
- Wherever `r.name` is read, use `r.macro_name`. The current code does `world.insert_entity(r.id, r.name, …)` — update to `r.macro_name`. Insertion of code + parent comes in Task 9.

- [ ] **Step 5: Verify tests still pass with the rename**

```bash
cargo test -p map-io --lib save_parser 2>&1 | tail -8
```
Expected: all current save_parser tests pass (parent_id is just `None` everywhere for now).

- [ ] **Step 6: Commit**

```bash
git add crates/map-io/src/save_parser/types.rs \
        crates/map-io/src/save_parser/sector_chunk.rs \
        crates/map-io/src/save_parser/merge.rs
git commit -m "feat(io): EntityRecord adds parent_id, code; renames name → macro_name"
```

---

### Task 7: World adds parents/children/codes; extends `insert_entity`

**Files:**
- Modify: `crates/map-domain/src/world.rs`

- [ ] **Step 1: Add fields**

In `crates/map-domain/src/world.rs`, inside `pub struct World`, after `sector_idx`, add:

```rust
    pub parents: HashMap<EntityId, EntityId>,
    pub children: HashMap<EntityId, Vec<EntityId>>,
    pub codes: HashMap<EntityId, String>,
```

(All inherit `#[derive(Default)]` so no `Default::default()` body change.)

- [ ] **Step 2: Extend `insert_entity` signature**

Replace the existing `insert_entity` method:

```rust
    pub fn insert_entity(
        &mut self,
        id: EntityId,
        name: String,
        kind: LiveObjectKind,
        faction: Option<FactionId>,
        position: Vec3,
        sector: SectorId,
        parent: Option<EntityId>,
        code: Option<String>,
    ) {
        self.names.insert(id, name);
        self.kinds.insert(id, kind);
        if let Some(f) = faction {
            self.factions.insert(id, f);
        }
        self.positions.insert(id, position);
        self.sectors.insert(id, sector);
        self.sector_idx.entry(sector).or_default().push(id);
        if let Some(p) = parent {
            self.parents.insert(id, p);
            self.children.entry(p).or_default().push(id);
        }
        if let Some(c) = code {
            self.codes.insert(id, c);
        }
    }

    pub fn parent_of(&self, id: EntityId) -> Option<EntityId> {
        self.parents.get(&id).copied()
    }

    pub fn children_of(&self, id: EntityId) -> &[EntityId] {
        self.children.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }
```

- [ ] **Step 3: Update existing tests in same file** (every `insert_entity` call site)

Search:
```bash
grep -n "insert_entity" crates/map-domain/src/world.rs
```
Update each call: add `None, None` at the end (parent + code).

- [ ] **Step 4: Add new test**

Append to the existing `mod tests`:

```rust
    #[test]
    fn parent_child_links_track_correctly() {
        let mut w = World::new();
        w.insert_entity(
            1, "station".into(), LiveObjectKind::Station,
            None, Vec3::ZERO, sector_a(), None, Some("YIB-1".into()),
        );
        w.insert_entity(
            2, "drone".into(), LiveObjectKind::ShipSmall,
            None, Vec3::ZERO, sector_a(), Some(1), None,
        );
        assert_eq!(w.parent_of(2), Some(1));
        assert_eq!(w.children_of(1), &[2]);
        assert_eq!(w.parent_of(1), None);
        assert_eq!(w.codes.get(&1).map(String::as_str), Some("YIB-1"));
    }
```

- [ ] **Step 5: Build + test**

```bash
cargo test -p map-domain --lib 2>&1 | tail -5
```
Expected: all map-domain tests pass including the new one.

(Note: `cargo build` workspace-wide will fail because `map-io::save_parser::merge` still calls `insert_entity` with the old arity. We'll fix that in Task 9.)

- [ ] **Step 6: Commit**

```bash
git add crates/map-domain/src/world.rs
git commit -m "feat(domain): World adds parents/children/codes maps"
```

---

### Task 8: Stack-based save parser (Section 2 core)

**Files:**
- Modify: `crates/map-io/src/save_parser/sector_chunk.rs`

- [ ] **Step 1: Write the failing tests**

Append to `#[cfg(test)] mod tests` in `crates/map-io/src/save_parser/sector_chunk.rs`:

```rust
    #[test]
    fn nested_ship_inside_station_emits_two_records_with_parent_link() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="zone">
    <component class="station" macro="station_arg_factory_01" owner="argon" code="YIB-1" id="[0x100]">
      <offset><position x="0" y="0" z="0"/></offset>
      <connections>
        <component class="ship_xs" macro="ship_xs_drone_01" owner="argon" id="[0x200]">
          <offset><position x="500" y="0" z="0"/></offset>
        </component>
      </connections>
    </component>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        assert_eq!(out.len(), 2);

        let station = out.iter().find(|r| r.id == 0x100).unwrap();
        assert_eq!(station.parent_id, None);
        assert_eq!(station.code.as_deref(), Some("YIB-1"));
        assert_eq!(station.kind, map_domain::world::LiveObjectKind::Station);

        let drone = out.iter().find(|r| r.id == 0x200).unwrap();
        assert_eq!(drone.parent_id, Some(0x100));
        assert_eq!(drone.kind, map_domain::world::LiveObjectKind::ShipSmall);
        // Position is captured relative to drone's own offset (parent-local).
        assert!((drone.position.x - 0.5).abs() < 1e-3);
    }

    #[test]
    fn three_level_nesting_emits_chain() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" macro="parent_st" owner="argon" code="P-1" id="[0x10]">
    <offset><position x="0" y="0" z="0"/></offset>
    <component class="ship_l" macro="carrier" owner="argon" code="C-1" id="[0x20]">
      <offset><position x="1000" y="0" z="0"/></offset>
      <component class="ship_xs" macro="drone" owner="argon" id="[0x30]">
        <offset><position x="100" y="0" z="0"/></offset>
      </component>
    </component>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        assert_eq!(out.len(), 3);
        let p = out.iter().find(|r| r.id == 0x10).unwrap();
        let c = out.iter().find(|r| r.id == 0x20).unwrap();
        let d = out.iter().find(|r| r.id == 0x30).unwrap();
        assert_eq!(p.parent_id, None);
        assert_eq!(c.parent_id, Some(0x10));
        assert_eq!(d.parent_id, Some(0x20));
    }

    #[test]
    fn nested_offset_does_not_overwrite_parent_position() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" owner="argon" id="[0xA]">
    <offset><position x="7000" y="0" z="0"/></offset>
    <component class="ship_s" owner="argon" id="[0xB]">
      <offset><position x="1" y="0" z="0"/></offset>
    </component>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let st = out.iter().find(|r| r.id == 0xA).unwrap();
        assert!((st.position.x - 7.0).abs() < 1e-3);
    }
```

- [ ] **Step 2: Run — expect FAIL on new tests**

```bash
cargo test -p map-io --lib save_parser::sector_chunk::tests 2>&1 | tail -10
```
Expected: existing 2 tests still pass; 3 new tests fail (current parser only emits the outermost entity).

- [ ] **Step 3: Rewrite `parse_sector_chunk` with stack-based capture**

Replace the body of `parse_sector_chunk` and the `Pending` struct in `sector_chunk.rs` with:

```rust
pub fn parse_sector_chunk(slice: &[u8], sector_macro: &str) -> Vec<EntityRecord> {
    let mut reader = Reader::from_reader(slice);
    reader.config_mut().trim_text(true);

    let mut out: Vec<EntityRecord> = Vec::new();
    let mut buf: Vec<u8> = Vec::new();

    let mut comp_depth: u32 = 0;
    let mut stack: Vec<Pending> = Vec::new();
    // Depth at which we are currently inside an <offset> element; None otherwise.
    let mut offset_depth: Option<u32> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"component" => {
                comp_depth += 1;
                if let Some(p) = build_pending(e, comp_depth, stack.last().map(|sp| sp.id)) {
                    stack.push(p);
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"component" => {
                if let Some(top) = stack.last() {
                    if top.open_depth == comp_depth {
                        let p = stack.pop().unwrap();
                        out.push(EntityRecord {
                            id: p.id,
                            parent_id: p.parent_id,
                            macro_name: p.macro_name,
                            code: p.code,
                            kind: p.kind,
                            owner: p.owner,
                            position: p.position.unwrap_or(Vec3::ZERO),
                            sector_macro: sector_macro.to_string(),
                        });
                    }
                }
                comp_depth = comp_depth.saturating_sub(1);
            }
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"offset" => {
                offset_depth = Some(comp_depth);
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"offset" => {
                offset_depth = None;
            }
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"position" => {
                // Only attribute position to the top pending entity if THIS <offset> sits
                // immediately inside it (open_depth + 1 == offset_depth). That prevents a
                // nested child's <offset> from overwriting its parent's position.
                if let (Some(top), Some(od)) = (stack.last_mut(), offset_depth) {
                    if top.open_depth + 1 == od && top.position.is_none() {
                        let x = attr_f32(e, b"x").unwrap_or(0.0);
                        let y = attr_f32(e, b"y").unwrap_or(0.0);
                        let z = attr_f32(e, b"z").unwrap_or(0.0);
                        // X4 stores positions in metres; convert to km.
                        top.position = Some(Vec3::new(x / 1000.0, y / 1000.0, z / 1000.0));
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

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

fn build_pending(e: &BytesStart<'_>, depth: u32, parent_id: Option<u32>) -> Option<Pending> {
    let class = attr_str(e, b"class")?;
    let kind = match class.as_str() {
        "station" => LiveObjectKind::Station,
        "ship_xs" | "ship_s" => LiveObjectKind::ShipSmall,
        "ship_m" => LiveObjectKind::ShipMedium,
        "ship_l" => LiveObjectKind::ShipLarge,
        "ship_xl" => LiveObjectKind::ShipExtraLarge,
        _ => return None,
    };
    let id_str = attr_str(e, b"id")?;
    let id = parse_entity_id(&id_str)?;
    let macro_name = attr_str(e, b"macro").unwrap_or_else(|| class.clone());
    let code = attr_str(e, b"code");
    let owner = attr_str(e, b"owner");

    Some(Pending {
        open_depth: depth,
        id,
        parent_id,
        macro_name,
        code,
        kind,
        owner,
        position: None,
    })
}
```

(The helpers `parse_entity_id`, `attr_str`, `attr_f32` already exist below; keep them as-is.)

- [ ] **Step 4: Run all `sector_chunk` tests — expect all PASS**

```bash
cargo test -p map-io --lib save_parser::sector_chunk::tests 2>&1 | tail -10
```
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/save_parser/sector_chunk.rs
git commit -m "feat(io): stack-based save parser captures nested entities + parent_id + code"
```

---

### Task 9: Merge — populate parents/children/codes; accept shared faction map

**Files:**
- Modify: `crates/map-io/src/save_parser/merge.rs`
- Modify: `crates/map-io/src/save_parser/mod.rs`

- [ ] **Step 1: Update merge signature + body**

Replace the `merge` function in `crates/map-io/src/save_parser/merge.rs`:

```rust
//! Stage 4: merge per-worker entity records into a single `World`.

use std::collections::HashMap;

use map_domain::ids::{FactionId, SectorId};
use map_domain::world::World;

use super::types::EntityRecord;

/// Combine all per-worker entity lists into a single `World`. Resolves each
/// record's `sector_macro` via `sector_macros` (drops records whose sector
/// isn't known). For new owner strings not already present in `faction_strings`,
/// allocates the next FactionId.
pub fn merge(
    batches: Vec<Vec<EntityRecord>>,
    sector_macros: Option<&HashMap<String, SectorId>>,
    faction_strings: &mut HashMap<String, FactionId>,
    next_faction_id: &mut u32,
) -> World {
    let mut world = World::new();
    let Some(sector_macros) = sector_macros else {
        return world;
    };

    for batch in batches {
        for r in batch {
            let Some(&sec_id) = sector_macros.get(&r.sector_macro) else {
                continue;
            };
            let faction = r.owner.map(|name| {
                let name = name.to_lowercase();
                *faction_strings.entry(name).or_insert_with(|| {
                    let id = FactionId(*next_faction_id);
                    *next_faction_id += 1;
                    id
                })
            });
            world.insert_entity(
                r.id,
                r.macro_name,
                r.kind,
                faction,
                r.position,
                sec_id,
                r.parent_id,
                r.code,
            );
        }
    }
    world
}
```

- [ ] **Step 2: Update merge tests**

Existing tests in `merge.rs` pass a `sector_macros` argument. They'll fail with the new signature. Refactor each test:

```rust
    #[test]
    fn merges_records_and_assigns_faction_ids() {
        let records = vec![
            EntityRecord {
                id: 0x10, parent_id: None, macro_name: "station_a".into(), code: None,
                kind: LiveObjectKind::Station, owner: Some("argon".into()),
                position: glam::Vec3::ZERO, sector_macro: "sa".into(),
            },
            EntityRecord {
                id: 0x11, parent_id: Some(0x10), macro_name: "drone".into(), code: Some("D-1".into()),
                kind: LiveObjectKind::ShipSmall, owner: Some("argon".into()),
                position: glam::Vec3::ZERO, sector_macro: "sa".into(),
            },
        ];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut next = 1u32;
        let world = merge(vec![records], Some(&sm), &mut fs, &mut next);
        assert_eq!(world.names.len(), 2);
        assert_eq!(world.parent_of(0x11), Some(0x10));
        assert_eq!(world.children_of(0x10), &[0x11]);
        assert_eq!(world.codes.get(&0x11).map(String::as_str), Some("D-1"));
        assert_eq!(fs.get("argon").copied(), Some(FactionId(1)));
        assert_eq!(next, 2);
    }

    #[test]
    fn unknown_sector_drops_entity() {
        let records = vec![EntityRecord {
            id: 0xFFFF, parent_id: None, macro_name: "x".into(), code: None,
            kind: LiveObjectKind::ShipSmall, owner: None,
            position: glam::Vec3::ZERO, sector_macro: "unknown".into(),
        }];
        let sm: HashMap<String, SectorId> = HashMap::new();
        let mut fs = HashMap::new();
        let mut next = 1u32;
        let world = merge(vec![records], Some(&sm), &mut fs, &mut next);
        assert!(world.names.is_empty());
    }

    #[test]
    fn no_sector_macros_drops_all() {
        let records = vec![EntityRecord {
            id: 1, parent_id: None, macro_name: "x".into(), code: None,
            kind: LiveObjectKind::Station, owner: None,
            position: glam::Vec3::ZERO, sector_macro: "anything".into(),
        }];
        let mut fs = HashMap::new();
        let mut next = 1u32;
        let world = merge(vec![records], None, &mut fs, &mut next);
        assert!(world.names.is_empty());
    }
```

- [ ] **Step 3: Update `mod.rs` to accept + pass through faction maps**

In `crates/map-io/src/save_parser/mod.rs`, change `parse_save` signature:

```rust
pub fn parse_save(
    path: &Path,
    sector_macros: Option<&HashMap<String, SectorId>>,
    faction_strings: &mut HashMap<String, FactionId>,
    next_faction_id: &mut u32,
) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError> {
```

(Add `use map_domain::ids::FactionId;` at the top.)

Replace the Stage 4 call site:

```rust
    let world = merge::merge(entity_lists, sector_macros, faction_strings, next_faction_id);
```

- [ ] **Step 4: Update the two integration tests in `mod.rs`**

```rust
    #[test]
    fn parse_mini_save_meta_and_overrides() {
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut nx = 1u32;
        let (meta, _world, overrides) = parse_save(&fixture_path(), None, &mut fs, &mut nx).unwrap();
        assert_eq!(meta.player_money, 40000);
        assert!((meta.game_time_seconds - 1734.285).abs() < 1e-2);
        assert_eq!(overrides.len(), 2);
    }

    #[test]
    fn parse_mini_save_entities_resolved_via_sector_macros() {
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("cluster_01_sector001_macro".into(), SectorId(1));
        sm.insert("cluster_06_sector001_macro".into(), SectorId(2));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut nx = 1u32;
        let (_meta, world, _) = parse_save(&fixture_path(), Some(&sm), &mut fs, &mut nx).unwrap();
        assert_eq!(world.names.len(), 4);
        assert_eq!(world.entities_in_sector(SectorId(1)).len(), 2);
        assert_eq!(world.entities_in_sector(SectorId(2)).len(), 2);
    }
```

- [ ] **Step 5: Build + test**

```bash
cargo test -p map-io --lib save_parser 2>&1 | tail -10
```
Expected: all save_parser tests pass (the merge unit tests + the orchestrator integration tests).

```bash
cargo build 2>&1 | grep "^error" | head -5
```
Expected: errors only in `map-app/src/main.rs` (caller of `parse_save`) — fixed next task.

- [ ] **Step 6: Commit**

```bash
git add crates/map-io/src/save_parser/merge.rs \
        crates/map-io/src/save_parser/mod.rs
git commit -m "feat(io): merge takes shared faction map; populates parents/children/codes"
```

---

### Task 10: Wire faction tables into main.rs + app.rs

**Files:**
- Modify: `crates/map-app/src/main.rs`
- Modify: `crates/map-app/src/app.rs`

- [ ] **Step 1: Update `parse_latest_save` to thread shared faction maps**

In `crates/map-app/src/main.rs`, replace `parse_latest_save`:

```rust
pub fn parse_latest_save(
    sector_macros: &HashMap<String, SectorId>,
    faction_strings: &mut HashMap<String, FactionId>,
    next_faction_id: &mut u32,
) -> Option<SnapshotMessage> {
    let (path, _dir) = find_latest_save()?;
    eprintln!("[map] Loading save: {:?}", path);
    match map_io::save_parser::parse_save(&path, Some(sector_macros), faction_strings, next_faction_id) {
        Ok((meta, world, faction_overrides)) => {
            eprintln!(
                "[map] Snapshot: time={:.1}s money={} location={}",
                meta.game_time_seconds, meta.player_money, meta.player_location_name
            );
            eprintln!(
                "[map] Faction overrides: {} sectors",
                faction_overrides.len()
            );
            Some(SnapshotMessage::Loaded { meta, world, faction_overrides })
        }
        Err(e) => {
            eprintln!("[map] save_parser error: {:?}", e);
            Some(SnapshotMessage::Error(format!("{:?}", e)))
        }
    }
}
```

- [ ] **Step 2: Update `spawn_save_parse`**

```rust
pub fn spawn_save_parse(
    tx: mpsc::Sender<SnapshotMessage>,
    sector_macros: HashMap<String, SectorId>,
    mut faction_strings: HashMap<String, FactionId>,
    mut next_faction_id: u32,
) {
    let _ = tx.send(SnapshotMessage::Loading);
    std::thread::spawn(move || {
        let msg = parse_latest_save(&sector_macros, &mut faction_strings, &mut next_faction_id)
            .unwrap_or(SnapshotMessage::None);
        let _ = tx.send(msg);
    });
}
```

(Note: clones of `faction_strings` happen at spawn site — the merged result is not propagated back through the channel. Future improvement: also send the updated faction map. For v1, the static load already populates known factions; save's unknowns get IDs but only inside the snapshot's World, where the panel resolves via `Universe.faction_table` (missing IDs render as raw owner string via fallback in helper). This is acceptable v1.)

- [ ] **Step 3: Update `main()` call sites**

Replace the initial-load block and the watcher spawn:

```rust
    let faction_strings = universe.faction_strings.clone();
    let next_faction_id: u32 = (universe.faction_strings.len() as u32) + 1;
    let initial_tx = snapshot_tx.clone();
    let sector_macros = universe.sector_macros.clone();
    let fs_init = faction_strings.clone();
    let nx_init = next_faction_id;
    std::thread::spawn(move || {
        let mut fs = fs_init;
        let mut nx = nx_init;
        let msg = parse_latest_save(&sector_macros, &mut fs, &mut nx).unwrap_or(SnapshotMessage::None);
        let _ = initial_tx.send(msg);
    });
```

In the watcher closure (where `spawn_save_parse` is called), pass the additional args:

```rust
    spawn_save_parse(
        parse_tx.clone(),
        sector_macros.clone(),
        faction_strings.clone(),
        next_faction_id,
    );
```

- [ ] **Step 4: Update app.rs refresh call**

In `crates/map-app/src/app.rs`, find `crate::spawn_save_parse(self.snapshot_tx.clone(), self.universe.sector_macros.clone())` and add the two new args:

```rust
                crate::spawn_save_parse(
                    self.snapshot_tx.clone(),
                    self.universe.sector_macros.clone(),
                    self.universe.faction_strings.clone(),
                    (self.universe.faction_strings.len() as u32) + 1,
                );
```

- [ ] **Step 5: Update `apply_faction_overrides` to reuse Universe's faction_strings**

Replace in `main.rs`:

```rust
pub fn apply_faction_overrides(universe: &mut Universe, overrides: &FactionOverrides) {
    let mut next_faction_id: u32 = (universe.faction_strings.len() as u32) + 1;
    let mut applied: usize = 0;

    for (macro_key, faction_name) in overrides.iter() {
        let Some(&sector_id) = universe.sector_macros.get(macro_key) else {
            continue;
        };
        let key = faction_name.to_lowercase();
        let fid = *universe.faction_strings.entry(key).or_insert_with(|| {
            let id = FactionId(next_faction_id);
            next_faction_id += 1;
            id
        });
        if let Some(sec) = universe.sectors.iter_mut().find(|s| s.id == sector_id) {
            sec.faction = Some(fid);
            applied += 1;
        }
    }
    eprintln!("[map] Applied {} sector faction overrides", applied);
}
```

- [ ] **Step 6: Build + smoke-run**

```bash
cargo build 2>&1 | grep "^error" | head -5
```
Expected: clean.

```bash
timeout 15 cargo run --release 2>&1 | grep -E "^\[(map|parse)\]" | head -20
```
Expected: `[map] Faction defs:`, `[map] Built faction table:`, `[map] Snapshot:`, `[parse] entities=`. The entity count should now be 12 600+ (was 12 242) thanks to nested capture in Task 8.

- [ ] **Step 7: Commit**

```bash
git add crates/map-app/src/main.rs crates/map-app/src/app.rs
git commit -m "feat(app): thread shared faction maps through parse_save call sites"
```

---

### Task 11: ViewMode adds `selected_entity`

**Files:**
- Modify: `crates/map-domain/src/view.rs`

- [ ] **Step 1: Update the enum + transitions**

Open `crates/map-domain/src/view.rs`. Update the `SectorView` variant of `ViewMode`:

```rust
    SectorView {
        sector: SectorId,
        selected_obj: Option<ObjectId>,
        selected_entity: Option<EntityId>,
    },
```

(Import `EntityId` at top: `use crate::world::EntityId;`.)

Update existing methods:

```rust
    pub fn select_object(self, obj: ObjectId) -> Self {
        match self {
            ViewMode::SectorView { sector, .. } => ViewMode::SectorView {
                sector, selected_obj: Some(obj), selected_entity: None,
            },
            other => other,
        }
    }

    pub fn select_entity(self, eid: EntityId) -> Self {
        match self {
            ViewMode::SectorView { sector, .. } => ViewMode::SectorView {
                sector, selected_obj: None, selected_entity: Some(eid),
            },
            other => other,
        }
    }

    pub fn deselect_object(self) -> Self {
        match self {
            ViewMode::SectorView { sector, selected_entity, .. } => ViewMode::SectorView {
                sector, selected_obj: None, selected_entity,
            },
            other => other,
        }
    }

    pub fn deselect_entity(self) -> Self {
        match self {
            ViewMode::SectorView { sector, selected_obj, .. } => ViewMode::SectorView {
                sector, selected_obj, selected_entity: None,
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
```

Update the `open_sector_3d` and any other constructor of `SectorView { ... }` to include `selected_entity: None`.

- [ ] **Step 2: Update existing view tests**

`view.rs` has unit tests for state transitions. Update each `SectorView { sector, selected_obj: ... }` literal to include `selected_entity: None`. Add a new test:

```rust
    #[test]
    fn select_entity_clears_selected_obj() {
        let v = ViewMode::SectorView {
            sector: SectorId(1),
            selected_obj: Some(ObjectId(99)),
            selected_entity: None,
        };
        let v = v.select_entity(42);
        match v {
            ViewMode::SectorView { selected_obj, selected_entity, .. } => {
                assert_eq!(selected_obj, None);
                assert_eq!(selected_entity, Some(42));
            }
            _ => panic!(),
        }
    }
```

- [ ] **Step 3: Build + test**

```bash
cargo test -p map-domain --lib view 2>&1 | tail -8
```
Expected: all view tests pass. Other map-domain tests still pass.

(Workspace build will fail in `map-app` where `SectorView { sector, selected_obj }` literal is constructed — fix in Task 12.)

- [ ] **Step 4: Commit**

```bash
git add crates/map-domain/src/view.rs
git commit -m "feat(domain): ViewMode::SectorView adds selected_entity"
```

---

### Task 12: GPU uniform buffer cap 128 → 2048

**Files:**
- Modify: `crates/map-app/src/renderer/gpu.rs`

- [ ] **Step 1: Bump constant**

In `crates/map-app/src/renderer/gpu.rs`, change:

```rust
const MAX_OBJECTS: u64 = 128;
```

to:

```rust
const MAX_OBJECTS: u64 = 2048;
```

- [ ] **Step 2: Add overflow warning log**

Find the line that does `self.draw_calls.len().min(MAX_OBJECTS as usize)`. Just above the `.min()` call, add:

```rust
        if self.draw_calls.len() > MAX_OBJECTS as usize {
            eprintln!(
                "[render] WARNING: scene has {} draw calls but GPU cap is {}; truncating",
                self.draw_calls.len(), MAX_OBJECTS
            );
        }
```

(Log fires per-frame which is too chatty; wrap with a `std::sync::OnceLock<()>` so it logs once per app lifetime — or just leave per-frame and accept the noise as a signal. For v1, leave per-frame; user can tighten later.)

- [ ] **Step 3: Build + run**

```bash
cargo build --release 2>&1 | grep "^error" | head -5
```
Expected: no errors. WebGPU minimum guarantee for uniform buffer binding size is 64 KB; 2048 × 256 = 512 KB. Modern desktop GPUs (incl. Intel/AMD/Nvidia/Apple Metal) support 1 MB+ — safe in practice. Document the risk in a code comment:

```rust
// 2048 × 256-byte stride = 512 KB uniform buffer. WebGPU baseline guarantees
// 64 KB; modern desktops give 1 MB+. If we ship to lower-spec hardware, chunk
// the draw passes instead of one giant buffer.
const MAX_OBJECTS: u64 = 2048;
```

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/src/renderer/gpu.rs
git commit -m "feat(render): bump GPU uniform buffer cap 128 → 2048 for live entities"
```

---

### Task 13: `colors.rs` helper module

**Files:**
- Create: `crates/map-app/src/colors.rs`
- Modify: `crates/map-app/src/main.rs` (add `mod colors;`)

- [ ] **Step 1: Create helper module**

```rust
//! Centralised faction colour + name resolution. Reads from `Universe.faction_table`.

use map_domain::ids::FactionId;
use map_domain::universe::Universe;

pub fn faction_color(universe: &Universe, id: FactionId) -> egui::Color32 {
    universe
        .faction_table
        .get(&id)
        .map(|m| {
            egui::Color32::from_rgba_unmultiplied(m.color[0], m.color[1], m.color[2], m.color[3])
        })
        .unwrap_or(crate::theme::TEXT_MUTED)
}

pub fn faction_name<'a>(universe: &'a Universe, id: FactionId) -> &'a str {
    universe
        .faction_table
        .get(&id)
        .map(|m| m.display_name.as_str())
        .unwrap_or("Unknown")
}
```

- [ ] **Step 2: Add `mod colors;` to `main.rs`**

After the existing `mod app;`, `mod renderer;`, etc., add:

```rust
mod colors;
```

- [ ] **Step 3: Build**

```bash
cargo build 2>&1 | grep "^error" | head -5
```
Expected: clean (or only the pre-existing Task-11 fallout in `app.rs` that we fix in Task 14).

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/src/colors.rs crates/map-app/src/main.rs
git commit -m "feat(app): colors module — faction_color + faction_name helpers"
```

---

### Task 14: Replace PALETTE in map_view + sector_view with `colors::faction_color`

**Files:**
- Modify: `crates/map-app/src/ui/map_view.rs`
- Modify: `crates/map-app/src/ui/sector_view.rs`
- Modify: `crates/map-app/src/app.rs` (fix `selected_entity` field constructor fallout from Task 11)

- [ ] **Step 1: app.rs — fix ViewMode construction**

Search:
```bash
grep -n "ViewMode::SectorView \|SectorView {" crates/map-app/src/app.rs
```
Every literal must include `selected_entity: None`. (The `view_mode.clone().open_sector_3d()` chain through `select_sector` etc. goes via methods updated in Task 11; only direct struct literals need fixing — there may not be any in `app.rs` if it always goes through methods.)

Also pass world reference to sector_view + sector_panel where needed; this is small wiring that doesn't change signatures yet (Task 16 changes panel sig).

- [ ] **Step 2: map_view — drop PALETTE, route through `crate::colors::faction_color`**

In `crates/map-app/src/ui/map_view.rs`, locate the local `PALETTE` constant + the function that uses it (probably `sector_color` or `faction_color`). Replace the function:

```rust
fn sector_color(universe: &Universe, faction: Option<FactionId>) -> egui::Color32 {
    match faction {
        Some(id) => crate::colors::faction_color(universe, id),
        None => crate::theme::TEXT_MUTED,
    }
}
```

…and delete the `PALETTE` constant. Adjust the call sites in `show` to pass `universe` along.

- [ ] **Step 3: sector_view — drop local `faction_color` + PALETTE**

In `crates/map-app/src/ui/sector_view.rs`, delete the `fn faction_color` defined locally (around line 475) and its `PALETTE`. Any call site (in `draw_live_ships`) that calls `faction_color(id)` becomes `crate::colors::faction_color(universe, id)` — we'll need to thread `universe` through, but `draw_live_ships` is going to be deleted in Task 15 anyway. For now, comment out the line and let Task 15 remove it entirely:

```rust
            // let color = world.factions.get(&eid).copied().map(faction_color)…
            // Replaced by GPU draw path in Task 15.
```

(If the compiler errors, just remove `draw_live_ships` body wholesale and call it a no-op until Task 15 deletes the call site.)

- [ ] **Step 4: Build**

```bash
cargo build 2>&1 | grep "^error" | head -5
```
Expected: clean.

- [ ] **Step 5: Smoke-run + visual check**

```bash
timeout 20 cargo run --release 2>&1 | tail -5
```
Eyeball the map: sectors should now be tinted with the **game's** faction colours (e.g. Argon = azure, Teladi = yellow), not the previous 8-colour palette. If colours look identical to before, debug:
- Check `[map] Built faction table: N factions` from Task 4 log; if N is 0, the faction parser isn't running.
- Check `Universe.faction_strings` includes "argon", "teladi", etc. (add a temporary `eprintln!` in main.rs after `load_universe()` if needed).

- [ ] **Step 6: Commit**

```bash
git add crates/map-app/src/ui/map_view.rs \
        crates/map-app/src/ui/sector_view.rs \
        crates/map-app/src/app.rs
git commit -m "feat(ui): replace hardcoded faction palette with game colours via colors helper"
```

---

### Task 15: GPU-render live entities; delete 2D `draw_live_ships`; add live picking

**Files:**
- Modify: `crates/map-app/src/ui/sector_view.rs`
- Modify: `crates/map-app/src/app.rs` (pass universe to sector_view.show)

- [ ] **Step 1: Update `SectorView3D::show` signature**

Replace the show function header:

```rust
pub fn show(
    &mut self,
    ui: &mut egui::Ui,
    sector: Option<&Sector>,
    camera: &mut OrbitCamera,
    selected_obj: Option<ObjectId>,
    selected_entity: Option<map_domain::world::EntityId>,
    world: Option<&map_domain::world::World>,
    universe: &map_domain::universe::Universe,
) -> SectorViewResponse {
```

`SectorViewResponse`:

```rust
pub enum ClickedTarget {
    Static(ObjectId),
    Entity(map_domain::world::EntityId),
}

pub struct SectorViewResponse {
    pub close_clicked: bool,
    pub clicked: Option<ClickedTarget>,
    pub hovered: Option<ClickedTarget>,
}
```

- [ ] **Step 2: Extend `build_draw_calls` with live entities**

Replace `build_draw_calls` with:

```rust
fn build_draw_calls(
    sector: &Sector,
    world: Option<&map_domain::world::World>,
    universe: &map_domain::universe::Universe,
    selected_obj: Option<ObjectId>,
    selected_entity: Option<map_domain::world::EntityId>,
) -> Vec<DrawCall> {
    let mut calls: Vec<DrawCall> = Vec::new();

    // Static objects (existing path; excludes gates + highways which render in 2D).
    for obj in &sector.static_objects {
        if matches!(obj.kind, StaticObjectKind::Gate | StaticObjectKind::Highway) {
            continue;
        }
        let scale = match obj.kind {
            StaticObjectKind::Station => 3.0,
            StaticObjectKind::Gate => 4.0,
            StaticObjectKind::ResourceZone => 8.0,
            StaticObjectKind::Anomaly => 2.0,
            StaticObjectKind::Highway => 4.0,
        };
        let mesh = match obj.kind {
            StaticObjectKind::Station => MeshKind::Box,
            StaticObjectKind::Gate => MeshKind::Ring,
            StaticObjectKind::ResourceZone => MeshKind::Sphere,
            StaticObjectKind::Anomaly => MeshKind::Sphere,
            StaticObjectKind::Highway => MeshKind::Ring,
        };
        let color = if selected_obj == Some(obj.id) {
            [1.0, 0.8, 0.1, 1.0]
        } else {
            kind_color(&obj.kind)
        };
        let rotation = obj
            .rotation
            .map(|(p, y, r)| {
                Mat4::from_euler(glam::EulerRot::YXZ, y.to_radians(), p.to_radians(), r.to_radians())
            })
            .unwrap_or(Mat4::IDENTITY);
        let model = Mat4::from_translation(obj.position) * rotation * Mat4::from_scale(Vec3::splat(scale));
        calls.push(DrawCall { kind: mesh, mvp: model, color });
    }

    // Live entities: top-level only (parent.is_none()).
    if let Some(world) = world {
        use map_domain::world::LiveObjectKind;
        for &eid in world.entities_in_sector(sector.id) {
            if world.parent_of(eid).is_some() {
                continue; // docked / subordinate — invisible in scene, listed in panel
            }
            let Some(&pos) = world.positions.get(&eid) else { continue };
            let kind = world.kinds.get(&eid);
            let (mesh, scale) = match kind {
                Some(LiveObjectKind::Station)        => (MeshKind::Box,    4.0),
                Some(LiveObjectKind::ShipExtraLarge) => (MeshKind::Box,    2.5),
                Some(LiveObjectKind::ShipLarge)      => (MeshKind::Box,    1.5),
                Some(LiveObjectKind::ShipMedium)     => (MeshKind::Sphere, 1.0),
                Some(LiveObjectKind::ShipSmall)      => (MeshKind::Sphere, 0.5),
                None => continue,
            };
            let fcolor = world.factions.get(&eid).copied()
                .map(|fid| crate::colors::faction_color(universe, fid))
                .unwrap_or(crate::theme::TEXT_MUTED);
            let base = [
                fcolor.r() as f32 / 255.0,
                fcolor.g() as f32 / 255.0,
                fcolor.b() as f32 / 255.0,
                1.0,
            ];
            let color = if selected_entity == Some(eid) {
                [1.0, 0.8, 0.1, 1.0]
            } else {
                base
            };
            let model = Mat4::from_translation(pos) * Mat4::from_scale(Vec3::splat(scale));
            calls.push(DrawCall { kind: mesh, mvp: model, color });
        }
    }
    calls
}
```

- [ ] **Step 3: Update `pick_object` to `pick_target` covering both kinds**

Replace `pick_object` signature + body:

```rust
fn pick_target(
    ptr: Pos2,
    rect: Rect,
    camera: &OrbitCamera,
    sector: &Sector,
    world: Option<&map_domain::world::World>,
) -> Option<ClickedTarget> {
    let aspect = rect.width() / rect.height().max(1.0);
    let vp = camera.proj_matrix(aspect) * camera.view_matrix();
    let mut best: Option<(f32, ClickedTarget)> = None;

    let project = |w_pos: Vec3| -> Option<Pos2> {
        let clip = vp * w_pos.extend(1.0);
        if clip.w <= 0.0 { return None; }
        let ndc = clip.truncate() / clip.w;
        Some(Pos2::new(
            (ndc.x * 0.5 + 0.5) * rect.width() + rect.left(),
            (1.0 - (ndc.y * 0.5 + 0.5)) * rect.height() + rect.top(),
        ))
    };

    let mut consider = |sp: Pos2, target: ClickedTarget| {
        let d = ((sp.x - ptr.x).powi(2) + (sp.y - ptr.y).powi(2)).sqrt();
        if d < 20.0 && best.as_ref().map_or(true, |(b, _)| d < *b) {
            best = Some((d, target));
        }
    };

    for obj in &sector.static_objects {
        if let Some(sp) = project(obj.position) {
            consider(sp, ClickedTarget::Static(obj.id));
        }
    }
    if let Some(world) = world {
        for &eid in world.entities_in_sector(sector.id) {
            if world.parent_of(eid).is_some() { continue; }
            let Some(&pos) = world.positions.get(&eid) else { continue };
            if let Some(sp) = project(pos) {
                consider(sp, ClickedTarget::Entity(eid));
            }
        }
    }
    best.map(|(_, t)| t)
}
```

- [ ] **Step 4: Update `show` to use the new pick + populate `clicked` + `hovered`**

In `SectorView3D::show`, replace the click handler:

```rust
        if canvas_resp.clicked() {
            if let (Some(pos), Some(sec)) = (canvas_resp.interact_pointer_pos(), sector) {
                clicked = pick_target(pos, view_rect, camera, sec, world);
            }
        }
        let hovered = canvas_resp.hover_pos().and_then(|pos| {
            sector.and_then(|sec| pick_target(pos, view_rect, camera, sec, world))
        });
```

…and assemble the response:

```rust
        SectorViewResponse { close_clicked, clicked, hovered }
```

- [ ] **Step 5: Delete `draw_live_ships`**

Remove the entire `fn draw_live_ships(...)` function and its call site near `// Live entities from save snapshot…`. The GPU pass now renders them.

- [ ] **Step 6: Pass new args through from app.rs**

In `crates/map-app/src/app.rs`, where `self.sector_view.show(...)` is called, update args:

```rust
                    let sv_resp = self.sector_view.show(
                        ui,
                        sec,
                        &mut self.camera,
                        self.view_mode.selected_object(),
                        self.view_mode.selected_entity(),
                        self.snapshot.as_ref().map(|(_, w)| w),
                        &self.universe,
                    );
                    if sv_resp.close_clicked {
                        self.view_mode = self.view_mode.clone().close_sector_3d();
                    }
                    match sv_resp.clicked {
                        Some(crate::ui::sector_view::ClickedTarget::Static(obj_id)) => {
                            self.view_mode = self.view_mode.clone().select_object(obj_id);
                            // existing fit_all on static obj
                        }
                        Some(crate::ui::sector_view::ClickedTarget::Entity(eid)) => {
                            self.view_mode = self.view_mode.clone().select_entity(eid);
                            if let Some((_, world)) = &self.snapshot {
                                if let Some(&pos) = world.positions.get(&eid) {
                                    self.camera.fit_all(&[pos]);
                                }
                            }
                        }
                        None => {}
                    }
```

(`selected_object` is the existing accessor; add it to `ViewMode` if missing — mirror `selected_entity`.)

- [ ] **Step 7: Build + run + manual smoke**

```bash
cargo build 2>&1 | grep "^error" | head -5
```
Expected: clean.

```bash
cargo run --release
```
Open a populated sector (e.g. Argon Prime). Expect a forest of coloured boxes/spheres — many stations as box clusters, ships as smaller spheres. Click one — yellow tint. Per-sector counts in 2D map were already in place.

- [ ] **Step 8: Commit**

```bash
git add crates/map-app/src/ui/sector_view.rs crates/map-app/src/app.rs
git commit -m "feat(3d): render top-level live entities via GPU pipeline; live picking + hover"
```

---

### Task 16: Sector panel — categories + live entities + DOCKED + back-to-parent

**Files:**
- Modify: `crates/map-app/src/ui/sector_panel.rs`
- Modify: `crates/map-app/src/app.rs`

- [ ] **Step 1: Update SectorPanelResponse**

Replace in `sector_panel.rs`:

```rust
pub struct SectorPanelResponse {
    pub open_3d_clicked: bool,
    pub back_to_map_clicked: bool,
    pub object_clicked: Option<ObjectId>,
    pub entity_clicked: Option<map_domain::world::EntityId>,
    pub back_to_parent_clicked: bool,
}
```

- [ ] **Step 2: Update `SectorPanel::show` signature**

```rust
pub fn show(
    &mut self,
    ui: &mut egui::Ui,
    sector: Option<&Sector>,
    universe: &Universe,
    view_mode: &ViewMode,
    world: Option<&map_domain::world::World>,
) -> SectorPanelResponse {
```

- [ ] **Step 3: Add helper to compute entity display**

In `sector_panel.rs`, add module-level helper:

```rust
fn entity_row_label(
    world: &map_domain::world::World,
    eid: map_domain::world::EntityId,
) -> (String, &'static str) {
    use map_domain::world::LiveObjectKind;
    let icon = match world.kinds.get(&eid) {
        Some(LiveObjectKind::Station) => "◼",
        Some(LiveObjectKind::ShipExtraLarge) | Some(LiveObjectKind::ShipLarge) => "▲",
        _ => "▴",
    };
    let code = world.codes.get(&eid).cloned();
    let macro_name = world.names.get(&eid).cloned().unwrap_or_default();
    let human = strip_macro(&macro_name);

    let label = match (code, &human) {
        (Some(c), h) if !h.is_empty() && h != c => format!("{} — {}", c, h),
        (Some(c), _) => c,
        (None, h) if !h.is_empty() => h.clone(),
        _ => macro_name,
    };
    (label, icon)
}

fn strip_macro(s: &str) -> String {
    let s = s.to_lowercase();
    let s = s.strip_suffix("_macro").unwrap_or(&s);
    s.replace('_', " ")
}
```

- [ ] **Step 4: Rewrite the SectorView branch with categories**

Replace the inside of the `if let ViewMode::SectorView { selected_obj, .. } = view_mode { ... }` block with:

```rust
                if let ViewMode::SectorView { selected_obj, selected_entity, .. } = view_mode {
                    // ─── Static objects ──────────────────────────────────
                    egui::CollapsingHeader::new(format!(
                        "STATIC OBJECTS ({})",
                        sector.static_objects.len()
                    ))
                    .default_open(true)
                    .show(ui, |ui| {
                        for obj in &sector.static_objects {
                            let is_sel = *selected_obj == Some(obj.id);
                            let label = format!("{} {}", kind_icon(&obj.kind), &obj.name);
                            let color = if is_sel { theme::ACCENT } else { theme::TEXT_PRIMARY };
                            if ui.colored_label(color, &label).clicked() {
                                object_clicked = Some(obj.id);
                            }
                        }
                    });

                    // ─── Live entities, grouped ──────────────────────────
                    if let Some(world) = world {
                        use map_domain::world::LiveObjectKind;
                        let mut by_group: std::collections::HashMap<&'static str, Vec<map_domain::world::EntityId>> = std::collections::HashMap::new();
                        for &eid in world.entities_in_sector(sector.id) {
                            if world.parent_of(eid).is_some() { continue; }
                            let bucket = match world.kinds.get(&eid) {
                                Some(LiveObjectKind::Station) => "STATIONS",
                                Some(LiveObjectKind::ShipExtraLarge) | Some(LiveObjectKind::ShipLarge) => "CAPITALS",
                                Some(LiveObjectKind::ShipMedium) => "MEDIUM",
                                Some(LiveObjectKind::ShipSmall) => "SMALL",
                                None => continue,
                            };
                            by_group.entry(bucket).or_default().push(eid);
                        }
                        for &group in &["STATIONS", "CAPITALS", "MEDIUM", "SMALL"] {
                            if let Some(eids) = by_group.get(group) {
                                egui::CollapsingHeader::new(format!("{} ({})", group, eids.len()))
                                    .default_open(group == "STATIONS")
                                    .show(ui, |ui| {
                                        for &eid in eids {
                                            let is_sel = *selected_entity == Some(eid);
                                            let (label, icon) = entity_row_label(world, eid);
                                            let color = if is_sel { theme::ACCENT } else { theme::TEXT_PRIMARY };
                                            let row = format!("{} {}", icon, label);
                                            if ui.colored_label(color, &row).clicked() {
                                                entity_clicked = Some(eid);
                                            }
                                            // Faction column (small muted line under the row).
                                            if let Some(&fid) = world.factions.get(&eid) {
                                                let f_name = crate::colors::faction_name(universe, fid);
                                                let f_color = crate::colors::faction_color(universe, fid);
                                                ui.horizontal(|ui| {
                                                    ui.add_space(20.0);
                                                    ui.colored_label(f_color, "●");
                                                    ui.colored_label(theme::TEXT_MUTED, f_name);
                                                });
                                            }
                                        }
                                    });
                            }
                        }
                    }

                    // ─── SELECTED detail ─────────────────────────────────
                    if let Some(eid) = *selected_entity {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.colored_label(theme::TEXT_MUTED, "SELECTED");
                        if let Some(parent) = world.and_then(|w| w.parent_of(eid)) {
                            let (parent_label, _) = world.map(|w| entity_row_label(w, parent)).unwrap_or_default();
                            if ui.button(format!("← Back to {}", parent_label)).clicked() {
                                back_to_parent_clicked = true;
                            }
                        }
                        if let Some(world) = world {
                            let (label, icon) = entity_row_label(world, eid);
                            ui.colored_label(theme::ACCENT, format!("{} {}", icon, label));
                            if let Some(kind) = world.kinds.get(&eid) {
                                ui.colored_label(theme::TEXT_MUTED, format!("Type: {:?}", kind));
                            }
                            if let Some(&fid) = world.factions.get(&eid) {
                                let f_color = crate::colors::faction_color(universe, fid);
                                let f_name = crate::colors::faction_name(universe, fid);
                                ui.horizontal(|ui| {
                                    ui.colored_label(f_color, "●");
                                    ui.colored_label(theme::TEXT_MUTED, f_name);
                                });
                            }
                            if let Some(&pos) = world.positions.get(&eid) {
                                ui.colored_label(
                                    theme::TEXT_MUTED,
                                    format!("Pos: x {:.1} y {:.1} z {:.1} km", pos.x, pos.y, pos.z),
                                );
                            }
                            let kids = world.children_of(eid);
                            if !kids.is_empty() {
                                egui::CollapsingHeader::new(format!("DOCKED ({})", kids.len()))
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        for &cid in kids {
                                            let (clabel, cicon) = entity_row_label(world, cid);
                                            if ui.colored_label(theme::TEXT_PRIMARY, format!("{} {}", cicon, clabel)).clicked() {
                                                entity_clicked = Some(cid);
                                            }
                                        }
                                    });
                            }
                        }
                    } else if let Some(obj) = selected_obj
                        .and_then(|id| sector.static_objects.iter().find(|o| o.id == id))
                    {
                        // Existing static-object detail block (copy from current code).
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.colored_label(theme::TEXT_MUTED, "SELECTED");
                        ui.add_space(2.0);
                        ui.colored_label(theme::ACCENT, &obj.name);
                        ui.colored_label(theme::TEXT_MUTED, format!("Type: {}", kind_label(&obj.kind)));
                        ui.colored_label(theme::TEXT_MUTED, format!("x {:.1}  y {:.1}  z {:.1} km", obj.position.x, obj.position.y, obj.position.z));
                        if let Some(f) = obj.faction {
                            let f_name = crate::colors::faction_name(universe, f);
                            ui.colored_label(theme::TEXT_MUTED, format!("Faction: {}", f_name));
                        }
                        if let Some((p, y, r)) = obj.rotation {
                            ui.colored_label(theme::TEXT_MUTED, format!("pitch {:.1}° yaw {:.1}° roll {:.1}°", p, y, r));
                        }
                        for (k, v) in &obj.details {
                            ui.colored_label(theme::TEXT_MUTED, format!("{}: {}", k, v));
                        }
                    }
                } else {
                    // Universe view: keep CONNECTIONS list as-is.
                    ui.colored_label(theme::TEXT_MUTED, "CONNECTIONS");
                    // ... (keep existing code) ...
                }
```

Also declare two local mutable vars at the top of `show`:

```rust
        let mut entity_clicked: Option<map_domain::world::EntityId> = None;
        let mut back_to_parent_clicked = false;
```

Return them in `SectorPanelResponse`:

```rust
        SectorPanelResponse {
            open_3d_clicked: open_clicked,
            back_to_map_clicked: back_clicked,
            object_clicked,
            entity_clicked,
            back_to_parent_clicked,
        }
```

- [ ] **Step 5: Wire panel response in app.rs**

In `app.rs`, where `panel_resp = self.sector_panel.show(...)` is called, update args + handlers:

```rust
                let panel_resp = self.sector_panel.show(
                    ui,
                    sector,
                    &self.universe,
                    &self.view_mode,
                    self.snapshot.as_ref().map(|(_, w)| w),
                );
                if panel_resp.open_3d_clicked { /* existing */ }
                if panel_resp.back_to_map_clicked { /* existing */ }
                if let Some(obj_id) = panel_resp.object_clicked { /* existing */ }
                if let Some(eid) = panel_resp.entity_clicked {
                    self.view_mode = self.view_mode.clone().select_entity(eid);
                    if let Some((_, world)) = &self.snapshot {
                        if let Some(&pos) = world.positions.get(&eid) {
                            self.camera.fit_all(&[pos]);
                        }
                    }
                }
                if panel_resp.back_to_parent_clicked {
                    if let (Some(eid), Some((_, world))) = (
                        self.view_mode.selected_entity(),
                        self.snapshot.as_ref(),
                    ) {
                        if let Some(parent) = world.parent_of(eid) {
                            self.view_mode = self.view_mode.clone().select_entity(parent);
                            if let Some(&pos) = world.positions.get(&parent) {
                                self.camera.fit_all(&[pos]);
                            }
                        }
                    }
                }
```

- [ ] **Step 6: Build + smoke**

```bash
cargo build 2>&1 | grep "^error" | head -5
```
Expected: clean.

```bash
cargo run --release
```
Enter a populated sector. Side panel should show categories. Click a station → see DOCKED list of its drones. Click a drone → SELECTED switches; `← Back to <station>` button visible.

- [ ] **Step 7: Commit**

```bash
git add crates/map-app/src/ui/sector_panel.rs crates/map-app/src/app.rs
git commit -m "feat(panel): categories + live entities + DOCKED list + back-to-parent"
```

---

### Task 17: Hover label in 3D scene

**Files:**
- Modify: `crates/map-app/src/ui/sector_view.rs`

- [ ] **Step 1: Add `draw_hover_label`**

Append after `draw_axis_arrows`:

```rust
fn draw_hover_label(
    painter: &egui::Painter,
    view_rect: Rect,
    camera: &OrbitCamera,
    sector: &Sector,
    world: Option<&map_domain::world::World>,
    universe: &map_domain::universe::Universe,
    target: ClickedTarget,
) {
    let aspect = view_rect.width() / view_rect.height().max(1.0);
    let vp = camera.proj_matrix(aspect) * camera.view_matrix();

    let project = |w_pos: Vec3| -> Option<Pos2> {
        let clip = vp * w_pos.extend(1.0);
        if clip.w <= 0.0 { return None; }
        let ndc = clip.truncate() / clip.w;
        Some(Pos2::new(
            (ndc.x * 0.5 + 0.5) * view_rect.width() + view_rect.left(),
            (1.0 - (ndc.y * 0.5 + 0.5)) * view_rect.height() + view_rect.top(),
        ))
    };

    // Lines to draw: (text, color)
    let mut lines: Vec<(String, egui::Color32)> = Vec::new();
    let anchor_pos: Option<Pos2> = match target {
        ClickedTarget::Static(obj_id) => {
            if let Some(obj) = sector.static_objects.iter().find(|o| o.id == obj_id) {
                lines.push((obj.name.clone(), crate::theme::TEXT_PRIMARY));
                lines.push((format!("Type: {:?}", obj.kind), crate::theme::TEXT_MUTED));
                project(obj.position)
            } else { None }
        }
        ClickedTarget::Entity(eid) => {
            if let Some(world) = world {
                let code = world.codes.get(&eid).cloned();
                let macro_name = world.names.get(&eid).cloned();
                let human = macro_name.as_deref().map(|m| {
                    let s = m.to_lowercase();
                    let s = s.strip_suffix("_macro").unwrap_or(&s);
                    s.replace('_', " ")
                });
                if let Some(c) = &code {
                    lines.push((c.clone(), crate::theme::ACCENT));
                }
                if let Some(h) = &human {
                    if !h.is_empty() && Some(h) != code.as_ref() {
                        lines.push((h.clone(), crate::theme::TEXT_PRIMARY));
                    }
                }
                if let Some(&fid) = world.factions.get(&eid) {
                    let f_name = crate::colors::faction_name(universe, fid);
                    let f_color = crate::colors::faction_color(universe, fid);
                    lines.push((f_name.to_string(), f_color));
                }
                world.positions.get(&eid).copied().and_then(project)
            } else { None }
        }
    };
    let Some(anchor) = anchor_pos else { return };

    let font = egui::FontId::proportional(11.0);
    let pad = 4.0;
    let line_h = 14.0;
    let max_w = lines.iter()
        .map(|(t, _)| painter.layout_no_wrap(t.clone(), font.clone(), egui::Color32::WHITE).rect.width())
        .fold(0.0_f32, f32::max);
    let h = pad * 2.0 + line_h * lines.len() as f32;
    let label_rect = egui::Rect::from_min_size(
        anchor + egui::Vec2::new(10.0, -h - 4.0),
        egui::Vec2::new(max_w + pad * 2.0, h),
    );
    painter.rect_filled(label_rect, 3.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200));
    for (i, (text, color)) in lines.iter().enumerate() {
        let p = label_rect.min + egui::Vec2::new(pad, pad + i as f32 * line_h);
        painter.text(p, egui::Align2::LEFT_TOP, text, font.clone(), *color);
    }
}
```

- [ ] **Step 2: Call from `show` (after axes, before border)**

```rust
        if let (Some(target), Some(sec)) = (hovered, sector) {
            draw_hover_label(ui.painter(), view_rect, camera, sec, world, universe, target);
        }
```

- [ ] **Step 3: Build + smoke**

```bash
cargo build 2>&1 | grep "^error" | head -5
cargo run --release
```

Hover over a station box → label appears with code + name + faction. Move cursor off → label disappears.

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/src/ui/sector_view.rs
git commit -m "feat(3d): hover label shows code + name + faction over any target"
```

---

### Task 18: Final smoke verification + CLAUDE.md update

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Run full test suite**

```bash
cargo test 2>&1 | tail -3
```
Expected: all pass (existing 53 + new tests added across tasks).

- [ ] **Step 2: Run app, capture parse log**

```bash
timeout 25 cargo run --release 2>&1 > /tmp/run.log
grep -E "^\[(map|parse)\]" /tmp/run.log | head -30
```

Verify:
- `[map] Faction defs: 30+` (game has ~30 factions across DLCs)
- `[map] Built faction table: N factions`
- No `[map] God stations loaded` line.
- `[parse] entities=12600+` (was 12 242 — increased due to nested capture).

- [ ] **Step 3: Update `CLAUDE.md`**

In the Data Loading section, remove the 50k+ god stations row and the 538-stations narrative line. Update the final tally line:

```
**Final loaded:** 144 sectors, 119 clusters, ~448 gates, ~67 god objects,
~103 superhighway endpoints, ~28 inter-sector superhighway pairs.
```

(Drop "~221 stations" from this sentence.)

In the Static Objects section, drop the `50k+ stations` row from the table.

In the Phase Status table, the row for Phase 3 stays ✅; mention the polish via a sentence after the table:

> 2026-05-18: Phase 3 polish — live ships + stations rendered via GPU, hierarchical side panel, in-game faction colours + names, dropped redundant god stations. See `docs/superpowers/specs/2026-05-18-station-fixes-design.md`.

- [ ] **Step 4: Manual visual checklist**

Walk through these in the running app and tick:

- [ ] 2D map sector hexes show game's faction colours (not the old 8-palette).
- [ ] Click into a populated sector (e.g. Argon Prime). 3D view shows many coloured boxes (stations) + spheres (ships).
- [ ] Side panel shows: STATIC OBJECTS / STATIONS / CAPITALS / MEDIUM / SMALL categories with non-zero counts on a heavily populated sector.
- [ ] Click a station in the panel — DOCKED list appears under SELECTED if it has subordinate ships.
- [ ] Click a docked drone — SELECTED switches, `← Back to <parent>` button shows.
- [ ] Click the back button — returns to parent.
- [ ] Hover any 3D shape — label appears with code (if present), human name, faction in faction colour.
- [ ] No GPU warnings logged under normal operation.

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: CLAUDE.md — drop god station counts; note 2026-05-18 phase 3 polish"
```

---

## Out-of-Scope (Phase 4)

- Absolute world-position composition for docked entities (currently parent-local offsets).
- `nameindex` → procedural-name resolution for live ships.
- Search index over live entities + faction filter.
- Camera lerp on selection (only `fit_all` snap today).
- 2D map ship-count badge tinted by sector's dominant faction (badge stays muted grey for now; tinting can land later).

---

## Self-Review

**Spec coverage:**
- Section 1 (faction pipeline) → Tasks 1–4.
- Section 2 (nested + parent + code) → Tasks 6, 7, 8, 9.
- Section 3 (drop god stations) → Task 5.
- Section 4 (panel categories + nav) → Task 16.
- Section 5 (GPU live render) → Tasks 12, 15.
- Section 6 (colors + human names) → Tasks 13, 14 + helper used in Tasks 15, 16, 17.
- Section 7 (hover label) → Task 17.
- Plan order matches spec's "Phase order" preamble.

**Placeholder scan:**
- No `TBD` / `TODO` / `implement later`. ✓
- Every code step shows the full code. ✓
- Every test step shows the assertion. ✓
- "Similar to Task N" — none. ✓

**Type consistency:**
- `FactionMeta { display_name, color }` — defined Task 1, used Tasks 4, 13, 16, 17.
- `EntityRecord { id, parent_id, macro_name, code, kind, owner, position, sector_macro }` — defined Task 6, used identically Tasks 8, 9.
- `World::insert_entity(id, name, kind, faction, position, sector, parent, code)` — sig set Task 7, called identically Task 9.
- `merge(batches, sector_macros, faction_strings, next_faction_id)` — sig set Task 9, called Task 9 (mod.rs) and tested Task 9.
- `parse_save(path, sector_macros, faction_strings, next_faction_id)` — sig set Task 9, called Task 10.
- `ClickedTarget { Static(ObjectId), Entity(EntityId) }` — defined Task 15, used Task 17.
- `SectorViewResponse { close_clicked, clicked, hovered }` — set Task 15, consumed Task 15 (app.rs).
- `SectorPanelResponse { open_3d_clicked, back_to_map_clicked, object_clicked, entity_clicked, back_to_parent_clicked }` — set Task 16, consumed Task 16 (app.rs).
- `ViewMode::SectorView { sector, selected_obj, selected_entity }` — set Task 11, used Tasks 15, 16.
