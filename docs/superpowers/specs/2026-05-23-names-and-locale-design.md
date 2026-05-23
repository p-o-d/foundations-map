# Entity Names + Locale Switching Design

**Date:** 2026-05-23
**Status:** Approved (pending implementation plan)
**Scope:** Fix ugly ship/station name display, support all locales the game ships, switch at runtime via top-bar dropdown.

## Goals

1. Replace the current `strip_macro` fallback with the proper X4 display name resolved from each entity's `name=` / `basename=` save attribute, so labels match what the game shows. User-renamed ships (literal name strings in the save) display their custom names.
2. Apply the resolved name everywhere a ship or station is identified: side-panel list rows, side-panel SELECTED block, and the 3D hover tooltip.
3. Add a top-bar locale dropdown listing every language the installed game ships. English (`l044`) remains the default. Choice persists across app restarts via eframe's storage.

## Why now

Phase 3 is otherwise complete. Live-data entity rendering is in place, but the labels read like `ship par l trans container 03 a` instead of `Argon Cerberus Vanguard`, so the panel and tooltip are still hard to read. Locale switching has been wanted since Phase 1 — `detect_locale()` already exists in `game_path.rs` but is never called, and the parser hard-codes `l044`.

## Save XML Schema

Verified against a real `quicksave.xml.gz`:

```xml
<component class="ship_l"
           macro="ship_par_l_trans_container_03_a_macro"
           name="{20101,122701}"
           code="AKV-484"
           owner="alliance" .../>

<component class="station"
           macro="station_gen_factory_base_01_macro"
           basename="{20102,1701}"
           code="FAR-140"
           owner="freesplit" .../>

<component class="station"
           macro="station_gen_factory_base_01_macro"
           name="{20103,2001}" basename="{20103,2001}"
           description="{20103,2002}" code="NLK-443" .../>

<component class="ship_xs"
           macro="ship_gen_xs_escapepod_01_a_macro"
           name="{20101,101101} ({20203,401})"
           code="PXP-294" .../>
```

Key facts:

- Ships carry a `name="{page,id}"` translation reference. Some are compound: `{p,t} ({p,t})` — both refs must be resolved.
- Stations carry `basename="{p,t}"`; some additionally carry `name="{p,t}"` (overrides basename for unique stations).
- A small set of player-renamed ships and lore-ships carry a literal `name="..."` string instead of a `{p,t}` ref. These must pass through unchanged.
- The macro name (`station_gen_factory_base_01_macro`) is shared across hundreds of stations and is **not** a useful display label — the per-instance `name=`/`basename=` is what the game shows.

## Translation Pages

Currently `parse_translations_xml` keeps only pages 20003, 20004, and 20201. The new requirement is to retain pages 20101, 20102, 20103, etc. — wherever ship and station class names live. The simplest correct rule is: retain **every** page, using parenthetical extraction for pages 20003 and 20004 (whose entries follow the `{ref} {ref}(Display Name)` convention) and plain text for all other pages.

Memory cost for English `l044`: roughly 150 000 entries, ~6 MB allocated for the `HashMap<(u32,u32), String>`. One-time load cost; acceptable.

## Locale Files

The shipped game directory contains the following 16 locale files under `t/`:

```
l007 Russian       l033 French        l034 Spanish (LA)   l039 Italian
l042 Czech         l044 English       l048 Polish         l049 German
l055 Portuguese    l081 Japanese      l082 Korean         l086 zh-CN
l088 zh-TW         l090 Turkish       l359 Bulgarian      l380 Ukrainian
```

The exact list comes from scanning the cat archive for `t/0001-l*.xml`. We surface it via `game_path::list_available_locales(game_dir) -> Vec<u32>` so the dropdown adapts to whatever the user's install actually has (DLCs and mods can extend it).

Native display names ("English", "Deutsch", "Русский", …) come from a static `locale_display_name(u32) -> &'static str` table — more recognisable to users browsing a locale picker than `l049`.

## Architecture

Three layers — same shape as recent feature work.

### Layer 1: Domain (`map-domain`)

`crates/map-domain/src/world.rs`:

```rust
pub struct World {
    // ... existing fields ...
    /// Raw `name=` / `basename=` value from the save. Either a `{page,id}` ref,
    /// a compound form `{p,t} ({p,t})`, or a literal string (player-renamed ships).
    /// Resolved at display time so it picks up the current locale.
    pub display_name_refs: HashMap<EntityId, String>,
}
```

