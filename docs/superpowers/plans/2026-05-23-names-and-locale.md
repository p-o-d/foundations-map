# Entity Names + Locale Switching Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `strip_macro` fallback with X4 display names resolved from each entity's per-instance `name=` / `basename=` save attribute, apply everywhere a ship or station is identified, and add a top-bar locale dropdown listing every language the install ships (English default, choice persists via eframe storage).

**Architecture:** Three layers, same shape as recent feature work. Domain adds `World.display_name_refs` plus `Universe.translations` / `available_locales` / `current_locale`. Parser captures the `name=`/`basename=` attributes and keeps the full translation table (no more page whitelist). UI gets a `colors::resolve_entity_label` helper used by the side panel and the 3D tooltip, plus a top-bar locale dropdown that triggers a full galaxy + save re-parse and persists the choice through eframe's `Storage`.

**Tech Stack:** Rust 2024, `quick_xml`, `egui` 0.34.2, `eframe` 0.34.2 (`Storage` + `get_value`/`set_value`), `serde` (already a transitive dep via `eframe::Storage`).

**Spec:** `docs/superpowers/specs/2026-05-23-names-and-locale-design.md`

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/map-domain/src/world.rs` | `World.display_name_refs: HashMap<EntityId, String>` |
| `crates/map-domain/src/universe.rs` | `translations`, `available_locales`, `current_locale` fields |
| `crates/map-io/src/save_parser/types.rs` | `EntityRecord.display_name_ref: Option<String>` |
| `crates/map-io/src/save_parser/sector_chunk.rs` | parse `name=`/`basename=` on station/ship components |
| `crates/map-io/src/save_parser/merge.rs` | copy `display_name_ref` into `World.display_name_refs` |
| `crates/map-io/src/xml_parser.rs` | drop page whitelist; accept `locale: u32`; populate Universe with translations + locales |
| `crates/map-io/src/game_path.rs` | `list_available_locales`, `locale_display_name` |
| `crates/map-app/src/colors.rs` | `replace_translation_refs`, `resolve_entity_label` |
| `crates/map-app/src/ui/sector_panel.rs` | swap `entity_row_label` to take `&Universe` and call new resolver |
| `crates/map-app/src/ui/sector_view.rs` | rewrite hover tooltip name/code/faction lines |
| `crates/map-app/src/settings.rs` (new) | `AppSettings { locale }` + eframe persistence helpers |
| `crates/map-app/src/main.rs` | expose settings to App, thread initial locale through to first `parse_galaxy_from_game` |
| `crates/map-app/src/app.rs` | hold `settings`, implement `reload_galaxy`, persist via `eframe::App::save` |
| `crates/map-app/src/ui/top_bar.rs` | locale ComboBox + `TopBarResponse.locale_changed_to` |

---

## Task 1: World.display_name_refs field

**Files:**
- Modify: `crates/map-domain/src/world.rs`

- [ ] **Step 1: Write the failing test** — append to the existing `tests` mod in `crates/map-domain/src/world.rs`:

```rust
    #[test]
    fn display_name_refs_round_trip() {
        let mut w = World::new();
        w.display_name_refs.insert(0x10, "{20101,122701}".into());
        w.display_name_refs.insert(0x11, "My Best Ship".into());
        assert_eq!(w.display_name_refs.get(&0x10).map(String::as_str), Some("{20101,122701}"));
        assert_eq!(w.display_name_refs.get(&0x11).map(String::as_str), Some("My Best Ship"));
        assert!(w.display_name_refs.get(&0x99).is_none());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p map-domain display_name_refs_round_trip`
Expected: FAIL — no field `display_name_refs` on `World`.

- [ ] **Step 3: Add the field**

Inside the existing `pub struct World { ... }` in `crates/map-domain/src/world.rs`, add:

```rust
    /// Raw `name=` / `basename=` value from the save. Either a `{page,id}` ref,
    /// a compound form `{p,t} ({p,t})`, or a literal string (player-renamed ships).
    /// Resolved at display time so it picks up the current locale.
    pub display_name_refs: HashMap<EntityId, String>,
```

`#[derive(Default)]` is already on `World`, so `HashMap::new()` is auto-supplied.

- [ ] **Step 4: Run all map-domain tests**

Run: `cargo test -p map-domain`
Expected: PASS, including the new test.

- [ ] **Step 5: Commit**

```bash
git add crates/map-domain/src/world.rs
git commit -m "$(cat <<'EOF'
feat(domain): World.display_name_refs for resolved entity labels

Per-entity raw name/basename ref from the save. Resolution to the
current locale happens at display time.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Universe locale + translations fields

**Files:**
- Modify: `crates/map-domain/src/universe.rs`

- [ ] **Step 1: Write the failing test** — append to the existing `tests` mod in `crates/map-domain/src/universe.rs`:

```rust
    #[test]
    fn universe_translations_and_locale_fields() {
        let mut u = Universe::default();
        u.translations.insert((20101, 122701), "Cerberus Vanguard".into());
        u.available_locales = vec![44, 49, 33];
        u.current_locale = 44;
        assert_eq!(u.translations.get(&(20101, 122701)).map(String::as_str), Some("Cerberus Vanguard"));
        assert_eq!(u.available_locales, vec![44, 49, 33]);
        assert_eq!(u.current_locale, 44);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p map-domain universe_translations_and_locale_fields`
Expected: FAIL — missing fields.

- [ ] **Step 3: Add the fields**

Inside `pub struct Universe { ... }` in `crates/map-domain/src/universe.rs`, add:

```rust
    /// Full translation table: (page_id, text_id) -> display string. Populated at
    /// galaxy load from `t/0001-l<locale>.xml`. Consumed by the entity-label
    /// resolver and the ware-name lookup.
    pub translations: std::collections::HashMap<(u32, u32), String>,
    /// Locale IDs the installed game ships (parsed from `t/0001-l*.xml` filenames).
    /// Drives the top-bar language dropdown.
    pub available_locales: Vec<u32>,
    /// Locale ID this Universe was loaded with (e.g. 44 for English).
    pub current_locale: u32,
```

Also update the test `make_universe()` literal in the same file to add the three new fields. The simplest edit:

```rust
    Universe {
        sector_macros: HashMap::new(),
        faction_strings: HashMap::new(),
        faction_table: HashMap::new(),
        ware_names: HashMap::new(),
        translations: HashMap::new(),
        available_locales: Vec::new(),
        current_locale: 0,
        sectors: vec![ /* ... */ ],
        clusters: vec![],
        connections: vec![ /* ... */ ],
    }
```

(Keep the existing `sectors` and `connections` entries intact; only the three new fields are added.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p map-domain`
Expected: PASS.

Then run: `cargo build -p map-io && cargo build -p map-app`
Expected: clean. The Universe construction sites in `xml_parser.rs::parse_galaxy_from_game` and `parse_galaxy_str` will fail to compile because they don't set the new fields; fix each by adding `translations: HashMap::new(), available_locales: Vec::new(), current_locale: 44,` to those struct literals. (Real population happens in Task 7; for now they get empty defaults so the workspace builds.)

- [ ] **Step 5: Commit**

```bash
git add crates/map-domain/src/universe.rs crates/map-io/src/xml_parser.rs
git commit -m "$(cat <<'EOF'
feat(domain): Universe.translations + locale fields

Plumbing only — full population lands in the galaxy-loader task.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: EntityRecord.display_name_ref plumbing

**Files:**
- Modify: `crates/map-io/src/save_parser/types.rs`
- Modify: `crates/map-io/src/save_parser/sector_chunk.rs`
- Modify: `crates/map-io/src/save_parser/merge.rs`

- [ ] **Step 1: Update the existing `entity_record_constructs` test** in `crates/map-io/src/save_parser/types.rs` to include the new field. Replace the test with:

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
            trade_offers: vec![],
            display_name_ref: Some("{20102,1701}".into()),
        };
        assert_eq!(e.id, 0x100);
        assert_eq!(e.display_name_ref.as_deref(), Some("{20102,1701}"));
        assert!(e.trade_offers.is_empty());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p map-io entity_record_constructs`
Expected: FAIL — missing field `display_name_ref`.

- [ ] **Step 3: Add the field**

Add to the `EntityRecord` struct in `crates/map-io/src/save_parser/types.rs`:

```rust
    pub display_name_ref: Option<String>,
```

- [ ] **Step 4: Update existing construction sites + tests**

In `crates/map-io/src/save_parser/sector_chunk.rs`:

- Add `display_name_ref: Option<String>` to `struct Pending`.
- Initialise `display_name_ref: None` in `build_pending`.
- In the `Event::End b"component"` branch where the `EntityRecord { ... }` literal is built, add `display_name_ref: p.display_name_ref.take(),` (the field is `Option<String>`; `take()` moves it out).
- `let mut p = stack.pop().unwrap();` is already in place from a previous task.

In `crates/map-io/src/save_parser/merge.rs`, update all four test `EntityRecord { ... }` literals (in `merges_records_and_assigns_faction_ids`, `unknown_sector_drops_entity`, `no_sector_macros_drops_all`, `trade_offers_propagated_to_world`) by appending `display_name_ref: None,` after `trade_offers: ...`.

- [ ] **Step 5: Run map-io tests**

Run: `cargo test -p map-io`
Expected: PASS, including the updated `entity_record_constructs`.

- [ ] **Step 6: Commit**

```bash
git add crates/map-io/src/save_parser/types.rs crates/map-io/src/save_parser/sector_chunk.rs crates/map-io/src/save_parser/merge.rs
git commit -m "$(cat <<'EOF'
feat(save_parser): display_name_ref field on EntityRecord + Pending

Plumbing only — parser still emits None. Name/basename capture
lands in the next commit.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Parse `name=` / `basename=` in sector_chunk

**Files:**
- Modify: `crates/map-io/src/save_parser/sector_chunk.rs`

- [ ] **Step 1: Write the failing test** — append to the existing `tests` module:

```rust
    #[test]
    fn parses_ship_name_station_basename_and_literal() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="ship_l" macro="ship_par_l_trans_container_03_a_macro"
             name="{20101,122701}" code="AKV-484" owner="alliance" id="[0x10]">
    <offset><position x="0" y="0" z="0"/></offset>
  </component>
  <component class="station" macro="station_gen_factory_base_01_macro"
             basename="{20102,1701}" code="FAR-140" owner="freesplit" id="[0x20]">
    <offset><position x="0" y="0" z="0"/></offset>
  </component>
  <component class="station" macro="station_gen_factory_base_01_macro"
             name="{20103,2001}" basename="{20103,2001}"
             code="NLK-443" owner="split" id="[0x21]">
    <offset><position x="0" y="0" z="0"/></offset>
  </component>
  <component class="ship_s" macro="ship_arg_s_scout_01_a_macro"
             name="My Best Ship" code="MBS-001" owner="player" id="[0x30]">
    <offset><position x="0" y="0" z="0"/></offset>
  </component>
  <component class="ship_xs" macro="ship_gen_xs_escapepod_01_a_macro"
             code="PXP-294" owner="paranid" id="[0x40]">
    <offset><position x="0" y="0" z="0"/></offset>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let ship_l = out.iter().find(|r| r.id == 0x10).unwrap();
        assert_eq!(ship_l.display_name_ref.as_deref(), Some("{20101,122701}"));

        let station_basename = out.iter().find(|r| r.id == 0x20).unwrap();
        assert_eq!(station_basename.display_name_ref.as_deref(), Some("{20102,1701}"));

        // When both name= and basename= are present, name= wins.
        let station_named = out.iter().find(|r| r.id == 0x21).unwrap();
        assert_eq!(station_named.display_name_ref.as_deref(), Some("{20103,2001}"));

        let renamed_ship = out.iter().find(|r| r.id == 0x30).unwrap();
        assert_eq!(renamed_ship.display_name_ref.as_deref(), Some("My Best Ship"));

        let pod = out.iter().find(|r| r.id == 0x40).unwrap();
        assert_eq!(pod.display_name_ref, None);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p map-io parses_ship_name_station_basename_and_literal`
Expected: FAIL — all `display_name_ref` come back as `None` because `build_pending` doesn't read those attributes yet.

- [ ] **Step 3: Implement attribute capture in `build_pending`**

Edit `crates/map-io/src/save_parser/sector_chunk.rs`. In `build_pending`, after the existing `let owner = attr_str(e, b"owner");` line and before `Some(Pending { ... })`, add:

```rust
    let name_attr = attr_str(e, b"name");
    let basename_attr = attr_str(e, b"basename");
    // Stations: prefer `name=` if present, else `basename=`.
    // Ships:    use `name=`. (Ships do not carry `basename=`.)
    let display_name_ref = name_attr.or(basename_attr);
```

Then change the `Some(Pending { ... })` literal so it stores the captured value:

```rust
    Some(Pending {
        open_depth: depth,
        id,
        parent_id,
        macro_name,
        code,
        kind,
        owner,
        position: None,
        trade_offers: Vec::new(),
        display_name_ref,
    })
```

- [ ] **Step 4: Run the new test + existing chunk tests**

Run: `cargo test -p map-io sector_chunk`
Expected: PASS for all sector_chunk tests including the new one.

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/save_parser/sector_chunk.rs
git commit -m "$(cat <<'EOF'
feat(save_parser): capture name=/basename= on entity components

Stations prefer name= over basename=; ships use name=. Both are
stored raw on EntityRecord (could be a {p,t} ref, a compound
form, or a literal string for player-renamed ships) and resolved
to the current locale at display time.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Propagate display_name_ref in merge

**Files:**
- Modify: `crates/map-io/src/save_parser/merge.rs`

- [ ] **Step 1: Write the failing test** — append to the `tests` mod in `crates/map-io/src/save_parser/merge.rs`:

```rust
    #[test]
    fn display_name_refs_propagated_to_world() {
        let records = vec![EntityRecord {
            id: 0x55,
            parent_id: None,
            macro_name: "ship_par_l_trans_container_03_a_macro".into(),
            code: Some("AKV-484".into()),
            kind: LiveObjectKind::ShipLarge,
            owner: Some("alliance".into()),
            position: glam::Vec3::ZERO,
            sector_macro: "sa".into(),
            trade_offers: vec![],
            display_name_ref: Some("{20101,122701}".into()),
        }];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut next = 1u32;
        let world = merge(vec![records], Some(&sm), &mut fs, &mut next);
        assert_eq!(
            world.display_name_refs.get(&0x55).map(String::as_str),
            Some("{20101,122701}")
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p map-io display_name_refs_propagated_to_world`
Expected: FAIL — `world.display_name_refs[&0x55]` is absent.

- [ ] **Step 3: Implement propagation**

In `crates/map-io/src/save_parser/merge.rs`, inside the `for r in batch { ... }` loop, after the existing block that copies `trade_offers`, add the matching block for `display_name_ref`. The loop body becomes (showing only the relevant tail):

```rust
            let entity_id = r.id;
            let trade_offers = r.trade_offers;
            let display_name_ref = r.display_name_ref;
            world.insert_entity(
                entity_id,
                r.macro_name,
                r.kind,
                faction,
                r.position,
                sec_id,
                r.parent_id,
                r.code,
            );
            if !trade_offers.is_empty() {
                world.trade_offers.insert(entity_id, trade_offers);
            }
            if let Some(name_ref) = display_name_ref {
                world.display_name_refs.insert(entity_id, name_ref);
            }
```

- [ ] **Step 4: Run all merge tests**

Run: `cargo test -p map-io merge`
Expected: PASS — new test green, four existing tests still green.

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/save_parser/merge.rs
git commit -m "$(cat <<'EOF'
feat(save_parser): copy EntityRecord.display_name_ref into World

The label resolver reads World.display_name_refs at render time
to produce locale-correct entity labels.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Translation parser retains all pages

**Files:**
- Modify: `crates/map-io/src/xml_parser.rs`

- [ ] **Step 1: Write the failing test** — append to the existing `#[cfg(test)] mod tests` block at the bottom of `crates/map-io/src/xml_parser.rs`:

```rust
    #[test]
    fn parse_translations_xml_retains_all_pages_with_correct_branch() {
        let xml = r#"<?xml version="1.0"?>
<language id="44">
  <page id="20003">
    <t id="1">{20003,2} {20004,1}(Argon Prime Cluster)</t>
  </page>
  <page id="20004">
    <t id="10011">{20004,10012} {20004,10013}(Argon Prime)</t>
  </page>
  <page id="20101">
    <t id="122701">Cerberus Vanguard</t>
  </page>
  <page id="10000">
    <t id="500">Some Lore Snippet</t>
  </page>
</language>"#;
        let map = super::parse_translations_xml(xml).unwrap();
        // 20003 + 20004 keep parenthetical extraction (existing behaviour).
        assert_eq!(map.get(&(20003, 1)).map(String::as_str), Some("Argon Prime Cluster"));
        assert_eq!(map.get(&(20004, 10011)).map(String::as_str), Some("Argon Prime"));
        // 20101 (ship class names) — plain text retained.
        assert_eq!(map.get(&(20101, 122701)).map(String::as_str), Some("Cerberus Vanguard"));
        // Unrelated page also kept.
        assert_eq!(map.get(&(10000, 500)).map(String::as_str), Some("Some Lore Snippet"));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p map-io parse_translations_xml_retains_all_pages_with_correct_branch`
Expected: FAIL — pages 20101 and 10000 are currently dropped by the page-id whitelist.

- [ ] **Step 3: Generalise the page filter**

In `crates/map-io/src/xml_parser.rs::parse_translations_xml`:

1. Locate the `<t>` Start arm that currently guards on `if matches!(current_page, Some(20003 | 20004 | 20201))`. Remove the guard so every `<t>` element is captured:

```rust
                b"t" => {
                    current_text_id = attr_value(e, b"id").and_then(|s| s.parse().ok());
                }
```

2. Locate the `Event::Text` arm. The current per-page branch (added in an earlier task) selects between parenthetical extraction and plain text. Generalise the branch so pages 20003 and 20004 keep the parenthetical convention and every other page uses plain text:

```rust
            Event::Text(e) => {
                if let (Some(page_id), Some(text_id)) = (current_page, current_text_id) {
                    let decoded = e.decode().unwrap_or_default();
                    let content =
                        quick_xml::escape::unescape(&decoded).unwrap_or_else(|_| decoded.clone());
                    // 20003 + 20004 (cluster / sector names) follow the
                    // "{ref} {ref}(Display Name)" convention — take only the
                    // parenthetical. Everything else (ware names, ship class
                    // names, lore text, …) is plain text.
                    let name = if matches!(page_id, 20003 | 20004) {
                        extract_last_parenthetical(&content)
                    } else {
                        let t = content.trim().to_string();
                        if t.is_empty() { None } else { Some(t) }
                    };
                    if let Some(name) = name {
                        translations.insert((page_id, text_id), name);
                    }
                    current_text_id = None;
                }
            }
```

- [ ] **Step 4: Run translation tests**

Run: `cargo test -p map-io xml_parser`
Then: `cargo test -p map-io`
Expected: PASS — new test green, all pre-existing translation tests still green (in particular the previously-added `parse_translations_xml_includes_wares_page_20201` and `parse_translations_xml_skips_empty_ware_entry`).

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/xml_parser.rs
git commit -m "$(cat <<'EOF'
feat(xml_parser): retain every translation page

Drop the per-page whitelist; keep parenthetical extraction only for
20003/20004 (cluster/sector names) and use plain text for every
other page. Ship class names (20101), station basenames (20102/3),
ware names (20201), and lore text all now reach the lookup table.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Galaxy loader accepts locale + stores translations

**Files:**
- Modify: `crates/map-io/src/xml_parser.rs`
- Modify: `crates/map-app/src/main.rs` (callsite)

- [ ] **Step 1: Add the `locale: u32` parameter to `parse_galaxy_from_game`**

In `crates/map-io/src/xml_parser.rs`, change the signature of `parse_galaxy_from_game`:

```rust
pub fn parse_galaxy_from_game(
    game_dir: &Path,
    locale: u32,
) -> Result<Universe, ParseError> {
```

Inside the function, replace the hard-coded translations path:

```rust
    let translations_path = format!("t/0001-l{:03}.xml", locale);
    let translations_data = load(&translations_path)?;
```

(The `load` closure is already defined inside the function.)

- [ ] **Step 2: Populate the new Universe fields**

Still inside `parse_galaxy_from_game`, after `translations` is parsed, build the locale list once:

```rust
    let available_locales = crate::game_path::list_available_locales(game_dir);
```

(This helper lands in Task 8. For this task, the call won't compile yet — that's OK, Task 8 ships before integration testing.)

Then at the final `Universe { ... }` literal at the end of the function, set the three fields:

```rust
    Ok(Universe {
        // ... existing fields ...
        ware_names,
        translations,
        available_locales,
        current_locale: locale,
        // ... rest unchanged ...
    })
```

If the `parse_galaxy_str` helper or any other internal builder constructs a `Universe { ... }` literal in this file, leave `translations: HashMap::new(), available_locales: Vec::new(), current_locale: 44,` on those — they are not for the runtime path.

- [ ] **Step 3: Update the caller in `main.rs`**

In `crates/map-app/src/main.rs::load_universe`, change the call:

```rust
    match map_io::xml_parser::parse_galaxy_from_game(&game_dir, 44) {
```

(Locale 44 = English. Settings-driven locale lands in Tasks 13–14, but for this task wiring the hard-coded English keeps the binary compiling.)

- [ ] **Step 4: Compile and test**

Run: `cargo build`
Expected: depends on Task 8 landing first for `list_available_locales`; if Tasks 7 and 8 are dispatched in either order the compile breaks until both are in. Workaround: the implementer of Task 7 may use a `vec![44u32]` stub inline and a `// TODO Task 8` comment, then Task 8 replaces it.

Actually — simpler: implement Task 7 to use `vec![44]` literally as a placeholder and remove that placeholder in Task 8. To avoid the TODO, Task 8 should be dispatched before Task 7, or the two should be merged. Recommended dispatch order: Task 8 first, then Task 7.

After Task 8 lands, run: `cargo test -p map-io` — expected PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/xml_parser.rs crates/map-app/src/main.rs
git commit -m "$(cat <<'EOF'
feat(xml_parser): accept locale + populate Universe.translations

parse_galaxy_from_game now takes locale: u32 and stores the full
translation table + locale metadata on the returned Universe.
The label resolver reads it at display time.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: game_path locale helpers

**Files:**
- Modify: `crates/map-io/src/game_path.rs`

- [ ] **Step 1: Write the failing test** — append to the existing `#[cfg(test)] mod tests` block in `crates/map-io/src/game_path.rs`:

```rust
    #[test]
    fn locale_display_name_known_ids() {
        assert_eq!(locale_display_name(44), "English");
        assert_eq!(locale_display_name(49), "Deutsch");
        assert_eq!(locale_display_name(7),  "Русский");
        assert_eq!(locale_display_name(86), "中文(简体)");
        assert_eq!(locale_display_name(380), "Українська");
    }

    #[test]
    fn locale_display_name_unknown_falls_back_to_id() {
        assert_eq!(locale_display_name(999), "l999");
    }
```

(Note: `list_available_locales` is not unit-tested here because it requires a cat archive fixture; it is exercised end-to-end via the manual verification in Task 17 and indirectly via the integration test suite.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p map-io locale_display_name`
Expected: FAIL — function does not exist.

- [ ] **Step 3: Implement both helpers**

Append to `crates/map-io/src/game_path.rs`:

```rust
/// Enumerate locale IDs the installed game ships, by scanning the cat archive
/// for `t/0001-lNNN.xml` filenames. Returns IDs sorted ascending, deduped.
/// Empty Vec if the game dir has no locale files or cannot be read.
pub fn list_available_locales(game_dir: &Path) -> Vec<u32> {
    let files = crate::cat_reader::list_files_matching(game_dir, "t/0001-l", ".xml");
    let mut ids: Vec<u32> = Vec::new();
    for (path, _data) in files {
        // Path looks like "t/0001-l044.xml" — extract the digits after `-l`.
        if let Some(after_l) = path.rsplit_once("-l").map(|(_, b)| b) {
            if let Some(num) = after_l.strip_suffix(".xml") {
                if let Ok(id) = num.parse::<u32>() {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
    }
    ids.sort_unstable();
    ids
}

/// Human-friendly native name for a locale ID, used in the language dropdown.
/// Unknown IDs render as `l<NNN>` so the user can at least identify the file.
pub fn locale_display_name(id: u32) -> &'static str {
    match id {
        7   => "Русский",
        33  => "Français",
        34  => "Español",
        39  => "Italiano",
        42  => "Čeština",
        44  => "English",
        48  => "Polski",
        49  => "Deutsch",
        55  => "Português (Brasil)",
        81  => "日本語",
        82  => "한국어",
        86  => "中文(简体)",
        88  => "中文(繁體)",
        90  => "Türkçe",
        359 => "Български",
        380 => "Українська",
        _   => locale_unknown_label(id),
    }
}

fn locale_unknown_label(id: u32) -> &'static str {
    // Keep a tiny static cache for unknown ids encountered at runtime so we can
    // hand out &'static str. Worst-case grows by the number of DLC-shipped
    // locales we don't recognise (currently zero).
    use std::sync::OnceLock;
    use std::sync::Mutex;
    static CACHE: OnceLock<Mutex<std::collections::HashMap<u32, &'static str>>> = OnceLock::new();
    let map = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut guard = map.lock().unwrap();
    if let Some(s) = guard.get(&id) {
        return *s;
    }
    let leaked: &'static str = Box::leak(format!("l{id}").into_boxed_str());
    guard.insert(id, leaked);
    leaked
}
```

> **Why `OnceLock + Box::leak`:** the function returns `&'static str`. Known locales live in the match arms (string literals are already `'static`). Unknown IDs need an allocated string, but we want to hand out the same `&'static` each time so callers can compare pointers or store them indefinitely. The leak is bounded by the number of distinct unknown IDs ever seen in a session (currently zero in practice).

- [ ] **Step 4: Run tests**

Run: `cargo test -p map-io game_path`
Expected: PASS for both new tests.

Run: `cargo build`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/game_path.rs
git commit -m "$(cat <<'EOF'
feat(game_path): list_available_locales + locale_display_name

Discovers locale IDs from cat-archive filenames; maps known IDs to
native display names for the language dropdown. Unknown IDs fall
back to `l<NNN>` via a per-session leaked-string cache so the
function can return &'static str.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: replace_translation_refs helper

**Files:**
- Modify: `crates/map-app/src/colors.rs`

- [ ] **Step 1: Write the failing tests** — append a new test module to the bottom of `crates/map-app/src/colors.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_translations() -> HashMap<(u32, u32), String> {
        let mut m = HashMap::new();
        m.insert((20101, 122701), "Cerberus Vanguard".into());
        m.insert((20203, 401), "Argon Federation".into());
        m
    }

    #[test]
    fn replace_translation_refs_single_ref() {
        let t = sample_translations();
        assert_eq!(
            replace_translation_refs("{20101,122701}", &t),
            "Cerberus Vanguard"
        );
    }

    #[test]
    fn replace_translation_refs_compound() {
        let t = sample_translations();
        assert_eq!(
            replace_translation_refs("{20101,122701} ({20203,401})", &t),
            "Cerberus Vanguard (Argon Federation)"
        );
    }

    #[test]
    fn replace_translation_refs_literal_passes_through() {
        let t = sample_translations();
        assert_eq!(
            replace_translation_refs("My Best Ship", &t),
            "My Best Ship"
        );
    }

    #[test]
    fn replace_translation_refs_unknown_key_left_intact() {
        let t = sample_translations();
        // Useful for debugging — missing translation IDs stay visible.
        assert_eq!(
            replace_translation_refs("{99999,1}", &t),
            "{99999,1}"
        );
    }

    #[test]
    fn replace_translation_refs_malformed_left_intact() {
        let t = sample_translations();
        assert_eq!(replace_translation_refs("{not,a,ref}", &t), "{not,a,ref}");
        assert_eq!(replace_translation_refs("{",          &t), "{");
        assert_eq!(replace_translation_refs("plain text", &t), "plain text");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p map-app replace_translation_refs`
Expected: FAIL — function does not exist.

- [ ] **Step 3: Implement the helper**

At the bottom of `crates/map-app/src/colors.rs` (above the new tests mod), add:

```rust
/// Substitute every `{page,id}` substring with its resolved translation, leaving
/// other text intact. Used for compound names like `{p,t} ({p,t})` and for
/// literal user-renamed ships (which contain no braces and pass through unchanged).
/// Unknown translation keys and malformed brace groups are left as-is, which
/// helps spot missing IDs while debugging.
pub fn replace_translation_refs(
    s: &str,
    translations: &std::collections::HashMap<(u32, u32), String>,
) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'{' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        // Find the matching '}'. If absent, treat the '{' as a literal.
        let Some(close_rel) = bytes[i + 1..].iter().position(|&b| b == b'}') else {
            out.push('{');
            i += 1;
            continue;
        };
        let close = i + 1 + close_rel;
        let inner = &s[i + 1..close];
        // Inner must be exactly `<digits>,<digits>` with no extra whitespace.
        let parsed: Option<(u32, u32)> = inner
            .split_once(',')
            .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)));
        match parsed.and_then(|(p, t)| translations.get(&(p, t))) {
            Some(text) => {
                out.push_str(text);
                i = close + 1;
            }
            None => {
                // Unknown key OR malformed inner — emit the brace group verbatim.
                out.push_str(&s[i..=close]);
                i = close + 1;
            }
        }
    }
    out
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p map-app replace_translation_refs`
Expected: PASS for all five.

- [ ] **Step 5: Commit**

```bash
git add crates/map-app/src/colors.rs
git commit -m "$(cat <<'EOF'
feat(colors): replace_translation_refs helper

Scans a string for {page,id} substrings and substitutes each via
the Universe.translations map. Compound forms like
`{p,t} ({p,t})` work naturally. Literals pass through.
Unknown / malformed keys stay verbatim so missing translation
IDs remain visible in the UI.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: resolve_entity_label helper

**Files:**
- Modify: `crates/map-app/src/colors.rs`

- [ ] **Step 1: Write the failing tests** — append to the `tests` mod in `crates/map-app/src/colors.rs`:

```rust
    use map_domain::ids::{FactionId, SectorId};
    use map_domain::universe::Universe;
    use map_domain::world::{LiveObjectKind, World};

    fn sample_universe() -> Universe {
        let mut u = Universe::default();
        u.translations = sample_translations();
        u.current_locale = 44;
        u
    }

    fn sample_world() -> World {
        let mut w = World::new();
        // Entity 1: has both a display_name_ref and a code.
        w.insert_entity(
            1, "ship_par_l_trans_container_03_a_macro".into(),
            LiveObjectKind::ShipLarge, Some(FactionId(1)),
            glam::Vec3::ZERO, SectorId(1), None, Some("AKV-484".into()),
        );
        w.display_name_refs.insert(1, "{20101,122701}".into());

        // Entity 2: literal name + code (player-renamed ship).
        w.insert_entity(
            2, "ship_arg_s_scout_01_a_macro".into(),
            LiveObjectKind::ShipSmall, None, glam::Vec3::ZERO,
            SectorId(1), None, Some("MBS-001".into()),
        );
        w.display_name_refs.insert(2, "My Best Ship".into());

        // Entity 3: no display_name_ref, no code — pure macro fallback.
        w.insert_entity(
            3, "ship_xen_n_fighter_01_a_macro".into(),
            LiveObjectKind::ShipSmall, None, glam::Vec3::ZERO,
            SectorId(1), None, None,
        );
        // Entity 4: has display_name_ref, no code.
        w.insert_entity(
            4, "ship_xen_p_destroyer_01_a_macro".into(),
            LiveObjectKind::ShipLarge, None, glam::Vec3::ZERO,
            SectorId(1), None, None,
        );
        w.display_name_refs.insert(4, "{20101,122701}".into());

        w
    }

    #[test]
    fn resolve_entity_label_name_and_code() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(resolve_entity_label(&w, &u, 1), "Cerberus Vanguard (AKV-484)");
    }

    #[test]
    fn resolve_entity_label_literal_name_and_code() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(resolve_entity_label(&w, &u, 2), "My Best Ship (MBS-001)");
    }

    #[test]
    fn resolve_entity_label_macro_fallback_when_nothing_known() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(resolve_entity_label(&w, &u, 3), "ship xen n fighter 01 a");
    }

    #[test]
    fn resolve_entity_label_name_only_when_no_code() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(resolve_entity_label(&w, &u, 4), "Cerberus Vanguard");
    }

    #[test]
    fn resolve_entity_label_unknown_entity_id_falls_back_to_empty() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(resolve_entity_label(&w, &u, 999), "");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p map-app resolve_entity_label`
Expected: FAIL — function does not exist.

- [ ] **Step 3: Implement the resolver**

In `crates/map-app/src/colors.rs`, above the test module, add:

```rust
/// Resolve a human label for one live entity, used by the side panel and
/// the 3D hover tooltip.
///
/// Returns `"Class Name (CODE)"` when both the resolved class name and the
/// short code are known; falls back to the class name alone, then the code
/// alone, and finally to `strip_macro(macro_name)` if nothing else is
/// available. Returns an empty string when the entity id is unknown to the
/// World.
pub fn resolve_entity_label(
    world: &map_domain::world::World,
    universe: &map_domain::universe::Universe,
    eid: map_domain::world::EntityId,
) -> String {
    if !world.names.contains_key(&eid) {
        return String::new();
    }
    let macro_name = world.names.get(&eid).cloned().unwrap_or_default();
    let class_name: Option<String> = world.display_name_refs.get(&eid).map(|raw| {
        replace_translation_refs(raw, &universe.translations)
    });
    let class_name: Option<String> = class_name
        .filter(|s| !s.is_empty() && !s.starts_with('{'))
        .or_else(|| {
            let stripped = strip_macro(&macro_name);
            if stripped.is_empty() { None } else { Some(stripped) }
        });
    let code = world.codes.get(&eid).cloned();
    match (class_name, code) {
        (Some(c), Some(code)) => format!("{c} ({code})"),
        (Some(c), None)       => c,
        (None, Some(code))    => code,
        (None, None)          => macro_name,
    }
}
```

(`strip_macro` already exists in this file; the resolver re-uses it.)

> **Why `.filter(|s| ... !s.starts_with('{'))`:** if `replace_translation_refs` returned the original `{p,t}` (unknown key, see Task 9), it is not a real display name and we fall through to the macro stripper for a sensible label.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p map-app resolve_entity_label`
Expected: PASS for all five.

- [ ] **Step 5: Commit**

```bash
git add crates/map-app/src/colors.rs
git commit -m "$(cat <<'EOF'
feat(colors): resolve_entity_label helper

Builds the per-entity display label used by the side panel and
the 3D tooltip: resolved class name + code, with strip_macro as
the final fallback so a missing translation never produces an
empty label.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: Adopt resolver in the side panel

**Files:**
- Modify: `crates/map-app/src/ui/sector_panel.rs`

> No new test added for this task: the rendering call sites are wired through to the resolver, which is independently tested in Task 10. Visual confirmation happens in Task 17.

- [ ] **Step 1: Change `entity_row_label` to take `&Universe` and use the new resolver**

In `crates/map-app/src/ui/sector_panel.rs`, replace the existing `entity_row_label` function:

```rust
fn entity_row_label(
    world: &map_domain::world::World,
    universe: &map_domain::universe::Universe,
    eid: map_domain::world::EntityId,
) -> (String, &'static str) {
    use map_domain::world::LiveObjectKind;
    let icon = match world.kinds.get(&eid) {
        Some(LiveObjectKind::Station) => "◼",
        Some(LiveObjectKind::ShipExtraLarge) | Some(LiveObjectKind::ShipLarge) => "▲",
        Some(LiveObjectKind::ShipMedium) => "▶",
        _ => "▴",
    };
    let label = crate::colors::resolve_entity_label(world, universe, eid);
    (label, icon)
}
```

- [ ] **Step 2: Update all callers in this file**

There are four call sites — update each to pass `universe`:

1. Inside the live-entities collapsible block (look for `let (label, icon) = entity_row_label(world, eid);`):
   ```rust
   let (label, icon) = entity_row_label(world, universe, eid);
   ```
2. Inside the SELECTED back-to-parent button (`entity_row_label(w, parent).0`):
   ```rust
   let parent_label = world.map(|w| entity_row_label(w, universe, parent).0).unwrap_or_default();
   ```
3. Inside the SELECTED entity detail block (`let (label, icon) = entity_row_label(world, eid);`):
   ```rust
   let (label, icon) = entity_row_label(world, universe, eid);
   ```
4. Inside the DOCKED list (`let (clabel, cicon) = entity_row_label(world, cid);`):
   ```rust
   let (clabel, cicon) = entity_row_label(world, universe, cid);
   ```

- [ ] **Step 3: The `strip_macro_removes_suffix_and_underscores` test still exercises `crate::colors::strip_macro` directly. Leave it as-is** — `strip_macro` is still a `pub` helper used by the new resolver.

- [ ] **Step 4: Compile + test**

Run: `cargo build`
Expected: clean.

Run: `cargo test -p map-app`
Expected: 31+ pre-existing tests pass plus the new `resolve_entity_label` and `replace_translation_refs` tests from Tasks 9 and 10.

- [ ] **Step 5: Commit**

```bash
git add crates/map-app/src/ui/sector_panel.rs
git commit -m "$(cat <<'EOF'
feat(panel): use resolve_entity_label for row + detail labels

Entity rows and the SELECTED block now show the X4-style class
name with the code in parentheses (e.g. "Cerberus Vanguard
(AKV-484)") instead of the raw stripped macro. User-renamed
ships display their literal name.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: Rewrite 3D hover tooltip

**Files:**
- Modify: `crates/map-app/src/ui/sector_view.rs`

- [ ] **Step 1: Locate `draw_hover_label`** in `crates/map-app/src/ui/sector_view.rs`. The function builds a `Vec<(String, Color32)>` of label lines, one per row.

Replace the `ClickedTarget::Entity(eid)` arm. The current block uses `world.codes`, `world.names`, and `strip_macro`. Replace it with the new resolver and a two-line "name then code" layout:

```rust
        ClickedTarget::Entity(eid) => {
            if let Some(world) = world {
                let name = crate::colors::resolve_entity_label_without_code(world, universe, eid);
                if !name.is_empty() {
                    lines.push((name, crate::theme::TEXT_PRIMARY));
                }
                if let Some(code) = world.codes.get(&eid) {
                    lines.push((code.clone(), crate::theme::TEXT_MUTED));
                }
                if let Some(&fid) = world.factions.get(&eid) {
                    let f_name = crate::colors::faction_name(universe, fid);
                    let f_color = crate::colors::faction_color(universe, fid);
                    lines.push((f_name.to_string(), f_color));
                }
                world.positions.get(&eid).copied().and_then(project)
            } else {
                None
            }
        }
```

The tooltip needs the **name without the appended code** (since the code becomes its own line). Add a sibling helper in `crates/map-app/src/colors.rs` (right next to `resolve_entity_label`):

```rust
/// Resolve only the class-name portion of the entity label (no code in parens).
/// Used by callers that render the code on its own line.
pub fn resolve_entity_label_without_code(
    world: &map_domain::world::World,
    universe: &map_domain::universe::Universe,
    eid: map_domain::world::EntityId,
) -> String {
    if !world.names.contains_key(&eid) {
        return String::new();
    }
    let macro_name = world.names.get(&eid).cloned().unwrap_or_default();
    world
        .display_name_refs
        .get(&eid)
        .map(|raw| replace_translation_refs(raw, &universe.translations))
        .filter(|s| !s.is_empty() && !s.starts_with('{'))
        .unwrap_or_else(|| {
            let stripped = strip_macro(&macro_name);
            if stripped.is_empty() { macro_name } else { stripped }
        })
}
```

Add one unit test for it in the same `tests` mod (alongside the Task-10 tests):

```rust
    #[test]
    fn resolve_entity_label_without_code_omits_code() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(resolve_entity_label_without_code(&w, &u, 1), "Cerberus Vanguard");
        assert_eq!(resolve_entity_label_without_code(&w, &u, 2), "My Best Ship");
        assert_eq!(resolve_entity_label_without_code(&w, &u, 3), "ship xen n fighter 01 a");
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p map-app resolve_entity_label_without_code`
Expected: PASS.

Run: `cargo build`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add crates/map-app/src/colors.rs crates/map-app/src/ui/sector_view.rs
git commit -m "$(cat <<'EOF'
feat(tooltip): two-line entity tooltip with resolved name

Hover tooltip now reads: resolved class name (primary text),
short code on the second line (muted), faction on the third
(faction colour). Matches X4's own info-panel ordering and shows
user-renamed ships correctly.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: Settings module

**Files:**
- Create: `crates/map-app/src/settings.rs`
- Modify: `crates/map-app/src/main.rs` (declare module)

- [ ] **Step 1: Write the failing tests** — create `crates/map-app/src/settings.rs` with this initial content (test only):

```rust
//! Persistent app settings stored via eframe's Storage trait.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_locale_is_english() {
        let s = AppSettings::default();
        assert_eq!(s.locale, 44);
    }

    #[test]
    fn settings_serde_roundtrip() {
        let s = AppSettings { locale: 49 };
        let json = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.locale, 49);
    }
}
```

- [ ] **Step 2: Declare the module in `main.rs`**

In `crates/map-app/src/main.rs`, add near the top with the other module declarations:

```rust
mod settings;
```

Verify `serde_json` is available for tests. If not already a dev-dependency, add to `crates/map-app/Cargo.toml`:

```toml
[dev-dependencies]
serde_json = "1"
```

Check first with `grep serde_json crates/map-app/Cargo.toml` — eframe transitively pulls serde, but `serde_json` is needed for the round-trip test.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p map-app default_locale_is_english`
Expected: FAIL — `AppSettings` does not exist.

- [ ] **Step 4: Implement `AppSettings` + storage helpers**

Replace `crates/map-app/src/settings.rs` with the full implementation:

```rust
//! Persistent app settings stored via eframe's Storage trait.

use serde::{Deserialize, Serialize};

const STORAGE_KEY: &str = "foundations-map-settings";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// X4 locale ID — 44 (English) by default.
    pub locale: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { locale: 44 }
    }
}

/// Load persisted settings. Falls back to `AppSettings::default()` if storage
/// is absent (first run, or no persistence backend available).
pub fn load(storage: Option<&dyn eframe::Storage>) -> AppSettings {
    storage
        .and_then(|s| eframe::get_value::<AppSettings>(s, STORAGE_KEY))
        .unwrap_or_default()
}

/// Persist settings. Called from `eframe::App::save`.
pub fn save(storage: &mut dyn eframe::Storage, s: &AppSettings) {
    eframe::set_value(storage, STORAGE_KEY, s);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_locale_is_english() {
        let s = AppSettings::default();
        assert_eq!(s.locale, 44);
    }

    #[test]
    fn settings_serde_roundtrip() {
        let s = AppSettings { locale: 49 };
        let json = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.locale, 49);
    }
}
```

- [ ] **Step 5: Confirm Cargo deps**

If `serde` isn't already a direct dependency of `map-app`, add to `crates/map-app/Cargo.toml`:

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
```

