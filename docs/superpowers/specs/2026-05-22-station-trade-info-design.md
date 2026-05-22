# Station Trade Info in Side Panel

**Date:** 2026-05-22
**Status:** Approved (pending implementation plan)
**Scope:** Phase 3 polish — wrap up live-data work by surfacing per-station buy/sell offers in the side panel.

## Goal

When a station is selected in the 3D sector view, the side panel SELECTED block shows what the station buys and sells, including price (Cr) and current/desired amounts. This is the last missing piece of "live data visibility" before Phase 3 closes.

## Save XML Schema

Every station component in a save has a child `<trade>` wrapper. Its `<offers>/<production>` block lists one `<trade>` element per ware the station trades:

```xml
<component class="station" macro="..." id="[0x2ecb7]" owner="freesplit">
  <offset><position .../></offset>
  ...
  <trade>
    <reservations>
      <reservation id="..." buyer="..." partner="..." ware="..." .../>
    </reservations>
    <offers>
      <production>
        <trade id="[0x116d]" buyer="[0x2ecb7]" ware="dronecomponents"
               price="102800" amount="16" desired="16" flags="supplies|..."/>
        <trade id="[0x116f]" seller="[0x2ecb7]" ware="medicalsupplies"
               price="7174" amount="6299" flags="invertfactionrestriction"/>
      </production>
    </offers>
  </trade>
  ...
</component>
```

Key facts:

- `buyer="[stationId]"` ⇒ station BUYS that ware. `seller="[stationId]"` ⇒ station SELLS it.
- `price` is per-unit Cr (integer).
- `amount` is the current count (in stock for sell, queued for buy). Zero is valid and meaningful ("accepts this ware but currently 0").
- `desired` is the wanted/max amount; absent on some sell entries ⇒ treat as `0`.
- Offers are already aggregated per station — no need to walk individual building modules.
- `<reservation>` elements inside `<reservations>` look similar but are in-flight trade bookings, not offers; they must be skipped.

## Ware Names

Raw ware IDs (`energycells`, `medicalsupplies`) are functional but ugly. Game install ships `libraries/wares.xml` inside `08.cat` with one row per ware:

```xml
<ware id="energycells" name="{20201,1101}" .../>
```

The `name` attribute is the existing `{pageId,textId}` translation tuple already understood by `parse_translation_table`. Lookup yields strings like `"Energy Cells"`.

## Architecture

Three layers, same shape as existing live-data flow (`save_parser` → `World` → `sector_panel`).

### 1. Domain (`map-domain`)

Add to `crates/map-domain/src/world.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeDirection { Buy, Sell }

#[derive(Debug, Clone)]
pub struct TradeOffer {
    pub ware_id: String,        // e.g. "energycells"
    pub direction: TradeDirection,
    pub price: i64,             // per-unit Cr
    pub amount: i64,            // current stock / queued
    pub desired: i64,           // wanted / cap (0 when absent)
}
```

Extend `World`:

```rust
pub trade_offers: HashMap<EntityId, Vec<TradeOffer>>,
```

Add helper:

```rust
impl World {
    pub fn trade_offers_of(&self, id: EntityId) -> &[TradeOffer] {
        self.trade_offers.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }
}
```

Extend `Universe` (`crates/map-domain/src/universe.rs`):

```rust
pub ware_names: HashMap<String, String>,   // ware_id → display name
```

### 2. Parsing (`map-io`)

**Sector chunk extension** (`save_parser/sector_chunk.rs`):

- Add `trade_offers: Vec<TradeOffer>` to `Pending` and `EntityRecord`.
- Track `in_offers_depth: Option<u32>` (depth of the open `<offers>` element). Set on `Start b"offers"`, clear on `End b"offers"`.
- On `Start`/`Empty` of `<trade>`:
  - If `in_offers_depth.is_some()` AND the element has either `buyer=` or `seller=` attribute, parse it as an offer.
  - Direction = `Buy` if `buyer` present, `Sell` if `seller` present.
  - Attach to nearest enclosing station `Pending` (top of stack whose `kind == Station`).
  - Otherwise (top-level `<trade>` wrapper, in-flight trades, reservations) skip.
- On `End` of `<component>`, copy `Pending.trade_offers` into the emitted `EntityRecord`.

**Merge stage** (`save_parser/merge.rs`):

- After `insert_entity`, populate `world.trade_offers` from `r.trade_offers` when non-empty.