`crates/map-domain/src/universe.rs`:

```rust
pub struct Universe {
    // ... existing fields ...
    pub translations: HashMap<(u32, u32), String>,
    pub available_locales: Vec<u32>,
    pub current_locale: u32,
}
```

### Layer 2: Parsing (`map-io`)

**Sector chunk** (`save_parser/sector_chunk.rs`):

- Add `display_name_ref: Option<String>` to `Pending` and `EntityRecord`.
- In `build_pending`, capture stations and ships:
  - For stations, prefer the `name=` attribute if present, else `basename=`.
  - For ships, take `name=`.
- Merge stage copies to `World.display_name_refs` when non-empty.

**Translation parser** (`xml_parser.rs::parse_translations_xml`):

- Remove the `Some(20003 | 20004 | 20201)` page guard. Retain every `<t>` element across all pages.
- Keep the page-21-style branch: for pages 20003/20004 use `extract_last_parenthetical`; for everything else use the trimmed plain text. This is a one-line generalisation of the existing 20201 branch.

**Galaxy loader** (`xml_parser.rs::parse_galaxy_from_game`):

- Takes a new parameter `locale: u32`. Replaces the hard-coded `"t/0001-l044.xml"` with `format!("t/0001-l{:03}.xml", locale)`.
- Populates `Universe.translations`, `Universe.available_locales` (via `game_path::list_available_locales`), and `Universe.current_locale`.

**Game path** (`game_path.rs`):

```rust
pub fn list_available_locales(game_dir: &Path) -> Vec<u32> { … }
pub fn locale_display_name(id: u32) -> &'static str { … }
```

`list_available_locales` scans `cat_reader::list_files_matching(game_dir, "t/", ".xml")` and parses each `0001-l<NNN>.xml` filename into a numeric ID, dedupes, sorts ascending.

### Layer 3: App / UI (`map-app`)

**New file** `crates/map-app/src/settings.rs`:

```rust
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AppSettings {
    pub locale: u32,
}

impl Default for AppSettings {
    fn default() -> Self { Self { locale: 44 } }
}

const STORAGE_KEY: &str = "foundations-map-settings";

pub fn load(storage: Option<&dyn eframe::Storage>) -> AppSettings { … }
pub fn save(storage: &mut dyn eframe::Storage, s: &AppSettings) { … }
```

Uses `eframe::get_value` / `eframe::set_value` (already a dependency).

**Label resolver** (`colors.rs`):

```rust
/// Substitute every `{page,id}` substring with its resolved translation, leaving
/// other text intact. Used for compound names like `{p,t} ({p,t})` and for
/// literal user-renamed ships (which contain no `{...}` and pass through unchanged).
pub fn replace_translation_refs(
    s: &str,
    translations: &HashMap<(u32, u32), String>,
) -> String { … }

/// Resolve a label for one live entity.
/// Returns `<class name> (<code>)` when both are known, just the class or just
/// the code otherwise, and ultimately falls back to the stripped macro.
pub fn resolve_entity_label(
    world: &World,
    universe: &Universe,
    eid: EntityId,
) -> String { … }
```

`replace_translation_refs` implementation: simple scan, no regex. Find each `{`, locate matching `}`, parse two comma-separated integers between them; if both parse and look up succeeds, replace; otherwise leave that `{…}` substring as-is.

**Side panel** (`sector_panel.rs`):

- `entity_row_label(world, universe, eid)` — now takes `&Universe` to access `translations`. Internally calls `resolve_entity_label` and pairs it with the existing kind-icon glyph. All four current callers updated.
- Remove the now-unused `strip_macro` import from this file (the helper itself stays in `colors.rs` because the resolver still falls back to it).

**3D hover tooltip** (`sector_view.rs::draw_hover_label`):

- Replace the existing code/human/faction lines block with:
  ```
  line 1: resolved name      (theme::TEXT_PRIMARY)
  line 2: code               (theme::TEXT_MUTED)         — only if code present
  line 3: faction name       (faction color)             — only if faction known
  ```
- The static-object branch (top of `draw_hover_label`) is unchanged.

**Top bar** (`top_bar.rs`):

- New `egui::ComboBox` for locale selection, placed next to the existing refresh button. Selected value comes from `app.settings.locale`. Items: `(id, display_name)` pairs from `universe.available_locales`.
- `TopBar::show` returns a new field `TopBarResponse.locale_changed_to: Option<u32>`.

