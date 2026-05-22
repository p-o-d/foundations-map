# Station Trade Info Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show each station's buy/sell trade offers (ware, direction, price, current/desired amounts) in the side panel when a station is selected in the 3D sector view.

**Architecture:** Three layers. (1) New `TradeOffer` + `TradeDirection` types on `map-domain`, stored on `World.trade_offers` and `Universe.ware_names`. (2) Save XML parsing extended in `save_parser/sector_chunk.rs` to read `<trade>/<offers>/<production>/<trade>` per station; merge stage propagates into `World`. (3) `parse_ware_names_xml` reads `libraries/wares.xml` from the cat archive at load, populates `Universe.ware_names`. Side panel renders a `TRADE` section in the SELECTED block for stations.

**Tech Stack:** Rust 2024, `quick_xml` 0.x (already in tree), `egui` 0.34.2, `glam`.

**Spec:** `docs/superpowers/specs/2026-05-22-station-trade-info-design.md`

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/map-domain/src/world.rs` | `TradeOffer`, `TradeDirection`, `World.trade_offers` + `trade_offers_of()` helper |
| `crates/map-domain/src/universe.rs` | `Universe.ware_names: HashMap<String,String>` |
| `crates/map-io/src/save_parser/types.rs` | `EntityRecord.trade_offers: Vec<TradeOffer>` |
| `crates/map-io/src/save_parser/sector_chunk.rs` | Parse `<offers>` block; attach to station `Pending` |
| `crates/map-io/src/save_parser/merge.rs` | Copy offers into `World.trade_offers` |
| `crates/map-io/src/xml_parser.rs` | (a) extend `parse_translations_xml` to retain page 20201; (b) new `parse_ware_names_xml` |
| `crates/map-io/src/xml_parser.rs` (caller) | Load `libraries/wares.xml` in `parse_galaxy_from_game`, populate `Universe.ware_names` |
| `crates/map-app/src/ui/sector_panel.rs` | Render TRADE section in SELECTED block |
| `crates/map-io/tests/fixtures/wares_mini.xml` | New fixture for ware-name parser test |

---

## Task 1: Domain types for trade offers

**Files:**
- Modify: `crates/map-domain/src/world.rs`

- [ ] **Step 1: Write the failing test** at the bottom of the existing `tests` mod in `crates/map-domain/src/world.rs`:

```rust
#[test]
fn trade_offers_can_be_inserted_and_looked_up() {
    use crate::world::{TradeDirection, TradeOffer};
    let mut w = World::new();
    w.trade_offers.insert(
        42,
        vec![
            TradeOffer {
                ware_id: "energycells".into(),
                direction: TradeDirection::Buy,
                price: 1092,
                amount: 0,
                desired: 1200,
            },
            TradeOffer {
                ware_id: "medicalsupplies".into(),
                direction: TradeDirection::Sell,
                price: 7174,
                amount: 6299,
                desired: 6299,
            },
        ],
    );
    let offers = w.trade_offers_of(42);
    assert_eq!(offers.len(), 2);
    assert_eq!(offers[0].direction, TradeDirection::Buy);
    assert_eq!(offers[1].ware_id, "medicalsupplies");
    assert!(w.trade_offers_of(999).is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p map-domain trade_offers_can_be_inserted_and_looked_up`
Expected: FAIL — unresolved imports `TradeDirection`/`TradeOffer`, missing field `trade_offers`, missing method `trade_offers_of`.

- [ ] **Step 3: Implement the types and field**

Append to `crates/map-domain/src/world.rs` (above the `#[cfg(test)] mod tests` block):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeDirection {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeOffer {
    pub ware_id: String,
    pub direction: TradeDirection,
    pub price: i64,
    pub amount: i64,
    pub desired: i64,
}
```

Add field to the `World` struct (inside the existing `pub struct World { ... }`):

```rust
    pub trade_offers: HashMap<EntityId, Vec<TradeOffer>>,
```

Add helper inside `impl World { ... }`:

```rust
    pub fn trade_offers_of(&self, id: EntityId) -> &[TradeOffer] {
        self.trade_offers.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }
```

- [ ] **Step 4: Run all map-domain tests to verify they pass**

Run: `cargo test -p map-domain`
Expected: PASS, including the new test.

- [ ] **Step 5: Commit**

```bash
git add crates/map-domain/src/world.rs
git commit -m "$(cat <<'EOF'
feat(domain): TradeOffer/TradeDirection + World.trade_offers

Per-entity buy/sell offer storage for upcoming station trade panel.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Universe.ware_names field

**Files:**
- Modify: `crates/map-domain/src/universe.rs`

- [ ] **Step 1: Write the failing test** at the bottom of the existing `tests` mod in `crates/map-domain/src/universe.rs`:

```rust
#[test]
fn ware_names_can_be_populated() {
    let mut u = Universe::default();
    u.ware_names.insert("energycells".into(), "Energy Cells".into());
    assert_eq!(u.ware_names.get("energycells").map(String::as_str), Some("Energy Cells"));
    assert!(u.ware_names.get("missing").is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p map-domain ware_names_can_be_populated`
Expected: FAIL — no field `ware_names` on `Universe`.

- [ ] **Step 3: Add the field**

Inside `pub struct Universe { ... }` in `crates/map-domain/src/universe.rs`, add:

```rust
    /// Lowercase ware id (e.g. "energycells") → display name (e.g. "Energy Cells").
    /// Built once at galaxy load from `libraries/wares.xml`. Empty if parse failed.
    pub ware_names: std::collections::HashMap<String, String>,
```

`#[derive(Default)]` already on `Universe`, so no other change required.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p map-domain`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/map-domain/src/universe.rs
git commit -m "$(cat <<'EOF'
feat(domain): Universe.ware_names lookup table

Storage for ware-id → display-name mapping, populated from
libraries/wares.xml at galaxy load time.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Add trade_offers to EntityRecord

**Files:**
- Modify: `crates/map-io/src/save_parser/types.rs`

- [ ] **Step 1: Update the existing `entity_record_constructs` test** in `crates/map-io/src/save_parser/types.rs` to construct the new field. Replace the existing test block with:

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
        };
        assert_eq!(e.id, 0x100);
        assert_eq!(e.parent_id, None);
        assert_eq!(e.code.as_deref(), Some("YIB-942"));
        assert_eq!(e.owner.as_deref(), Some("argon"));
        assert!(e.trade_offers.is_empty());
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p map-io entity_record_constructs`
Expected: FAIL — missing field `trade_offers`.

- [ ] **Step 3: Add the field**

In `crates/map-io/src/save_parser/types.rs`, add to the top imports block:

```rust
use map_domain::world::{LiveObjectKind, TradeOffer};
```

(Replace the existing `use map_domain::world::LiveObjectKind;` line.)

Add a new field at the end of `pub struct EntityRecord { ... }`:

```rust
    pub trade_offers: Vec<TradeOffer>,
```

- [ ] **Step 4: Update existing sites that construct `EntityRecord`** so the workspace still compiles.

In `crates/map-io/src/save_parser/sector_chunk.rs`, the `EntityRecord { ... }` literal inside `parse_sector_chunk` (currently lines ~41–50) becomes:

```rust
                        out.push(EntityRecord {
                            id: p.id,
                            parent_id: p.parent_id,
                            macro_name: p.macro_name,
                            code: p.code,
                            kind: p.kind,
                            owner: p.owner,
                            position: p.position.unwrap_or(Vec3::ZERO),
                            sector_macro: sector_macro.to_string(),
                            trade_offers: std::mem::take(&mut p.trade_offers),
                        });
```

Add `trade_offers: Vec<TradeOffer>` to `struct Pending`:

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
    trade_offers: Vec<TradeOffer>,
}
```

Add the missing import at the top of `sector_chunk.rs` (alongside `use map_domain::world::LiveObjectKind;`):

```rust
use map_domain::world::{LiveObjectKind, TradeOffer};
```

(Replace the single existing import.)

Initialize `trade_offers: Vec::new()` in `build_pending`:

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
    })
```