(`eframe` re-exports `Storage` but does not re-export `serde::Serialize`/`Deserialize`.) Verify with `grep -A 10 dependencies crates/map-app/Cargo.toml` before editing.

- [ ] **Step 6: Run tests**

Run: `cargo test -p map-app settings`
Expected: PASS for both.

Run: `cargo build`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/map-app/src/settings.rs crates/map-app/src/main.rs crates/map-app/Cargo.toml
git commit -m "$(cat <<'EOF'
feat(app): AppSettings persisted via eframe Storage

Holds the chosen locale ID across app restarts. Default is 44
(English). Loaded in App::new, written in App::save.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 14: App state + reload_galaxy

**Files:**
- Modify: `crates/map-app/src/app.rs`

> No new unit tests for this task — the App struct's behaviour is exercised end-to-end in Task 17. Egui integration is hard to unit-test in this codebase (no panel tests exist already).

- [ ] **Step 1: Add `settings` to the App struct**

In `crates/map-app/src/app.rs`, add a new field to `pub struct App { ... }`:

```rust
    pub settings: crate::settings::AppSettings,
```

- [ ] **Step 2: Load settings in `App::new`**

In `App::new`, before building the `Self { ... }` literal:

```rust
        let settings = crate::settings::load(cc.storage);
        eprintln!("[map] Loaded settings: locale={}", settings.locale);
```

