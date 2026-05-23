# 3D View — Custom Icon Redesign

**Date:** 2026-05-19
**Status:** spec (approved, pre-plan)
**Branch:** `p-o-d/threed-view-improvements` (work continues on this branch; do not merge until visually validated).
**Predecessor:** `2026-05-18-3d-icon-billboards-design.md` introduced GPU sprite billboards; that path was reverted in commits `8f8be51` / `9d47c2d` in favour of an egui screen-space painter approach with Unicode glyph + ring. Current icons rely on font glyphs (⚙ ⎈ ⚔ ¤ ⚒ ⌂ ◉ ▦ ◆ ▶ ▴ ▭ ✦ ◎).
**Trigger:** Unicode glyphs are visually generic, font-dependent (we ship DejaVuSansMono as a fallback specifically because base egui font lacks several of these codepoints), and the ship glyphs (▶ ▴ ◆ ▭) look like the in-game X4 chevron icons we're meant to differ from. We want hand-drawn vector icons rendered directly via `egui::Painter` primitives that read at the actual on-screen sizes and lean harder on faction colour.

## Goals

1. Replace all 14 IconId Unicode glyphs with custom vector icons composed of `egui::Painter` primitive shapes (rect, circle, polygon, line). No font dependency for icon rendering.
2. Increase the on-screen size from 16 px / 22 px (normal / selected) to **22 px / 30 px** so that detail is readable.
3. Differentiate the three super-categories — **Station** / **Ship** / **Static** — via three distinct frame shapes:
   - Station: **closed square outline** (faction-coloured stroke).
   - Ship: **closed circle outline** (faction-coloured stroke).
   - Static: **four corner dots** (faint neutral grey).
4. Encode subtype via the white inner glyph. Each of the 14 subtypes has a unique, hand-crafted glyph that does **not** mimic the in-game X4 icon set.
5. Player-owned stations always render with a **white frame** (not faction-coloured) regardless of owner, so they stand out from neutral/own-faction stations.
6. Drop the `DejaVuSansMono` font fallback that exists today solely to render icon codepoints — base egui font is still needed for labels/text, but no icon glyph depends on a specific font any more.

## Non-Goals

- Going back to GPU sprites or per-icon textures — painter-side drawing only.
- Animated icons (pulsing, rotating, etc.).
- LOD / multiple-resolution variants — one design per icon, scaled by a single radius parameter.
- Touching the 2D map view, hover-label text, or side-panel iconography.
- Adding new icon categories beyond the existing 14 — Factory, Wharf/Shipyard, Defense, Trading, EquipDock, HQ, PlayerStation, GenericStation, Capital, Medium, Small, Transport, Anomaly, ResourceZone.

## Architecture Summary

```
crates/map-app/src/renderer/atlas.rs
  IconId enum (unchanged), GLYPHS table (REMOVED), icon_char (REMOVED)
  classify_live, classify_static (unchanged)

crates/map-app/src/renderer/icons.rs  (NEW)
  pub struct IconStyle { half: f32, stroke: f32, color: Color32 }
  pub fn draw_station_frame(painter, center, style, frame_color)
  pub fn draw_ship_frame(painter, center, style, frame_color)
  pub fn draw_static_frame(painter, center, style)
  pub fn draw_glyph(painter, icon: IconId, center: Pos2, half: f32)
       └── matches IconId → one of 14 hand-coded paint functions

crates/map-app/src/ui/sector_view.rs::draw_icons_2d
  Replaces glyph-via-text + rect_stroke/circle_stroke path
  Each entity:
    1. classify → IconId
    2. determine super-category (station/ship/static) + frame_color
    3. draw_*_frame(...)
    4. draw_glyph(...)
```

The `font.ttf` (DejaVuSansMono) embed remains in the build because egui still needs **a** monospace fallback for text labels in some panels, but **icons no longer use any text path**. The font being installed as an egui fallback at startup remains (commit `6446320`); we just don't depend on its specific glyph coverage.

## Section 1 — Sizing constants

All sizes are in screen pixels. Constants live at top of `icons.rs`:

```rust
pub const HALF_NORMAL:     f32 = 11.0;   // 22 px total
pub const HALF_SELECTED:   f32 = 15.0;   // 30 px total
pub const STROKE_NORMAL:   f32 = 1.6;
pub const STROKE_SELECTED: f32 = 2.2;
pub const DOT_RADIUS:      f32 = 1.6;    // static frame corner dots @ normal
pub const DOT_RADIUS_SEL:  f32 = 2.0;    // @ selected
```

