# 3D Icon Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 14 Unicode-glyph icons in the 3D sector view with hand-drawn vector icons painted via `egui::Painter` primitives, organised by three super-category frames (square / circle / corner-dots) at a larger 22 / 30 px scale.

**Architecture:** New `renderer/icons.rs` module owns all icon-drawing code: sizing constants, three frame helpers (`draw_station_frame`, `draw_ship_frame`, `draw_static_frame`), and one `draw_glyph` dispatcher that matches on `IconId` and paints 14 distinct inner-glyph shape sets. `atlas.rs` keeps `IconId` + classifiers but loses the `GLYPHS` table and `icon_char` helper; it gains a `SuperCategory` enum and an `IconId::super_category()` method. `sector_view.rs::draw_icons_2d` is rewritten to use the new module.

**Tech Stack:** Rust 2024, egui 0.34.2 (`Painter`, `Shape::convex_polygon`, `Shape::Path`, `Stroke`, `Rect`), wgpu (untouched here — icons are screen-space 2D).

**Spec:** `docs/superpowers/specs/2026-05-19-3d-icon-redesign-design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `crates/map-app/src/renderer/atlas.rs` | Modify | Drop `GLYPHS`, drop `icon_char`. Add `SuperCategory` enum + `IconId::super_category()`. Keep `IconId`, `classify_live`, `classify_static`. |
| `crates/map-app/src/renderer/icons.rs` | **Create** | Sizing constants + 3 frame helpers + `draw_glyph` dispatcher + 14 glyph paint fns. Single source of truth for icon visuals. |
| `crates/map-app/src/renderer/mod.rs` | Modify | Add `pub mod icons;` |
| `crates/map-app/src/ui/sector_view.rs` | Modify | Rewrite `draw_icons_2d` to use `icons` module. Delete `is_station_icon` and the local sizing constants (`STATION_HALF_*`, `SHIP_RADIUS_*`, `GLYPH_FONT_PX*`, `RING_THICKNESS_*`). |

---

## Task 1: Add `SuperCategory` enum + `IconId::super_category()` method

**Files:**
- Modify: `crates/map-app/src/renderer/atlas.rs`

- [ ] **Step 1: Add the failing test at the bottom of the existing `#[cfg(test)] mod tests` block**

Open `crates/map-app/src/renderer/atlas.rs`. Inside `mod tests` (after the existing `classify_static_returns_none_for_gates_and_highways` test) add:

```rust
    #[test]
    fn super_category_station_variants() {
        for v in [
            IconId::Factory, IconId::WharfShipyard, IconId::Defense, IconId::Trading,
            IconId::EquipDock, IconId::HQ, IconId::PlayerStation, IconId::GenericStation,
        ] {
            assert_eq!(v.super_category(), SuperCategory::Station, "{:?}", v);
        }
    }

    #[test]
    fn super_category_ship_variants() {
        for v in [IconId::Capital, IconId::Medium, IconId::Small, IconId::Transport] {
            assert_eq!(v.super_category(), SuperCategory::Ship, "{:?}", v);
        }
    }

    #[test]
    fn super_category_static_variants() {
        for v in [IconId::Anomaly, IconId::ResourceZone] {
            assert_eq!(v.super_category(), SuperCategory::Static, "{:?}", v);
        }
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p map-app --lib renderer::atlas::tests::super_category`

Expected: compile errors — `SuperCategory` not defined, `super_category` method not found.

- [ ] **Step 3: Add the `SuperCategory` enum + method**

Insert near the top of `crates/map-app/src/renderer/atlas.rs`, just after the existing `IconId` enum (after line 26):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SuperCategory {
    Station,
    Ship,
    Static,
}