Add `settings,` to the `Self { ... }` literal.

- [ ] **Step 3: Implement `eframe::App::save`**

In the `impl eframe::App for App { ... }` block, add a `save` method (eframe trait already provides a default no-op; we override it):

```rust
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        crate::settings::save(storage, &self.settings);
    }
```

(Check the existing impl block first; if a `save` method is already present, merge into it.)

- [ ] **Step 4: Add `reload_galaxy` method**

In the `impl App { ... }` block (NOT the eframe impl), add:

```rust
    pub fn reload_galaxy(&mut self, locale: u32, game_dir: &std::path::Path) {
        eprintln!("[map] Reloading universe with locale {}", locale);
        match map_io::xml_parser::parse_galaxy_from_game(game_dir, locale) {
            Ok(universe) => {
                self.universe = universe;
                self.settings.locale = locale;
                // Clear the stale snapshot; the next save parse will rebuild it.
                self.snapshot = None;
                self.snapshot_loading = true;
                crate::spawn_save_parse(
                    self.snapshot_tx.clone(),
                    self.universe.sector_macros.clone(),
                    self.universe.faction_strings.clone(),
                    (self.universe.faction_strings.len() as u32) + 1,
                );
            }
            Err(e) => {
                eprintln!("[map] Locale switch failed (parse error): {:?}", e);
            }
        }
    }
```