**App reload** (`app.rs`):

- On startup, `App::new` loads settings from `cc.storage`, calls `parse_galaxy_from_game(game_dir, settings.locale)`.
- New method `App::reload_galaxy(&mut self, locale: u32)` re-runs `parse_galaxy_from_game(game_dir, locale)`, swaps `self.universe`, persists new locale via `eframe::App::save`, then fires `spawn_save_parse` with the fresh `sector_macros` / `faction_strings` so live data re-resolves against the new universe.
- Top-bar dropdown change handler calls `reload_galaxy` and updates `self.settings.locale`.

## Data Flow on Locale Change

1. User picks `Deutsch` from the dropdown.
2. `TopBarResponse.locale_changed_to = Some(49)` returned to `App::ui`.
3. `App::reload_galaxy(49)`:
   - Calls `parse_galaxy_from_game(game_dir, 49)` → new Universe with German strings.
   - Replaces `self.universe`.
   - Clears `self.snapshot` and sets `snapshot_loading = true`.
   - Spawns the save parser with the new universe's `sector_macros` / `faction_strings`.
   - Persists `settings.locale = 49` via eframe storage on next `save()` call.
4. Snapshot finishes loading; entity labels resolve via the new translations map and render in German.

Roughly two-second blocking reload — acceptable for a rare user action.

## Testing

**Unit tests:**

- `xml_parser::parse_translations_xml_retains_all_pages` — verify a fixture with pages 10000, 20003, 20101 all produce hits, with parenthetical for 20003 and plain text otherwise.
- `sector_chunk::parses_ship_name_and_station_basename` — fixture chunk with one ship `name=` and one station `basename=`; verify `display_name_ref` captured. Also covers a player-renamed ship literal.
- `merge::display_name_refs_propagated_to_world` — `EntityRecord` with the field set ⇒ `World.display_name_refs[id]` matches.
- `colors::replace_translation_refs` — three cases: single ref resolves, compound `{p,t} ({p,t})` both substitute, literal passes through unchanged. One negative: unknown key leaves the `{…}` untouched (so debugging is possible).
- `colors::resolve_entity_label` — entity with `display_name_ref + code` → `"Name (CODE)"`; with code only → `"CODE"`; with neither → `strip_macro(macro_name)` fallback.
- `game_path::list_available_locales` — uses cat fixture or skips (integration covers).
- `settings::roundtrip` — `Default::default() → save → load` returns equal struct (uses an in-memory `eframe::Storage` mock if available, else a tiny shim).

**Manual verification:**

- Launch app; default locale is English; switch to Deutsch — sector names, station names, ship names all change to German strings.
- Hover a ship in 3D: tooltip shows two lines (name top, code below) and faction.
- Find a player-renamed ship (or rename one in-game and reload save) — custom name appears verbatim, code on second line of tooltip.
- Locale choice persists: close + relaunch the app; the previously chosen language is still active.
- Locale dropdown lists only locale files actually present in the install (smoke-tested by deleting one `t/0001-lNNN.xml` from a fresh local copy — should disappear from the dropdown).

## Out of Scope

- Showing the original macro name as a debugging tooltip.
- Caching per-locale ware-name tables across switches (re-parsed each reload — fast).
- Macro-definition-file parsing (we sidestep it by reading the per-instance `name=` attribute from the save).
- Editing in-game ship/station names from the app.
- Auto-switching locale when Steam language changes during a session.
- Partial locale change without re-parsing the save (would be a future optimisation).

## Trade-offs Considered

| Alternative | Why rejected |
|---|---|
| Parse `macros/*.xml` for `<identification name="{p,t}"/>` instead of reading the per-instance `name=`/`basename=` | Requires building a macro-name → file-path index from `index/macros.xml` and parsing thousands of XML files. The save already carries the per-instance ref, which is what the game uses for display. Much simpler. |
| Hot-swap translations without re-parsing the save | The `Universe.translations` map is the only state that depends on locale, and the save parser uses the universe's `sector_macros` / `faction_strings` which are language-independent. Skipping the save reparse is technically possible but adds branching for marginal speedup. |
| Persist settings in our own TOML/JSON file | Eframe's `Storage` already abstracts the platform-correct location and survives upgrades. No reason to add a new file. |