**Ware name parser** (`map-io/src/xml_parser.rs`):

```rust
pub fn parse_ware_names_xml(
    xml: &[u8],
    translations: &HashMap<(u32, u32), String>,
) -> HashMap<String, String>
```

- Streams `<ware id="..." name="..."/>` with `quick_xml`.
- For each entry, if `name` matches `{page,id}` ⇒ resolve via `translations`. If literal ⇒ use as-is. If unresolved ⇒ fall back to the raw ID.

**Load wiring** (`map-io/src/lib.rs`):

- After `parse_translation_table` produces the translations map, read `libraries/wares.xml` via `cat_reader::read_file_first_match`, call `parse_ware_names_xml`, and store on `Universe.ware_names`.
- Failure to find or parse wares.xml is non-fatal: leave `ware_names` empty; UI falls back to raw IDs.

### 3. UI (`map-app/src/ui/sector_panel.rs`)

In the SELECTED block, after position/faction lines, if `world.kinds.get(eid) == Some(Station)` and `world.trade_offers_of(eid)` is non-empty:

```
TRADE
  BUYS (n)
    Energy Cells       1,092 Cr    0 / 1200
    Spices             2,800 Cr   4672 / 4672
  SELLS (n)
    Medical Supplies   7,174 Cr   6299 / 6299
    Drone Components 102,800 Cr     16 /   16
```

Rendering rules:

- Two sub-headers using `egui::CollapsingHeader` (default open) for `BUYS (n)` and `SELLS (n)`.
- Within each, render rows via `egui::Grid` with 3 columns: ware name | price right-aligned | `amount / desired` right-aligned.
- Sort alphabetically by display name within each section.
- Show all offers, including `amount == 0` rows (signals "accepts but currently empty/full").
- Display name: `universe.ware_names.get(&offer.ware_id).cloned().unwrap_or(offer.ware_id.clone())`.
- Format numbers with `,` thousands separators (price + amounts).

## Trade-offs Considered

| Alternative | Why rejected |
|---|---|
| Per-module rows (one section per production module) | Big stations have 20+ modules — too much clutter and the save already aggregates offers onto the parent station. |
| Raw ware IDs (skip wares.xml parsing) | Saves a few seconds of work but every panel view becomes hard to read; one-time parse cost is negligible. |
| Hide `amount == 0` rows | Loses "this station accepts X but is full / empty" info, which matters for trade planning. |
| Store offers on `StaticObject` instead of `World` | Trade data is live and changes per save; static objects are loaded once from game files. Wrong lifecycle. |

## Testing

**Unit tests:**

- `sector_chunk::tests::parses_station_trade_offers` — station with two buy + one sell offer, one reservation; returns exactly the three offers attached to the station, ignores reservation.
- `sector_chunk::tests::nested_reservation_trade_not_treated_as_offer` — `<reservations><reservation .../></reservations>` outside `<offers>` produces no `TradeOffer`.
- `sector_chunk::tests::ship_inside_station_inherits_no_offers` — docked drone with no `<offers>` block gets empty `trade_offers`.
- `merge::tests::trade_offers_propagated_to_world` — `EntityRecord` with offers ⇒ `World.trade_offers[id]` matches.
- `xml_parser::tests::parse_ware_names_resolves_translation_tuples` — `<ware id="x" name="{20201,1101}"/>` with translation table maps to display string.
- `xml_parser::tests::parse_ware_names_handles_literal_name` — literal `name="Energy Cells"` returned as-is.

**Manual verification:**

- `cargo run`, load real save (`save_002.xml.gz`), navigate to a known production station (e.g. Holy Vision shipyard), open 3D view, click station, confirm TRADE section shows expected wares with sensible prices/amounts.
- Pick a station with `<reservation>` entries, confirm those don't appear in the offers list.

## Conventions

- TDD per existing repo norms.
- New types public on `map_domain` so renderer can read them.
- Conventional Commits (`feat(panel): show station trade offers`, etc.) with `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>` trailer.

## Out of Scope

- Ware filtering, search, sorting by price/amount, or column-header toggles.
- Trade-route or supply-chain visualisation between stations.
- Per-module breakdown.
- Storage capacity/free space (separate from per-ware `desired`).
- Showing in-flight `<trade>` reservations between stations.
- Updating trade data live without reloading the save snapshot.