This depends on `self.snapshot_tx` already being on the App struct (it is — `pub snapshot_tx: mpsc::Sender<SnapshotMessage>`).

> **Why not push the new locale to `cc.storage` synchronously here:** eframe's `save()` runs on a timer (every ~30 s by default) and on exit. The user's choice will land within 30 s, which is fine for a setting that takes effect immediately on screen.

- [ ] **Step 5: Compile**

Run: `cargo build`
Expected: clean.

Run: `cargo test -p map-app`
Expected: PASS (no behaviour-change-sensitive tests exist for App).

- [ ] **Step 6: Commit**

```bash
git add crates/map-app/src/app.rs
git commit -m "$(cat <<'EOF'
feat(app): hold AppSettings + add reload_galaxy

App now loads persisted settings on init and writes them via the
eframe Storage save() hook. reload_galaxy re-parses the universe
with a new locale and kicks off a fresh save parse.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 15: Top-bar locale dropdown

**Files:**
- Modify: `crates/map-app/src/ui/top_bar.rs`
- Modify: `crates/map-app/src/app.rs`

- [ ] **Step 1: Extend `TopBarResponse`**

In `crates/map-app/src/ui/top_bar.rs`, change `TopBarResponse`:

```rust
#[derive(Default)]
pub struct TopBarResponse {
    pub refresh_clicked: bool,
    /// `Some(new_locale_id)` when the user picked a different locale in the
    /// dropdown this frame; `None` otherwise.
    pub locale_changed_to: Option<u32>,
}
```

- [ ] **Step 2: Extend `TopBar::show` signature + render the ComboBox**

Change `TopBar::show` to take the universe + current locale, and render the dropdown after the Refresh button:

```rust
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: Option<&SnapshotMeta>,
        loading: bool,
        available_locales: &[u32],
        current_locale: u32,
    ) -> TopBarResponse {
        let mut resp = TopBarResponse::default();
        ui.horizontal(|ui| {
            // ... existing label + search + refresh button block, unchanged ...

            ui.add_space(8.0);
            // Locale dropdown — disabled while loading so we don't trigger a
            // reload mid-parse.
            let mut chosen = current_locale;
            ui.add_enabled_ui(!loading, |ui| {
                egui::ComboBox::from_id_salt("locale-dropdown")
                    .selected_text(map_io::game_path::locale_display_name(current_locale))
                    .show_ui(ui, |ui| {
                        for &id in available_locales {
                            let label = map_io::game_path::locale_display_name(id);
                            ui.selectable_value(&mut chosen, id, label);
                        }
                    });
            });
            if chosen != current_locale {
                resp.locale_changed_to = Some(chosen);
            }

            ui.add_space(16.0);
            // ... existing snapshot status block, unchanged ...
        });
        resp
    }