The current `STATION_HALF_*`, `SHIP_RADIUS_*`, `GLYPH_FONT_PX*`, `RING_THICKNESS_*` constants in `sector_view.rs` are deleted — `icons.rs` is the single source of truth.

Selection treatment: caller passes `selected: bool` → `icons.rs` swaps `(HALF_NORMAL, STROKE_NORMAL)` for `(HALF_SELECTED, STROKE_SELECTED)` and uses the **yellow selection colour** (`Color32::from_rgb(255, 217, 25)`, matches existing) for the frame instead of the faction colour.

## Section 2 — Frames

All frames are drawn in viewport coordinates centred at `(center.x, center.y)`. `half` = HALF_NORMAL or HALF_SELECTED.

### 2.1 Station frame — square

```rust
pub fn draw_station_frame(painter: &Painter, center: Pos2, half: f32, stroke: f32, color: Color32) {
    let rect = Rect::from_center_size(center, Vec2::splat(half * 2.0));
    painter.rect_stroke(rect, 0.0, Stroke::new(stroke, color), StrokeKind::Outside);
}
```

PlayerStation passes `color = Color32::WHITE`; all other stations pass `faction_color(...)`.

### 2.2 Ship frame — circle

```rust
pub fn draw_ship_frame(painter: &Painter, center: Pos2, half: f32, stroke: f32, color: Color32) {
    let radius = half - stroke * 0.5;
    painter.circle_stroke(center, radius, Stroke::new(stroke, color));
}
```

Radius is inset by half a stroke so the outer edge sits at `half`, matching the station bounding box.

### 2.3 Static frame — corner dots

```rust
pub fn draw_static_frame(painter: &Painter, center: Pos2, half: f32, dot_r: f32) {
    let off = half - dot_r;
    let g = Color32::from_rgb(140, 140, 140);  // theme::TEXT_MUTED-ish
    for sign in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let p = Pos2::new(center.x + sign.0 * off, center.y + sign.1 * off);
        painter.circle_filled(p, dot_r, g);
    }
}
```

Static frame is **always** grey, regardless of faction — static objects are anomalies and resource zones, neither of which has meaningful faction ownership in our model.

## Section 3 — The 14 Glyphs

All glyph functions take `(painter, center: Pos2, half: f32, color: Color32)`. `color` is `Color32::WHITE` by default. Inside each function, all coordinates are computed relative to `center` and scaled relative to `half`. The reference design was sketched at `half = 8` (i.e. a 16×16 box for the inner glyph area, which sits inside the 22×22 frame); each function defines

```rust
let s = half / 8.0;
```

and multiplies **both coordinates and stroke widths** by `s`. So at HALF_NORMAL (11) the inner glyph area is roughly 22×22 (s ≈ 1.375); at HALF_SELECTED (15) it is roughly 30×30 (s ≈ 1.875). Rect sizes and polygon points scale by `s` for x/y; `Stroke::new(w, …)` widths also multiply by `s`. The frame stroke (Section 2) is **not** scaled by `s` — it uses the explicit `STROKE_NORMAL` / `STROKE_SELECTED` values, since the frame is the unit boundary, not part of the inner glyph.

For brevity, the body below uses the `half=8` reference dimensions; multiply each numeric constant by `s` at implementation time.

### 3.1 Factory — stepped refinery

Filled polygon path approximating an industrial stair-step silhouette:

```
M(-3, +3) L(-3, 0) L(0, +2) L(0, -2) L(+3, 0) L(+3, -3) L(+4, -3) L(+4, +3) Z
```

(coords relative to center; positive Y = down). Filled solid white via `Shape::convex_polygon` (or `closed_path` if egui requires non-convex — verify; this polygon is mildly concave).

### 3.2 Wharf / Shipyard — crane jib + load

Two `Shape::line_segment` strokes + one filled square:

- Vertical post: `(-3, -3) → (-3, +3)`, stroke 1.6 white
- Horizontal arm: `(-3, -3) → (+3, -3)`, stroke 1.6 white
- Load box: `Rect::from_min_size(Pos2::new(+1, -1), Vec2::splat(3))` filled white