Update the `merge::tests` literal constructors in `crates/map-io/src/save_parser/merge.rs` to add `trade_offers: vec![]` to each `EntityRecord { ... }`. There are three such literals (in `merges_records_and_assigns_faction_ids`, `unknown_sector_drops_entity`, `no_sector_macros_drops_all`).

Update the existing `sector_chunk::tests` assertions if needed — every existing test only inspects published fields and should still pass once `trade_offers: Vec::new()` is created by `build_pending`. No assertion changes required for current tests.

- [ ] **Step 5: Run all map-io tests to verify they pass**

Run: `cargo test -p map-io`
Expected: PASS, including the modified `entity_record_constructs`.

- [ ] **Step 6: Commit**

```bash
git add crates/map-io/src/save_parser/types.rs crates/map-io/src/save_parser/sector_chunk.rs crates/map-io/src/save_parser/merge.rs
git commit -m "$(cat <<'EOF'
feat(save_parser): trade_offers field on EntityRecord + Pending

Plumbing only — parser still emits empty vec. Offers populated in
subsequent commit.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Parse `<offers>` block in sector_chunk

**Files:**
- Modify: `crates/map-io/src/save_parser/sector_chunk.rs`

- [ ] **Step 1: Write the failing test** — append three tests to the existing `tests` module in `crates/map-io/src/save_parser/sector_chunk.rs`:

```rust
    #[test]
    fn parses_station_trade_offers() {
        use map_domain::world::TradeDirection;
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" macro="st" owner="argon" id="[0x100]">
    <offset><position x="0" y="0" z="0"/></offset>
    <trade>
      <reservations>
        <reservation id="[0x900]" buyer="[0x999]" partner="[0x100]" ware="ignored" price="1" amount="1" desired="1"/>
      </reservations>
      <offers>
        <production>
          <trade id="[0x1]" buyer="[0x100]" ware="energycells" price="1092" amount="0"/>
          <trade id="[0x2]" buyer="[0x100]" ware="spices" price="2800" amount="4672" desired="4672"/>
          <trade id="[0x3]" seller="[0x100]" ware="medicalsupplies" price="7174" amount="6299"/>
        </production>
      </offers>
    </trade>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let station = out.iter().find(|r| r.id == 0x100).expect("station present");
        assert_eq!(station.trade_offers.len(), 3, "expected 3 offers, got {:?}", station.trade_offers);

        let ec = station.trade_offers.iter().find(|o| o.ware_id == "energycells").unwrap();
        assert_eq!(ec.direction, TradeDirection::Buy);
        assert_eq!(ec.price, 1092);
        assert_eq!(ec.amount, 0);
        assert_eq!(ec.desired, 0, "desired absent ⇒ 0");

        let sp = station.trade_offers.iter().find(|o| o.ware_id == "spices").unwrap();
        assert_eq!(sp.direction, TradeDirection::Buy);
        assert_eq!(sp.desired, 4672);

        let med = station.trade_offers.iter().find(|o| o.ware_id == "medicalsupplies").unwrap();
        assert_eq!(med.direction, TradeDirection::Sell);
        assert_eq!(med.amount, 6299);
    }

    #[test]
    fn reservation_outside_offers_is_not_a_trade_offer() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" macro="st" owner="argon" id="[0x100]">
    <offset><position x="0" y="0" z="0"/></offset>
    <trade>
      <reservations>
        <reservation id="[0x900]" buyer="[0x999]" partner="[0x100]" ware="ignored" price="1" amount="1" desired="1"/>
      </reservations>
    </trade>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let station = out.iter().find(|r| r.id == 0x100).unwrap();
        assert!(station.trade_offers.is_empty());
    }

    #[test]
    fn docked_ship_inherits_no_offers() {
        let chunk: &[u8] = br#"<component class="sector" macro="m">
  <component class="station" macro="st" owner="argon" id="[0x100]">
    <offset><position x="0" y="0" z="0"/></offset>
    <trade><offers><production>
      <trade id="[0x1]" buyer="[0x100]" ware="energycells" price="50" amount="0"/>
    </production></offers></trade>
    <connections>
      <component class="ship_xs" macro="drone" owner="argon" id="[0x200]">
        <offset><position x="0" y="0" z="0"/></offset>
      </component>
    </connections>
  </component>
</component>"#;
        let out = parse_sector_chunk(chunk, "m");
        let drone = out.iter().find(|r| r.id == 0x200).unwrap();
        assert!(drone.trade_offers.is_empty());
        let station = out.iter().find(|r| r.id == 0x100).unwrap();
        assert_eq!(station.trade_offers.len(), 1);
    }