```

- [ ] **Step 3: Update the existing unit tests**

The two `TopBar` unit tests in `top_bar.rs` don't call `show`, so they still compile. No edits needed there.

- [ ] **Step 4: Wire the dropdown response in `app.rs`**

In `crates/map-app/src/app.rs::ui`, locate the top-bar call:

```rust
        let mut refresh_clicked = false;
        egui::Panel::top("top_bar")
            .exact_size(36.0)
            .show_inside(ui, |ui| {
                let meta = self.snapshot.as_ref().map(|(m, _)| m);
                let resp = self.top_bar.show(ui, meta, self.snapshot_loading);
                refresh_clicked = resp.refresh_clicked;
            });
```

Replace with:

```rust
        let mut refresh_clicked = false;
        let mut locale_change: Option<u32> = None;
        egui::Panel::top("top_bar")
            .exact_size(36.0)
            .show_inside(ui, |ui| {
                let meta = self.snapshot.as_ref().map(|(m, _)| m);
                let resp = self.top_bar.show(
                    ui,
                    meta,
                    self.snapshot_loading,
                    &self.universe.available_locales,
                    self.settings.locale,
                );
                refresh_clicked = resp.refresh_clicked;
                locale_change = resp.locale_changed_to;
            });
        if let Some(new_locale) = locale_change {
            if let Some(game_dir) = map_io::game_path::detect() {
                self.reload_galaxy(new_locale, &game_dir);
            }
        }