### 3.3 Defense — heraldic shield

Filled polygon shaped as a heater shield:

```
M(0, -4) L(+4, -3) L(+3, +2) L(0, +4) L(-3, +2) L(-4, -3) Z
```

Filled white. Single shape; no inner detail (would over-noise at 22 px).

### 3.4 Trading — coin stack

Three filled `egui::Shape::ellipse` (or 16-segment polygon approximation if ellipse is unavailable in our egui version — `Shape::ellipse` is in `egui::epaint` since 0.27, our 0.34 has it).

- Top ellipse:    center `(0, -3.5)`, radii `(5, 1.2)`
- Middle ellipse: center `(0, 0)`,    radii `(5, 1.2)`
- Bottom ellipse: center `(0, +3.5)`, radii `(5, 1.2)`

All filled white.

### 3.5 Equipment Dock — repair cross

Two filled `Rect`s forming a plus:

- Vertical: `Rect::from_center_size(center, Vec2::new(2, 12))` filled white
- Horizontal: `Rect::from_center_size(center, Vec2::new(12, 2))` filled white

### 3.6 HQ — pyramid + command dot

- White-filled triangle: `(0, -3.5)`, `(+5, +4)`, `(-5, +4)`
- Faction-coloured dot on top: `circle_filled(center + (0, +1.5), 1.4, faction_color)`

The inner dot uses the **frame's** faction colour so that a player-owned HQ (which has a white frame) shows a **white dot** — visually consistent.

### 3.7 Player Station — white frame + diamond

- Station frame already drawn with `Color32::WHITE` by caller.
- Inner glyph: filled diamond polygon `(0, -5)`, `(+5, 0)`, `(0, +5)`, `(-5, 0)`, white.

### 3.8 Generic Station — hollow ring

`painter.circle_stroke(center, 3.5, Stroke::new(1.6, Color32::WHITE))`.

### 3.9 Capital — 3 tally bars

Three filled `Rect`s, each `Vec2::new(2, 10)`:

- Left bar:   centered at `(-3.5, 0)`
- Middle bar: centered at `( 0,   0)`
- Right bar:  centered at `(+3.5, 0)`

All filled white.

### 3.10 Medium — 2 tally bars

Same as Capital but only 2 bars at `(-2, 0)` and `(+2, 0)`.

### 3.11 Small — 1 tally bar

Single bar at `(0, 0)`.

### 3.12 Transport — 2 stacked containers

Two filled white rectangles + two faction-coloured divider lines:

- Top container:    `Rect::from_min_size(Pos2::new(-7, -5), Vec2::new(14, 3.5))` filled white
- Bottom container: `Rect::from_min_size(Pos2::new(-7, +1), Vec2::new(14, 3.5))` filled white
- Top divider:    `line_segment(Pos2::new(0, -5),  Pos2::new(0, -1.5), Stroke::new(0.8, faction_color))`
- Bottom divider: `line_segment(Pos2::new(0, +1),  Pos2::new(0, +4.5), Stroke::new(0.8, faction_color))`

Dividers use the **frame faction colour** so that the gap reads as background colour, not as accidental detail.

### 3.13 Anomaly — 4-point starburst

Filled polygon — concave 8-point shape giving a 4-point star feel:

```
M(0, -5) L(+1.5, -1.5) L(+5, 0) L(+1.5, +1.5) L(0, +5) L(-1.5, +1.5) L(-5, 0) L(-1.5, -1.5) Z
```

Filled white. Use `Shape::convex_polygon` and accept the visual approximation, OR triangulate manually if needed.

### 3.14 Resource Zone — asteroid blob cluster

Four filled white circles of varying radii at varying positions, suggesting an irregular rock field:

- `circle_filled((-3.5, -3),  1.6, WHITE)`
- `circle_filled((+3,   -4),  1.2, WHITE)`
- `circle_filled((0,    +2.5), 2.0, WHITE)`
- `circle_filled((+5,   +4),  1.4, WHITE)`

The coordinates above are the reference. Implementation may tune for visual balance after rendering.

## Section 4 — Wire-up in `sector_view.rs::draw_icons_2d`

The existing function flow stays the same — for each entity:

1. Project world position to screen.
2. Classify via `classify_live` or `classify_static` → `IconId`.
3. Resolve `selected: bool`, `frame_color: Color32`.
4. **NEW:** Derive super-category from IconId via `IconId::super_category() -> SuperCategory { Station, Ship, Static }` and dispatch to the matching frame function:

   ```rust
   match icon.super_category() {
       SuperCategory::Station => icons::draw_station_frame(painter, center, half, stroke, frame_color),
       SuperCategory::Ship    => icons::draw_ship_frame(painter, center, half, stroke, frame_color),
       SuperCategory::Static  => icons::draw_static_frame(painter, center, half, dot_r),
   }
   ```

   No wrapper helper — caller dispatches.
5. **NEW:** Call `icons::draw_glyph(painter, icon, center, half, frame_color)`. The glyph function reads its own constants for sub-shapes.

The existing `is_station_icon` helper is deleted; the `IconId::super_category()` method replaces it.

PlayerStation gets a special path **inside step 3** — when `icon == PlayerStation`, override `frame_color = Color32::WHITE` regardless of the entity's faction.

## Section 5 — Out of scope / deferred

- Hover-label restyling — current overlay label code stays as-is.
- Selection ring animation or pulse — not introduced.
- Hovered (not selected) entity treatment — no visual change.
- Replacing the existing `theme::TEXT_MUTED` with a new "static dot" colour constant — reuse `theme::TEXT_MUTED` or hard-code `Color32::from_rgb(140, 140, 140)` directly inside `icons.rs`; the latter is preferred because the static frame is icon-specific and doesn't belong to the theme module.
- Removing the `ab_glyph` dependency or `font.ttf` embed — those were already removed in commit `8f8be51`. Confirmed by `Cargo.toml` inspection during plan execution.

## Section 6 — Validation / acceptance

Manual visual checks after implementation:

1. Open Argon Prime (or any populated sector). All ship circles, station squares, and static dot-frames are visible and distinguishable.
2. Three factions visible in one sector → frame colours read as distinct.
3. Player-owned station visible → its frame is white, not faction-coloured.
4. Click a ship / station / static object → frame jumps to yellow `(255, 217, 25)`, size grows ~36 %.
5. All 14 glyph variants render correctly (no `?` fallback character, no font miss) — sanity-check by visiting a sector that has all kinds, or by adding a debug-build helper that emits one of each at fixed positions.

Automated checks:

- `cargo test -p map-app` (existing icon-classification tests stay green).
- New unit tests in `icons.rs`:
  - `super_category(IconId::Factory)` → `SuperCategory::Station`
  - `super_category(IconId::Capital)` → `SuperCategory::Ship`
  - `super_category(IconId::Anomaly)` → `SuperCategory::Static`
  - One test per IconId verifies the corresponding `draw_glyph` arm is reachable (smoke test — call each with a no-op painter via `egui::Painter::for_layer` and confirm no panic; if egui doesn't expose a head-less painter cheaply, skip and rely on visual check).

## Section 7 — File touch list

| File | Change |
|---|---|
| `crates/map-app/src/renderer/atlas.rs` | Delete `GLYPHS`, `icon_char`. Add `IconId::super_category()` impl. Keep `IconId`, `classify_live`, `classify_static`. Update tests. |
| `crates/map-app/src/renderer/icons.rs` | **NEW.** Frame helpers + 14 glyph functions + sizing constants. |
| `crates/map-app/src/renderer/mod.rs` | Add `pub mod icons;` |
| `crates/map-app/src/ui/sector_view.rs` | Rewrite `draw_icons_2d`. Delete `is_station_icon`. Delete the in-file sizing constants. |

Estimated diff: ~+350 / -90 lines.

## Section 8 — Risk + rollback

- **Risk:** A glyph could be visually ambiguous in some faction colour combination that we didn't anticipate. Mitigation: glyphs are pure white; only frame colour varies. Faction-colour-vs-background contrast was already tuned in 2D map work.
- **Risk:** `Shape::convex_polygon` rejects concave Anomaly / Factory paths. Mitigation: if egui rejects, fall back to `Shape::Path` with closed = true, filled = white, stroke = none. Verified in egui 0.34 docs that `Shape::Path { closed, fill, stroke }` accepts arbitrary polygon.
- **Rollback:** Single commit revert. The icon module is additive; `atlas.rs`-side deletions can be reintroduced in one diff.