impl IconId {
    pub fn super_category(self) -> SuperCategory {
        match self {
            IconId::Factory
            | IconId::WharfShipyard
            | IconId::Defense
            | IconId::Trading
            | IconId::EquipDock
            | IconId::HQ
            | IconId::PlayerStation
            | IconId::GenericStation => SuperCategory::Station,

            IconId::Capital
            | IconId::Medium
            | IconId::Small
            | IconId::Transport => SuperCategory::Ship,

            IconId::Anomaly
            | IconId::ResourceZone => SuperCategory::Static,
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p map-app --lib renderer::atlas::tests::super_category`

Expected: 3 passed; 0 failed.

- [ ] **Step 5: Run full test suite to verify no regression**

Run: `cargo test -p map-app`

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/map-app/src/renderer/atlas.rs
git commit -m "feat(renderer): SuperCategory enum + IconId::super_category()"
```

---

## Task 2: Create `renderer/icons.rs` skeleton with sizing constants

**Files:**
- Create: `crates/map-app/src/renderer/icons.rs`
- Modify: `crates/map-app/src/renderer/mod.rs`

- [ ] **Step 1: Add the module declaration**

Open `crates/map-app/src/renderer/mod.rs`. Replace the contents with:

```rust
pub mod atlas;
pub mod camera;
pub mod gpu;
pub mod icons;
pub mod mesh;
```

- [ ] **Step 2: Create `crates/map-app/src/renderer/icons.rs` with constants only**

```rust
//! Hand-drawn icon set for the 3D sector view.
//!
//! Three super-category frames + 14 inner-glyph paint functions. All drawing
//! goes through `egui::Painter` — no font or texture dependency.

use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

use crate::renderer::atlas::{IconId, SuperCategory};

pub const HALF_NORMAL:     f32 = 11.0;   // 22 px total
pub const HALF_SELECTED:   f32 = 15.0;   // 30 px total
pub const STROKE_NORMAL:   f32 = 1.6;
pub const STROKE_SELECTED: f32 = 2.2;
pub const DOT_RADIUS:      f32 = 1.6;
pub const DOT_RADIUS_SEL:  f32 = 2.0;

pub const SELECTION_COLOR: Color32 = Color32::from_rgb(255, 217, 25);
pub const STATIC_FRAME_COLOR: Color32 = Color32::from_rgb(140, 140, 140);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_size_larger_than_normal() {
        assert!(HALF_SELECTED > HALF_NORMAL);
        assert!(STROKE_SELECTED > STROKE_NORMAL);
        assert!(DOT_RADIUS_SEL > DOT_RADIUS);
    }
}
```

- [ ] **Step 3: Verify compile**

Run: `cargo build -p map-app`

Expected: builds cleanly. Unused-import warning for `IconId`, `SuperCategory`, `Painter`, `Pos2`, `Rect`, `Stroke`, `StrokeKind`, `Vec2` is fine; they're consumed in later tasks.

- [ ] **Step 4: Run the smoke test**

Run: `cargo test -p map-app --lib renderer::icons::tests::selected_size_larger_than_normal`

Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/map-app/src/renderer/mod.rs crates/map-app/src/renderer/icons.rs
git commit -m "feat(renderer): icons module skeleton with sizing constants"
```

---

## Task 3: Frame helpers (`draw_station_frame`, `draw_ship_frame`, `draw_static_frame`)

**Files:**
- Modify: `crates/map-app/src/renderer/icons.rs`

- [ ] **Step 1: Add the three frame functions**

Add to `crates/map-app/src/renderer/icons.rs`, after the constants block but before the `#[cfg(test)]` block:

```rust
/// Draw the square outline frame used by all 8 station icons.
pub fn draw_station_frame(
    painter: &Painter,
    center: Pos2,
    half: f32,
    stroke: f32,
    color: Color32,
) {
    let rect = Rect::from_center_size(center, Vec2::splat(half * 2.0));
    painter.rect_stroke(rect, 0.0, Stroke::new(stroke, color), StrokeKind::Outside);
}

/// Draw the circle outline frame used by all 4 ship icons.
pub fn draw_ship_frame(
    painter: &Painter,
    center: Pos2,
    half: f32,
    stroke: f32,
    color: Color32,
) {
    let radius = half - stroke * 0.5;
    painter.circle_stroke(center, radius, Stroke::new(stroke, color));
}

/// Draw the 4-corner-dot frame used by static (anomaly, resource zone) icons.
/// Always grey, regardless of any caller-supplied colour — static objects have
/// no meaningful faction.
pub fn draw_static_frame(painter: &Painter, center: Pos2, half: f32, dot_r: f32) {
    let off = half - dot_r;
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let p = Pos2::new(center.x + sx * off, center.y + sy * off);
        painter.circle_filled(p, dot_r, STATIC_FRAME_COLOR);
    }
}
```

- [ ] **Step 2: Verify compile**

Run: `cargo build -p map-app`

Expected: builds cleanly. `IconId` / `SuperCategory` imports still unused (next tasks consume them) — warnings OK.

- [ ] **Step 3: Commit**

```bash
git add crates/map-app/src/renderer/icons.rs
git commit -m "feat(renderer): icons frame helpers (station/ship/static)"
```

---

## Task 4: `draw_glyph` dispatcher + 8 station glyphs

**Files:**
- Modify: `crates/map-app/src/renderer/icons.rs`

- [ ] **Step 1: Add the dispatcher + 8 station glyph functions**

Append to `crates/map-app/src/renderer/icons.rs`, before the `#[cfg(test)]` block:

```rust
/// Paint the inner glyph for the given IconId at `center`. `half` is the icon
/// half-size in screen pixels (HALF_NORMAL or HALF_SELECTED). `frame_color` is
/// passed through to a couple of glyphs that need it (HQ command dot, Transport
/// dividers).
pub fn draw_glyph(
    painter: &Painter,
    icon: IconId,
    center: Pos2,
    half: f32,
    frame_color: Color32,
) {
    let s = half / 8.0;
    let white = Color32::WHITE;

    match icon {
        // -- stations --
        IconId::Factory        => glyph_factory(painter, center, s, white),
        IconId::WharfShipyard  => glyph_wharf_shipyard(painter, center, s, white),
        IconId::Defense        => glyph_defense(painter, center, s, white),
        IconId::Trading        => glyph_trading(painter, center, s, white),
        IconId::EquipDock      => glyph_equip_dock(painter, center, s, white),
        IconId::HQ             => glyph_hq(painter, center, s, white, frame_color),
        IconId::PlayerStation  => glyph_player_station(painter, center, s, white),
        IconId::GenericStation => glyph_generic_station(painter, center, s, white),

        // -- ships --
        IconId::Capital   => glyph_capital(painter, center, s, white),
        IconId::Medium    => glyph_medium(painter, center, s, white),
        IconId::Small     => glyph_small(painter, center, s, white),
        IconId::Transport => glyph_transport(painter, center, s, white, frame_color),

        // -- static --
        IconId::Anomaly      => glyph_anomaly(painter, center, s, white),
        IconId::ResourceZone => glyph_resource_zone(painter, center, s, white),
    }
}

// -------------------------------------------------------------------------
// Station glyphs
// -------------------------------------------------------------------------

fn glyph_factory(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // stepped refinery silhouette
    let pts = [
        (-3.0, 3.0),
        (-3.0, 0.0),
        (0.0, 2.0),
        (0.0, -2.0),
        (3.0, 0.0),
        (3.0, -3.0),
        (4.0, -3.0),
        (4.0, 3.0),
    ]
    .into_iter()
    .map(|(x, y)| Pos2::new(c.x + x * s, c.y + y * s))
    .collect::<Vec<_>>();
    p.add(egui::Shape::Path(egui::epaint::PathShape {
        points: pts,
        closed: true,
        fill: col,
        stroke: egui::epaint::PathStroke::NONE,
    }));
}

fn glyph_wharf_shipyard(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // L-shaped crane jib + load box
    let stroke = Stroke::new(1.6 * s, col);
    let post_top = Pos2::new(c.x - 3.0 * s, c.y - 3.0 * s);
    let post_bot = Pos2::new(c.x - 3.0 * s, c.y + 3.0 * s);
    let arm_end  = Pos2::new(c.x + 3.0 * s, c.y - 3.0 * s);
    p.line_segment([post_top, post_bot], stroke);
    p.line_segment([post_top, arm_end], stroke);
    let load = Rect::from_min_size(
        Pos2::new(c.x + 1.0 * s, c.y - 1.0 * s),
        Vec2::new(3.0 * s, 3.0 * s),
    );
    p.rect_filled(load, 0.0, col);
}

fn glyph_defense(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // heraldic shield
    let pts = [
        (0.0, -4.0),
        (4.0, -3.0),
        (3.0, 2.0),
        (0.0, 4.0),
        (-3.0, 2.0),
        (-4.0, -3.0),
    ]
    .into_iter()
    .map(|(x, y)| Pos2::new(c.x + x * s, c.y + y * s))
    .collect::<Vec<_>>();
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
}

fn glyph_trading(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // 3-coin stack — wide ellipses approximated as 24-segment polygons
    for dy in [-3.5_f32, 0.0, 3.5] {
        ellipse_filled(p, Pos2::new(c.x, c.y + dy * s), 5.0 * s, 1.2 * s, col);
    }
}

fn glyph_equip_dock(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // thick repair plus
    let v = Rect::from_center_size(c, Vec2::new(2.0 * s, 12.0 * s));
    let h = Rect::from_center_size(c, Vec2::new(12.0 * s, 2.0 * s));
    p.rect_filled(v, 0.0, col);
    p.rect_filled(h, 0.0, col);
}

fn glyph_hq(p: &Painter, c: Pos2, s: f32, col: Color32, dot_color: Color32) {
    // pyramid + command dot (dot uses frame colour, so player-owned HQ shows white dot)
    let pts = vec![
        Pos2::new(c.x + 0.0 * s, c.y - 3.5 * s),
        Pos2::new(c.x + 5.0 * s, c.y + 4.0 * s),
        Pos2::new(c.x - 5.0 * s, c.y + 4.0 * s),
    ];
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
    p.circle_filled(Pos2::new(c.x, c.y + 1.5 * s), 1.4 * s, dot_color);
}

fn glyph_player_station(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // inner diamond (frame is drawn separately with white stroke by caller)
    let pts = vec![
        Pos2::new(c.x + 0.0 * s, c.y - 5.0 * s),
        Pos2::new(c.x + 5.0 * s, c.y + 0.0 * s),
        Pos2::new(c.x + 0.0 * s, c.y + 5.0 * s),
        Pos2::new(c.x - 5.0 * s, c.y + 0.0 * s),
    ];
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
}

fn glyph_generic_station(p: &Painter, c: Pos2, s: f32, col: Color32) {
    p.circle_stroke(c, 3.5 * s, Stroke::new(1.6 * s, col));
}

/// 24-segment polygon ellipse — egui doesn't have a built-in filled ellipse.
fn ellipse_filled(p: &Painter, center: Pos2, rx: f32, ry: f32, col: Color32) {
    const N: usize = 24;
    let pts: Vec<Pos2> = (0..N)
        .map(|i| {
            let t = (i as f32) * std::f32::consts::TAU / (N as f32);
            Pos2::new(center.x + rx * t.cos(), center.y + ry * t.sin())
        })
        .collect();
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
}
```

- [ ] **Step 2: Add stub function bodies for the ship + static glyphs (filled in by later tasks)**

So that the dispatcher's match still compiles, append these stubs at the end of `icons.rs` before the `#[cfg(test)]` block:

```rust
// stubs — filled in by Task 5 + Task 6
fn glyph_capital(_p: &Painter, _c: Pos2, _s: f32, _col: Color32) {}
fn glyph_medium(_p: &Painter, _c: Pos2, _s: f32, _col: Color32) {}
fn glyph_small(_p: &Painter, _c: Pos2, _s: f32, _col: Color32) {}
fn glyph_transport(_p: &Painter, _c: Pos2, _s: f32, _col: Color32, _div: Color32) {}
fn glyph_anomaly(_p: &Painter, _c: Pos2, _s: f32, _col: Color32) {}
fn glyph_resource_zone(_p: &Painter, _c: Pos2, _s: f32, _col: Color32) {}
```

- [ ] **Step 3: Verify compile**

Run: `cargo build -p map-app`

Expected: builds cleanly.

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/src/renderer/icons.rs
git commit -m "feat(renderer): draw_glyph dispatcher + 8 station glyphs"
```

---

## Task 5: 4 ship glyphs (replacing stubs)

**Files:**
- Modify: `crates/map-app/src/renderer/icons.rs`

- [ ] **Step 1: Replace the ship-glyph stubs with real implementations**

In `crates/map-app/src/renderer/icons.rs`, locate the `glyph_capital`, `glyph_medium`, `glyph_small`, `glyph_transport` stubs and replace them with:

```rust
fn glyph_capital(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // 3 vertical tally bars
    for dx in [-3.5_f32, 0.0, 3.5] {
        let r = Rect::from_center_size(
            Pos2::new(c.x + dx * s, c.y),
            Vec2::new(2.0 * s, 10.0 * s),
        );
        p.rect_filled(r, 0.0, col);
    }
}

fn glyph_medium(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // 2 vertical tally bars
    for dx in [-2.0_f32, 2.0] {
        let r = Rect::from_center_size(
            Pos2::new(c.x + dx * s, c.y),
            Vec2::new(2.0 * s, 10.0 * s),
        );
        p.rect_filled(r, 0.0, col);
    }
}

fn glyph_small(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // 1 vertical tally bar
    let r = Rect::from_center_size(c, Vec2::new(2.0 * s, 10.0 * s));
    p.rect_filled(r, 0.0, col);
}

fn glyph_transport(p: &Painter, c: Pos2, s: f32, col: Color32, div: Color32) {
    // 2 stacked horizontal containers + faction-coloured dividers
    let top = Rect::from_min_size(
        Pos2::new(c.x - 7.0 * s, c.y - 5.0 * s),
        Vec2::new(14.0 * s, 3.5 * s),
    );
    let bot = Rect::from_min_size(
        Pos2::new(c.x - 7.0 * s, c.y + 1.0 * s),
        Vec2::new(14.0 * s, 3.5 * s),
    );
    p.rect_filled(top, 0.0, col);
    p.rect_filled(bot, 0.0, col);
    let div_stroke = Stroke::new(0.8 * s, div);
    p.line_segment(
        [Pos2::new(c.x, c.y - 5.0 * s), Pos2::new(c.x, c.y - 1.5 * s)],
        div_stroke,
    );
    p.line_segment(
        [Pos2::new(c.x, c.y + 1.0 * s), Pos2::new(c.x, c.y + 4.5 * s)],
        div_stroke,
    );
}
```

- [ ] **Step 2: Verify compile**

Run: `cargo build -p map-app`

Expected: builds cleanly.

- [ ] **Step 3: Commit**

```bash
git add crates/map-app/src/renderer/icons.rs
git commit -m "feat(renderer): 4 ship glyphs (tally bars + transport)"
```

---

## Task 6: 2 static glyphs (anomaly + resource zone)

**Files:**
- Modify: `crates/map-app/src/renderer/icons.rs`

- [ ] **Step 1: Replace the static-glyph stubs**

Locate the `glyph_anomaly` + `glyph_resource_zone` stubs and replace them with:

```rust
fn glyph_anomaly(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // 4-point starburst — concave 8-vertex polygon (paint as Shape::Path)
    let pts = [
        (0.0, -5.0),
        (1.5, -1.5),
        (5.0, 0.0),
        (1.5, 1.5),
        (0.0, 5.0),
        (-1.5, 1.5),
        (-5.0, 0.0),
        (-1.5, -1.5),
    ]
    .into_iter()
    .map(|(x, y)| Pos2::new(c.x + x * s, c.y + y * s))
    .collect::<Vec<_>>();
    p.add(egui::Shape::Path(egui::epaint::PathShape {
        points: pts,
        closed: true,
        fill: col,
        stroke: egui::epaint::PathStroke::NONE,
    }));
}

fn glyph_resource_zone(p: &Painter, c: Pos2, s: f32, col: Color32) {
    // irregular cluster of 4 asteroid circles
    for (dx, dy, r) in [
        (-3.5_f32, -3.0_f32, 1.6_f32),
        (3.0,      -4.0,     1.2),
        (0.0,       2.5,     2.0),
        (5.0,       4.0,     1.4),
    ] {
        p.circle_filled(Pos2::new(c.x + dx * s, c.y + dy * s), r * s, col);
    }
}
```

- [ ] **Step 2: Verify compile**

Run: `cargo build -p map-app`

Expected: builds cleanly.

- [ ] **Step 3: Run the icons module tests**

Run: `cargo test -p map-app --lib renderer::icons`

Expected: 1 passed (the `selected_size_larger_than_normal` test from Task 2).

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/src/renderer/icons.rs
git commit -m "feat(renderer): 2 static glyphs (anomaly + resource zone)"
```

---

## Task 7: Rewrite `draw_icons_2d` in `sector_view.rs`

**Files:**
- Modify: `crates/map-app/src/ui/sector_view.rs`

- [ ] **Step 1: Delete the `is_station_icon` helper**

In `crates/map-app/src/ui/sector_view.rs`, find and delete the function starting at line 183:

```rust
fn is_station_icon(icon: crate::renderer::atlas::IconId) -> bool {
    use crate::renderer::atlas::IconId;
    matches!(
        icon,
        IconId::Factory
            | IconId::WharfShipyard
            | IconId::Defense
            | IconId::Trading
            | IconId::EquipDock
            | IconId::HQ
            | IconId::PlayerStation
            | IconId::GenericStation
    )
}
```

Delete the whole function block.

- [ ] **Step 2: Rewrite `draw_icons_2d` to use the new `icons` module**

In `crates/map-app/src/ui/sector_view.rs`, replace the entire `draw_icons_2d` function body (between the `fn draw_icons_2d(...) {` line and its closing `}`) with:

```rust
fn draw_icons_2d(
    painter: &egui::Painter,
    view_rect: egui::Rect,
    camera: &OrbitCamera,
    sector: &Sector,
    world: Option<&map_domain::world::World>,
    universe: &map_domain::universe::Universe,
    selected_obj: Option<ObjectId>,
    selected_entity: Option<map_domain::world::EntityId>,
) {
    use crate::renderer::atlas::{classify_live, classify_static, IconId, SuperCategory};
    use crate::renderer::icons;

    let aspect = view_rect.width() / view_rect.height().max(1.0);
    let vp = camera.proj_matrix(aspect) * camera.view_matrix();

    let project = |w_pos: Vec3| -> Option<Pos2> {
        let clip = vp * w_pos.extend(1.0);
        if clip.w <= 0.0 { return None; }
        let ndc = clip.truncate() / clip.w;
        if ndc.x.abs() > 1.5 || ndc.y.abs() > 1.5 { return None; }
        Some(Pos2::new(
            (ndc.x * 0.5 + 0.5) * view_rect.width() + view_rect.left(),
            (1.0 - (ndc.y * 0.5 + 0.5)) * view_rect.height() + view_rect.top(),
        ))
    };

    let emit = |screen: Pos2, icon: IconId, faction_color: egui::Color32, selected: bool| {
        let (half, stroke, dot_r) = if selected {
            (icons::HALF_SELECTED, icons::STROKE_SELECTED, icons::DOT_RADIUS_SEL)
        } else {
            (icons::HALF_NORMAL, icons::STROKE_NORMAL, icons::DOT_RADIUS)
        };
        // PlayerStation always has a white frame; selection overrides to yellow.
        let frame_color = if selected {
            icons::SELECTION_COLOR
        } else if icon == IconId::PlayerStation {
            egui::Color32::WHITE
        } else {
            faction_color
        };
        match icon.super_category() {
            SuperCategory::Station => icons::draw_station_frame(painter, screen, half, stroke, frame_color),
            SuperCategory::Ship    => icons::draw_ship_frame(painter, screen, half, stroke, frame_color),
            SuperCategory::Static  => icons::draw_static_frame(painter, screen, half, dot_r),
        }
        icons::draw_glyph(painter, icon, screen, half, frame_color);
    };

    // Static objects.
    for obj in &sector.static_objects {
        let Some(icon) = classify_static(&obj.kind) else { continue };
        let Some(screen) = project(obj.position) else { continue };
        let ring = obj.faction
            .map(|f| crate::colors::faction_color(universe, f))
            .unwrap_or(crate::theme::TEXT_MUTED);
        emit(screen, icon, ring, selected_obj == Some(obj.id));
    }

    // Live entities (top-level only).
    if let Some(world) = world {
        let mut fid_to_str: std::collections::HashMap<map_domain::ids::FactionId, &str> =
            std::collections::HashMap::new();
        for (k, v) in &universe.faction_strings {
            fid_to_str.insert(*v, k.as_str());
        }
        for &eid in world.entities_in_sector(sector.id) {
            if world.parent_of(eid).is_some() { continue; }
            let Some(&pos) = world.positions.get(&eid) else { continue };
            let kind = match world.kinds.get(&eid) { Some(k) => k.clone(), None => continue };
            let Some(screen) = project(pos) else { continue };
            let macro_name = world.names.get(&eid).map(String::as_str).unwrap_or("");
            let owner_str = world.factions.get(&eid).and_then(|f| fid_to_str.get(f).copied());
            let icon = classify_live(kind, macro_name, owner_str);
            let ring = world.factions.get(&eid).copied()
                .map(|f| crate::colors::faction_color(universe, f))
                .unwrap_or(crate::theme::TEXT_MUTED);
            emit(screen, icon, ring, selected_entity == Some(eid));
        }
    }
}
```

- [ ] **Step 3: Verify compile**

Run: `cargo build -p map-app`

Expected: builds cleanly. Any "unused" warnings from former imports (e.g. `icon_char`) — proceed; we clean atlas.rs in Task 8.

- [ ] **Step 4: Run the full test suite**

Run: `cargo test -p map-app`

Expected: all pre-existing tests pass.

- [ ] **Step 5: Build + run the app for a visual check**

Run: `cargo run --release` and open a populated sector (e.g. Argon Prime). Confirm:
- Stations: square frame, faction colour.
- Ships: circle frame, faction colour.
- Static objects: 4 grey dots, no frame colour.
- Player station: white frame.
- Click an entity: frame turns yellow, scale jumps to 30 px.

If anything renders wrong, fix in place before committing.

- [ ] **Step 6: Commit**

```bash
git add crates/map-app/src/ui/sector_view.rs
git commit -m "feat(3d): draw_icons_2d uses new icons module (vector glyphs + 3 frames)"
```

---

## Task 8: Clean up `atlas.rs` — drop `GLYPHS` and `icon_char`

**Files:**
- Modify: `crates/map-app/src/renderer/atlas.rs`

- [ ] **Step 1: Delete the `GLYPHS` constant**

In `crates/map-app/src/renderer/atlas.rs`, delete the entire `GLYPHS` constant block (lines roughly 28–43 in the current file):

```rust
const GLYPHS: &[(IconId, char)] = &[
    (IconId::Factory,        '⚙'),
    (IconId::WharfShipyard,  '⎈'),
    (IconId::Defense,        '⚔'),
    (IconId::Trading,        '¤'),
    (IconId::EquipDock,      '⚒'),
    (IconId::HQ,             '⌂'),
    (IconId::PlayerStation,  '◉'),
    (IconId::GenericStation, '▦'),
    (IconId::Capital,        '◆'),
    (IconId::Medium,         '▶'),
    (IconId::Small,          '▴'),
    (IconId::Transport,      '▭'),
    (IconId::Anomaly,        '✦'),
    (IconId::ResourceZone,   '◎'),
];
```

- [ ] **Step 2: Delete the `icon_char` function**

Delete:

```rust
pub fn icon_char(icon: IconId) -> char {
    GLYPHS.iter().find(|(i, _)| *i == icon).map(|(_, c)| *c).unwrap_or('?')
}
```

- [ ] **Step 3: Delete the two now-obsolete tests inside `mod tests`**

Delete:

```rust
    #[test]
    fn icon_char_returns_known_glyphs() {
        assert_eq!(icon_char(IconId::Factory), '⚙');
        assert_eq!(icon_char(IconId::WharfShipyard), '⎈');
        assert_eq!(icon_char(IconId::Trading), '¤');
        assert_eq!(icon_char(IconId::Anomaly), '✦');
    }

    #[test]
    fn glyph_table_has_one_entry_per_variant() {
        let all: Vec<IconId> = GLYPHS.iter().map(|(i, _)| *i).collect();
        let expected = [
            IconId::Factory, IconId::WharfShipyard, IconId::Defense, IconId::Trading,
            IconId::EquipDock, IconId::HQ, IconId::PlayerStation, IconId::GenericStation,
            IconId::Capital, IconId::Medium, IconId::Small, IconId::Transport,
            IconId::Anomaly, IconId::ResourceZone,
        ];
        for e in &expected { assert!(all.contains(e), "missing {:?}", e); }
        assert_eq!(all.len(), expected.len());
    }
```

- [ ] **Step 4: Verify compile + tests**

Run: `cargo build -p map-app && cargo test -p map-app`

Expected: builds cleanly; all tests pass (super_category + classify_* tests + new icons sizing test).

- [ ] **Step 5: Confirm no stale imports / dead code**

Run: `cargo clippy -p map-app -- -D warnings`

Expected: clippy passes. Any new warnings from removed code should be fixed in place.

- [ ] **Step 6: Commit**

```bash
git add crates/map-app/src/renderer/atlas.rs
git commit -m "refactor(renderer): drop GLYPHS table + icon_char (icons module replaces)"
```

---

## Task 9 (optional): Drop DejaVuSansMono egui-fallback install if labels still render

The font fallback installation lives in `crates/map-app/src/main.rs` (or wherever `egui::Context::set_fonts` is called — confirm via `grep -rn "set_fonts\|FontDefinitions" crates/map-app/src/`). Its commit message (`6446320`) states the fallback was added *specifically* to render icon glyphs. With icons no longer using text, the fallback is likely dead code.

- [ ] **Step 1: Locate the fallback install**

Run: `grep -rn "DejaVuSansMono\|set_fonts\|FontDefinitions" crates/map-app/src/`

- [ ] **Step 2: Comment out the fallback registration call (do not delete the font file yet)**

Comment out the `set_fonts` / `FontDefinitions` block. Save the original lines as a code comment above so the next reader can see what was removed.

- [ ] **Step 3: Run the app and inspect every panel + label**

Run: `cargo run --release`

Walk through:
- Universe map: sector hex labels, cluster labels
- Sector view: hover labels, axis arrow labels (E/W/N/S/Up/Dn)
- Side panel: header, faction names, object detail rows

If everything renders, proceed. If any character renders as `□` or missing-glyph, revert step 2 and skip this task.

- [ ] **Step 4: Remove the now-commented block entirely + delete the font asset**

If step 3 passes, delete the `set_fonts` block (not just comment) and remove `crates/map-app/assets/font.ttf`. Update any `include_bytes!` reference accordingly.

- [ ] **Step 5: Run tests + clippy**

Run: `cargo test -p map-app && cargo clippy -p map-app -- -D warnings`

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "chore(app): drop DejaVuSansMono fallback (no longer needed without text glyphs)"
```

If step 3 failed, skip the commit and leave the fallback alone — spec Goal 6 is treated as best-effort.

---

## Final verification

After Task 8 (and 9 if applicable):

- [ ] Run: `cargo test -p map-app` — full suite green.
- [ ] Run: `cargo clippy -p map-app -- -D warnings` — clean.
- [ ] Visual check: open the app, walk through 3-4 sectors with mixed factions, confirm all 14 icon types render and selection works as designed.
- [ ] Visual check: hover doesn't break (label overlay still works, untouched by this change).

If everything checks, the branch `p-o-d/threed-view-improvements` is ready for either merge to master or further iteration based on visual feedback.