```

- [ ] **Step 5: Compile + test**

Run: `cargo build`
Expected: clean.

Run: `cargo test -p map-app`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/map-app/src/ui/top_bar.rs crates/map-app/src/app.rs
git commit -m "$(cat <<'EOF'
feat(top_bar): locale dropdown

ComboBox lists every locale the install ships, identified by its
native display name. Changing it triggers App::reload_galaxy
which re-parses the universe and save with the new translations.
Disabled while a save parse is already in flight.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 16: Pass settings.locale to the initial galaxy load

**Files:**
- Modify: `crates/map-app/src/main.rs`

- [ ] **Step 1: Locate `load_universe` in `main.rs`** — currently hard-codes `44`:

```rust
    match map_io::xml_parser::parse_galaxy_from_game(&game_dir, 44) {
```

Replace with a parameter:

```rust
fn load_universe(locale: u32) -> map_domain::universe::Universe {
    let game_path = map_io::game_path::detect();
    let Some(game_dir) = game_path else {
        eprintln!("[map] Game path not found — starting with empty universe.");
        return map_domain::universe::Universe::default();
    };
    eprintln!("[map] Found game at: {:?}", game_dir);
    match map_io::xml_parser::parse_galaxy_from_game(&game_dir, locale) {
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

- [ ] **Step 2: Read settings before loading the universe**

In `main()`, before `let universe = load_universe(...);`, read the locale from persistent storage. eframe's `Storage` only becomes available inside the `CreationContext` callback, so we can't read it here directly. Instead, attempt to read it from the well-known eframe storage file via `eframe::storage_dir`:

```rust
    // Read persisted locale (if any) before launching the GUI so the initial
    // galaxy load uses the right locale.
    let initial_locale = {
        let storage = eframe::storage_dir("Foundations Map")
            .map(|dir| eframe::create_storage(&dir).ok())
            .flatten();
        let s = crate::settings::load(storage.as_deref());
        s.locale
    };
    let universe = load_universe(initial_locale);
```

> **eframe storage API note:** `eframe::storage_dir(app_name)` returns the path eframe uses for `Storage`, and `eframe::create_storage(path)` (or similar — verify API name against current eframe version) constructs the `Box<dyn Storage>`. Inspect `eframe`'s 0.34 docs first; if no public helper exists, the simpler fallback is to leave `initial_locale = 44` here and apply the user's persisted choice on the second frame via a deferred reload inside `App::new`. That deferred-reload fallback looks like:
>
> ```rust
> // In App::new, after constructing Self { ... }:
> if app.settings.locale != 44 {
>     if let Some(game_dir) = map_io::game_path::detect() {
>         app.reload_galaxy(app.settings.locale, &game_dir);
>     }
> }
> ```
>
> Pick whichever path actually compiles against eframe 0.34. If the deferred path is used, document that the first frame after a non-default-locale launch shows English briefly before the reload completes.

- [ ] **Step 3: Compile + test**

Run: `cargo build`
Expected: clean.

Run: `cargo test`
Expected: all pre-existing tests + the new ones from Tasks 1–13 pass.

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/src/main.rs crates/map-app/src/app.rs
git commit -m "$(cat <<'EOF'
feat(app): use persisted locale for the initial galaxy load

Reads AppSettings before constructing the universe so the first
frame is already in the user's chosen language. Falls back to
English when no persisted setting is found.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 17: Manual verification

**Files:** none.

- [ ] **Step 1: Run the app against the real save**

Run: `cargo run --release`
Expected: app launches; stderr shows `[map] Loaded settings: locale=44` (or the persisted value) and `[map] Loaded N sectors.`

- [ ] **Step 2: Verify default-locale name resolution**

1. Open the 3D view on any sector with several ships (e.g. an Argon trade hub).
2. Hover a large ship. Tooltip should show three lines: class name (e.g. "Argon Cerberus Vanguard"), code (e.g. "AKV-484"), faction.
3. Open the side panel CAPITALS list. Rows should read `Cerberus Vanguard (AKV-484)` rather than `AKV-484 — ship arg l ...`.
4. Click a station. SELECTED block should show the resolved station name + code.

- [ ] **Step 3: Verify user-renamed ships**

1. In X4 itself, rename one of your owned ships to "My Best Ship", quicksave.
2. The watcher should fire a reload (look for `[map] Save changed:` in stderr).
3. Locate that ship in the side panel; label should read `My Best Ship (CODE)`.

If you don't want to launch X4: this case is also covered by the unit test `parses_ship_name_station_basename_and_literal` from Task 4 and the `replace_translation_refs_literal_passes_through` test from Task 9.

- [ ] **Step 4: Verify locale switching**

1. Open the top-bar locale dropdown. Expected items: 16 native names (English, Deutsch, Français, ...).
2. Pick `Deutsch`. Wait ~2 s for the reload.
3. Sector names should change to German strings; ship class names likewise; station names; the side-panel faction labels; the TRADE section's ware names.
4. Pick `English` again. Verify the labels return to English.

- [ ] **Step 5: Verify persistence**

1. With `Deutsch` selected, close the app cleanly.
2. Re-launch.
3. Stderr should log `[map] Loaded settings: locale=49`.
4. App should come up with German labels from the first frame (or briefly English then German if the deferred-reload fallback was chosen in Task 16).

- [ ] **Step 6: Verify dropdown disable-on-loading**

1. Click `Refresh` — the dropdown should grey out while loading.
2. After loading completes the dropdown re-enables.

- [ ] **Step 7: If everything works, no commit needed.** Bugs become follow-up tasks rather than amendments.

---

## Self-Review Checklist (already performed)

- ✅ Every spec section maps to at least one task: domain types → Tasks 1+2; EntityRecord plumbing → Tasks 3+4+5; translation pages → Task 6; loader → Tasks 7+8; resolvers → Tasks 9+10; UI adoption → Tasks 11+12; settings + dropdown + reload → Tasks 13+14+15+16; verification → Task 17.
- ✅ No placeholders, all code blocks complete (the only narrative note is the eframe Storage API caveat in Task 16, which is a real uncertainty to be resolved at implement-time, not a missing instruction).
- ✅ Type names consistent across tasks: `World.display_name_refs: HashMap<EntityId, String>`; `Universe.translations: HashMap<(u32,u32), String>` + `available_locales: Vec<u32>` + `current_locale: u32`; `EntityRecord.display_name_ref: Option<String>`; `colors::replace_translation_refs(s, &HashMap) -> String`; `colors::resolve_entity_label(&World, &Universe, EntityId) -> String`; `colors::resolve_entity_label_without_code(&World, &Universe, EntityId) -> String`; `game_path::list_available_locales(&Path) -> Vec<u32>`; `game_path::locale_display_name(u32) -> &'static str`; `settings::AppSettings { locale: u32 }`; `TopBarResponse.locale_changed_to: Option<u32>`.
- ✅ Spec out-of-scope items are absent from the plan (no macro-file parsing, no per-locale ware caching, no partial reload).
- ✅ Task 7 and Task 8 carry an explicit dispatch-order note (Task 8 before Task 7) to avoid a transient compile break.