```

- [ ] **Step 2: Run the new tests to verify they fail**

Run: `cargo test -p map-io parses_station_trade_offers reservation_outside_offers_is_not_a_trade_offer docked_ship_inherits_no_offers`
Expected: FAIL — `trade_offers` is always empty.

- [ ] **Step 3: Implement `<offers>` parsing in `parse_sector_chunk`**

Edit `crates/map-io/src/save_parser/sector_chunk.rs`. Replace the existing event loop (lines 17–82 in the current file) with this version that tracks an `in_offers_depth: Option<u32>`:

```rust
pub fn parse_sector_chunk(slice: &[u8], sector_macro: &str) -> Vec<EntityRecord> {
    let mut reader = Reader::from_reader(slice);
    reader.config_mut().trim_text(true);

    let mut out: Vec<EntityRecord> = Vec::new();
    let mut buf: Vec<u8> = Vec::new();

    let mut comp_depth: u32 = 0;
    let mut stack: Vec<Pending> = Vec::new();
    let mut offset_depth: Option<u32> = None;
    // Depth at which we are currently inside an `<offers>` element; None otherwise.
    // `<trade>` elements inside this scope (and with a buyer= or seller= attribute)
    // are offers, not in-flight trades.
    let mut in_offers_depth: Option<u32> = None;

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
                        let mut p = stack.pop().unwrap();
                        out.push(EntityRecord {
                            id: p.id,
                            parent_id: p.parent_id,
                            macro_name: p.macro_name,
                            code: p.code,
                            kind: p.kind,
                            owner: p.owner,
                            position: p.position.unwrap_or(Vec3::ZERO),
                            sector_macro: sector_macro.to_string(),
                            trade_offers: std::mem::take(&mut p.trade_offers),
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
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"offers" => {
                in_offers_depth = Some(comp_depth);
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"offers" => {
                in_offers_depth = None;
            }
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"position" => {
                if let (Some(top), Some(od)) = (stack.last_mut(), offset_depth) {
                    if top.open_depth == od && top.position.is_none() {
                        let x = attr_f32(e, b"x").unwrap_or(0.0);
                        let y = attr_f32(e, b"y").unwrap_or(0.0);
                        let z = attr_f32(e, b"z").unwrap_or(0.0);
                        top.position = Some(Vec3::new(x / 1000.0, y / 1000.0, z / 1000.0));
                    }
                }
            }
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e))
                if e.name().as_ref() == b"trade" && in_offers_depth.is_some() =>
            {
                if let Some(offer) = build_offer(e) {
                    if let Some(top) = stack.last_mut() {
                        top.trade_offers.push(offer);
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
```

Add the `build_offer` helper at the bottom of the file (above the `#[cfg(test)] mod tests`):

```rust
fn build_offer(e: &BytesStart<'_>) -> Option<TradeOffer> {
    use map_domain::world::TradeDirection;
    let direction = if attr_str(e, b"buyer").is_some() {
        TradeDirection::Buy
    } else if attr_str(e, b"seller").is_some() {
        TradeDirection::Sell
    } else {
        return None;
    };
    let ware_id = attr_str(e, b"ware")?;
    let price = attr_i64(e, b"price").unwrap_or(0);
    let amount = attr_i64(e, b"amount").unwrap_or(0);
    let desired = attr_i64(e, b"desired").unwrap_or(0);
    Some(TradeOffer {
        ware_id,
        direction,
        price,
        amount,
        desired,
    })
}

fn attr_i64(e: &BytesStart<'_>, name: &[u8]) -> Option<i64> {
    e.attributes()
        .filter_map(Result::ok)
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| std::str::from_utf8(&a.value).ok()?.parse::<i64>().ok())
}
```

- [ ] **Step 4: Run the new tests + existing chunk tests**

Run: `cargo test -p map-io sector_chunk`
Expected: PASS for all sector_chunk tests including the three new ones. If `parses_station_and_ship_with_positions`, `nested_ship_inside_station_emits_two_records_with_parent_link`, etc. now fail, re-check that `trade_offers: Vec::new()` is set in `build_pending`.

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/save_parser/sector_chunk.rs
git commit -m "$(cat <<'EOF'
feat(save_parser): parse station trade offers from <offers> block

Tracks <offers> scope to disambiguate offer <trade> elements from
in-flight <trade> records elsewhere in the save. Skips <reservation>.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Propagate trade offers in merge stage

**Files:**
- Modify: `crates/map-io/src/save_parser/merge.rs`

- [ ] **Step 1: Write the failing test** — append to the existing `tests` mod in `crates/map-io/src/save_parser/merge.rs`:

```rust
    #[test]
    fn trade_offers_propagated_to_world() {
        use map_domain::world::{TradeDirection, TradeOffer};
        let records = vec![EntityRecord {
            id: 0x10,
            parent_id: None,
            macro_name: "station_a".into(),
            code: None,
            kind: LiveObjectKind::Station,
            owner: Some("argon".into()),
            position: glam::Vec3::ZERO,
            sector_macro: "sa".into(),
            trade_offers: vec![TradeOffer {
                ware_id: "energycells".into(),
                direction: TradeDirection::Buy,
                price: 1092,
                amount: 0,
                desired: 1200,
            }],
        }];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut next = 1u32;
        let world = merge(vec![records], Some(&sm), &mut fs, &mut next);
        let offers = world.trade_offers_of(0x10);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].ware_id, "energycells");
        assert_eq!(offers[0].direction, TradeDirection::Buy);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p map-io trade_offers_propagated_to_world`
Expected: FAIL — `world.trade_offers_of(0x10)` returns empty.

- [ ] **Step 3: Implement propagation**

In `crates/map-io/src/save_parser/merge.rs`, inside the `for r in batch { ... }` loop, after the existing `world.insert_entity(...)` call, add:

```rust
            if !r.trade_offers.is_empty() {
                world.trade_offers.insert(r.id, r.trade_offers);
            }
```

(The `insert_entity` call consumes `r.macro_name`, `r.kind`, `r.code`. `r.trade_offers` is still owned because `r` is a fresh binding from the iterator — Rust will allow a moved-out field as long as no later code reads `r`. If borrow-checker complains because `r` is moved by `insert_entity`'s previous-field consumption, restructure as: move `r.trade_offers` out into a local variable before the `insert_entity` call.)

Concrete safe edit — replace the existing loop body in `merge()`:

```rust
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
            let entity_id = r.id;
            let trade_offers = r.trade_offers;
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
        }
```

- [ ] **Step 4: Run all merge tests**

Run: `cargo test -p map-io merge`
Expected: PASS — new test green, existing tests still green.

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/save_parser/merge.rs
git commit -m "$(cat <<'EOF'
feat(save_parser): copy EntityRecord.trade_offers into World

Side panel reads World.trade_offers_of(eid) for trade rendering.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Extend translation parser to retain wares page (20201)

**Files:**
- Modify: `crates/map-io/src/xml_parser.rs`

- [ ] **Step 1: Write the failing test** — append to the existing `#[cfg(test)] mod tests` in `crates/map-io/src/xml_parser.rs` (or add the module if none exists; check first with grep):

```rust
    #[test]
    fn parse_translations_xml_includes_wares_page_20201() {
        let xml = r#"<?xml version="1.0"?>
<language id="44">
  <page id="20201">
    <t id="1101">Energy Cells</t>
    <t id="1102">Medical Supplies</t>
  </page>
</language>"#;
        let map = super::parse_translations_xml(xml).unwrap();
        assert_eq!(map.get(&(20201, 1101)).map(String::as_str), Some("Energy Cells"));
        assert_eq!(map.get(&(20201, 1102)).map(String::as_str), Some("Medical Supplies"));
    }
```

> Note: `parse_translations_xml` is currently private. If the test is in the same file's `tests` mod it can access `super::parse_translations_xml`. If no `tests` mod exists in this file, create one at the bottom:
>
> ```rust
> #[cfg(test)]
> mod tests {
>     // (the test above goes here)
> }
> ```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p map-io parse_translations_xml_includes_wares_page_20201`
Expected: FAIL — page 20201 currently discarded; assertion returns `None`.

- [ ] **Step 3: Extend the page filter**

In `crates/map-io/src/xml_parser.rs`, locate this line in `parse_translations_xml`:

```rust
                    if matches!(current_page, Some(20003 | 20004)) {
```

Change it to:

```rust
                    if matches!(current_page, Some(20003 | 20004 | 20201)) {
```

The existing `Event::Text` arm uses `extract_last_parenthetical(&content)` which falls back nicely only when parentheses exist. Wares-page entries are plain text (e.g. `Energy Cells`) with no parenthetical. Update the `Event::Text` arm so a missing parenthetical falls back to the raw text:

```rust
            Event::Text(e) => {
                if let (Some(page_id), Some(text_id)) = (current_page, current_text_id) {
                    let decoded = e.decode().unwrap_or_default();
                    let content =
                        quick_xml::escape::unescape(&decoded).unwrap_or_else(|_| decoded.clone());
                    let name = extract_last_parenthetical(&content)
                        .unwrap_or_else(|| content.trim().to_string());
                    if !name.is_empty() {
                        translations.insert((page_id, text_id), name);
                    }
                    current_text_id = None;
                }
            }
```

> The existing sector-name translation behaviour is preserved because `extract_last_parenthetical` still returns the parenthetical for pages 20003/20004 entries that have them.

- [ ] **Step 4: Run translation-related tests**

Run: `cargo test -p map-io xml_parser`
Expected: PASS — new test green, existing tests still green.

Also run the higher-level integration tests to be safe:

Run: `cargo test -p map-io`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/xml_parser.rs
git commit -m "$(cat <<'EOF'
feat(xml_parser): retain translation page 20201 (wares)

Wares-page entries have no parenthetical, so fall back to raw text
when extract_last_parenthetical returns None. Sector/cluster pages
(20003/20004) keep their existing parenthetical-extraction behaviour.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: parse_ware_names_xml + fixture

**Files:**
- Create: `crates/map-io/tests/fixtures/wares_mini.xml`
- Modify: `crates/map-io/src/xml_parser.rs`

- [ ] **Step 1: Create the fixture**

Write file `crates/map-io/tests/fixtures/wares_mini.xml`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<wares>
  <ware id="energycells" name="{20201,1101}" tags="economy"/>
  <ware id="medicalsupplies" name="{20201,1102}" tags="economy"/>
  <ware id="literalwell" name="Hand-Written Name" tags="misc"/>
  <ware id="unknownware" name="{20201,9999}" tags="economy"/>
  <missingid name="{20201,1101}"/>
</wares>
```

- [ ] **Step 2: Write the failing test** — append to the `#[cfg(test)] mod tests` block in `crates/map-io/src/xml_parser.rs`:

```rust
    #[test]
    fn parse_ware_names_resolves_translations_literals_and_falls_back() {
        use std::collections::HashMap;
        let xml = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/wares_mini.xml"),
        )
        .unwrap();
        let mut translations: HashMap<(u32, u32), String> = HashMap::new();
        translations.insert((20201, 1101), "Energy Cells".into());
        translations.insert((20201, 1102), "Medical Supplies".into());

        let names = super::parse_ware_names_xml(&xml, &translations);

        assert_eq!(names.get("energycells").map(String::as_str), Some("Energy Cells"));
        assert_eq!(names.get("medicalsupplies").map(String::as_str), Some("Medical Supplies"));
        assert_eq!(names.get("literalwell").map(String::as_str), Some("Hand-Written Name"));
        // Unknown translation key falls back to raw ware id.
        assert_eq!(names.get("unknownware").map(String::as_str), Some("unknownware"));
        // <missingid> has no `id` attribute — skipped.
        assert!(names.get("missingid").is_none());
    }
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p map-io parse_ware_names_resolves_translations_literals_and_falls_back`
Expected: FAIL — `parse_ware_names_xml` does not exist.

- [ ] **Step 4: Implement `parse_ware_names_xml`**

Append to `crates/map-io/src/xml_parser.rs` (above the `#[cfg(test)] mod tests` if there is one, otherwise just append):

```rust
/// Parse `libraries/wares.xml`. Returns `ware_id (lowercase) → display name`.
///
/// `name="{page,id}"` resolved via `translations`; literal names returned as-is;
/// unknown translation keys fall back to the raw ware id so the UI is never empty.
pub fn parse_ware_names_xml(
    xml: &[u8],
    translations: &HashMap<(u32, u32), String>,
) -> HashMap<String, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut out: HashMap<String, String> = HashMap::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e))
                if e.name().as_ref() == b"ware" =>
            {
                let Some(id) = attr_value(e, b"id") else {
                    continue;
                };
                let id_lc = id.to_lowercase();
                let name_attr = attr_value(e, b"name").unwrap_or_default();
                let display = if let Some((pid, tid)) = parse_page_text_ref(&name_attr) {
                    translations
                        .get(&(pid, tid))
                        .cloned()
                        .unwrap_or_else(|| id_lc.clone())
                } else if !name_attr.is_empty() {
                    name_attr
                } else {
                    id_lc.clone()
                };
                out.insert(id_lc, display);
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

> `parse_page_text_ref` already exists in this file (private helper used by `parse_sector_name_refs_xml`). It returns `Option<(u32, u32)>` from `{page,id}` strings.

- [ ] **Step 5: Run the test to verify pass**

Run: `cargo test -p map-io parse_ware_names_resolves_translations_literals_and_falls_back`
Expected: PASS.

Also run the full crate suite:

Run: `cargo test -p map-io`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/map-io/tests/fixtures/wares_mini.xml crates/map-io/src/xml_parser.rs
git commit -m "$(cat <<'EOF'
feat(xml_parser): parse_ware_names_xml + fixture

Reads libraries/wares.xml entries, resolves {page,id} via the existing
translation table, falls back to literal name or raw ware id.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Wire wares.xml into `parse_galaxy_from_game`

**Files:**
- Modify: `crates/map-io/src/xml_parser.rs` (function `parse_galaxy_from_game`)

- [ ] **Step 1: Locate the end of `parse_galaxy_from_game`** where the `Universe { ... }` is constructed and returned. Find the line `let translations = parse_translations_xml(&translations_str)?;` (~line 100 in current file). This already gives us a `translations` map that — after Task 6 — includes page 20201.

- [ ] **Step 2: Read wares.xml from the cat archive and parse it**

Add the following block immediately after the `translations` is built, but before `cluster_to_sectors` is constructed (so `ware_names` is available when the `Universe` struct is filled in):

```rust
    // Ware id → display name (e.g. "energycells" → "Energy Cells"). Non-fatal:
    // missing or unparseable wares.xml leaves `ware_names` empty, UI falls back
    // to raw ids.
    let ware_names = crate::cat_reader::read_game_file(game_dir, "libraries/wares.xml")
        .map(|data| parse_ware_names_xml(&data, &translations))
        .unwrap_or_default();
    eprintln!("[map] Ware names: {}", ware_names.len());
```

- [ ] **Step 3: Set the field on the returned `Universe`**

Find the final `Universe { ... }` constructor at the end of `parse_galaxy_from_game`. Add `ware_names,` to the struct literal. The compiler will tell you the exact location if you forget — the field is on the struct from Task 2.

If the function does not build a `Universe { ... }` literal but instead mutates a `Universe::default()`, set the field on that instance: `universe.ware_names = ware_names;`.

> **Verification (no test for the wiring itself — covered manually by Task 10):** ensure the workspace compiles with `cargo build -p map-io`.

- [ ] **Step 4: Compile**

Run: `cargo build -p map-io`
Expected: success, no warnings about unused `ware_names`.

Run: `cargo test -p map-io`
Expected: PASS — existing integration tests should still pass (they don't read ware_names).

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/xml_parser.rs
git commit -m "$(cat <<'EOF'
feat(map_io): load libraries/wares.xml into Universe.ware_names

Non-fatal: missing or unparseable wares.xml leaves ware_names empty
and the panel falls back to raw ware ids.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Render TRADE section in side panel

**Files:**
- Modify: `crates/map-app/src/ui/sector_panel.rs`

> No test — egui rendering verified manually in Task 10. Egui paint functions don't have unit-test coverage in this repo.

- [ ] **Step 1: Add a helper to format integers with thousands separators**

At the bottom of `crates/map-app/src/ui/sector_panel.rs` (before `#[cfg(test)] mod tests`), add:

```rust
fn fmt_thousands(n: i64) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let abs = n.unsigned_abs().to_string();
    let bytes = abs.as_bytes();
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i != 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    format!("{sign}{out}")
}
```

- [ ] **Step 2: Render the TRADE section in the SELECTED block**

Locate the SELECTED block for live entities in `show()`. It is the `if let Some(eid) = *selected_entity { ... }` arm. After the existing `position` line (`ui.colored_label(... "Pos: ...")`) and before the `let kids = world.children_of(eid);` block, insert:

```rust
                            if matches!(
                                world.kinds.get(&eid),
                                Some(map_domain::world::LiveObjectKind::Station)
                            ) {
                                let offers = world.trade_offers_of(eid);
                                if !offers.is_empty() {
                                    ui.add_space(6.0);
                                    ui.colored_label(theme::TEXT_MUTED, "TRADE");
                                    render_trade_section(ui, offers, universe);
                                }
                            }
```

Add the `render_trade_section` helper next to `entity_row_label` at the bottom of the file:

```rust
fn render_trade_section(
    ui: &mut egui::Ui,
    offers: &[map_domain::world::TradeOffer],
    universe: &map_domain::universe::Universe,
) {
    use map_domain::world::TradeDirection;

    let mut buys: Vec<&map_domain::world::TradeOffer> = offers
        .iter()
        .filter(|o| o.direction == TradeDirection::Buy)
        .collect();
    let mut sells: Vec<&map_domain::world::TradeOffer> = offers
        .iter()
        .filter(|o| o.direction == TradeDirection::Sell)
        .collect();
    let name_for = |o: &map_domain::world::TradeOffer| -> String {
        universe
            .ware_names
            .get(&o.ware_id)
            .cloned()
            .unwrap_or_else(|| o.ware_id.clone())
    };
    buys.sort_by(|a, b| name_for(a).cmp(&name_for(b)));
    sells.sort_by(|a, b| name_for(a).cmp(&name_for(b)));

    render_offer_group(ui, "BUYS", &buys, &name_for);
    render_offer_group(ui, "SELLS", &sells, &name_for);
}

fn render_offer_group(
    ui: &mut egui::Ui,
    label: &str,
    offers: &[&map_domain::world::TradeOffer],
    name_for: &dyn Fn(&map_domain::world::TradeOffer) -> String,
) {
    if offers.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(format!("{} ({})", label, offers.len()))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new(format!("trade-grid-{}", label))
                .num_columns(3)
                .spacing([10.0, 2.0])
                .show(ui, |ui| {
                    for o in offers {
                        ui.colored_label(theme::TEXT_PRIMARY, name_for(o));
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.colored_label(
                                    theme::TEXT_MUTED,
                                    format!("{} Cr", fmt_thousands(o.price)),
                                );
                            },
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.colored_label(
                                    theme::TEXT_MUTED,
                                    format!(
                                        "{} / {}",
                                        fmt_thousands(o.amount),
                                        fmt_thousands(o.desired),
                                    ),
                                );
                            },
                        );
                        ui.end_row();
                    }
                });
        });
}
```

- [ ] **Step 3: Compile**

Run: `cargo build`
Expected: success, no warnings about unused imports.

Run: `cargo test`
Expected: existing 75 tests still pass (no panel tests added; behaviour will be checked manually in Task 10).

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/src/ui/sector_panel.rs
git commit -m "$(cat <<'EOF'
feat(panel): render station trade offers in SELECTED block

BUYS/SELLS collapsing sections, each with a 3-column grid
(ware name | price right-aligned | amount/desired right-aligned).
Alphabetical within each section. Zero-amount rows shown.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Manual verification

**Files:** none.

- [ ] **Step 1: Run the app against the real save**

Run: `cargo run --release`
Expected: app launches; universe map loads.

- [ ] **Step 2: Navigate to a production station and verify**

1. Click any sector known to host stations (e.g. "Family Zhin" or any Argon Federation sector).
2. Click "Open 3D View" in the side panel.
3. Click a station icon (square marker). The SELECTED block should appear in the side panel.
4. Verify a `TRADE` section appears below the position/faction lines.
5. Within it, verify two collapsing sub-headers `BUYS (n)` and `SELLS (n)`, each containing a grid of `ware | price Cr | amount / desired`.
6. Verify ware names are human-readable (e.g. `Energy Cells`, not `energycells`). If they show as raw ids, check the stderr log for `[map] Ware names: 0` — that indicates wares.xml failed to load.
7. Verify a station with `amount=0` rows still shows them.
8. Verify a docked ship (click into one inside the station via the DOCKED list) shows no `TRADE` section.

- [ ] **Step 3: If everything works, no commit needed.** If you discover a bug, file it as a follow-up task rather than amending these commits.

---

## Self-Review Checklist (already performed)

- ✅ Every spec section maps to at least one task: domain types → Task 1+2; EntityRecord → Task 3; sector_chunk parsing → Task 4; merge → Task 5; ware-name resolution → Tasks 6+7; load wiring → Task 8; UI rendering → Task 9; manual verification → Task 10.
- ✅ No placeholders, all code blocks complete.
- ✅ Type names consistent across tasks: `TradeOffer` (fields `ware_id`, `direction`, `price`, `amount`, `desired`); `TradeDirection { Buy, Sell }`; `World.trade_offers: HashMap<EntityId, Vec<TradeOffer>>`; `World::trade_offers_of(id) -> &[TradeOffer]`; `Universe.ware_names: HashMap<String, String>`; `parse_ware_names_xml(xml: &[u8], translations: &HashMap<(u32,u32),String>) -> HashMap<String,String>`.
- ✅ Out-of-scope items from the spec are absent from the plan (no trade-route viz, no per-module rows, no storage-capacity field).
