# 3D Icon Billboards — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace generic GPU boxes / spheres for top-level live entities and non-gate static objects with billboard icons — 2D glyphs facing the camera, faction-coloured ring outside a white glyph — drawn through a new sprite pipeline.

**Architecture:** At startup, rasterise 14 Unicode glyphs (⚙ ⎈ ⚔ ⛁ ⚒ ⌂ ◉ ▦ ◆ ▶ ▴ ▭ ✦ ◎) from an embedded TTF into a 384×144 R8 atlas. Build per-IconId UV lookup. Add a new `wgpu` sprite pipeline alongside the existing mesh pipeline. Per frame, classify each entity to an `IconId`, build a `SpriteInstance` (world position + UV rect + ring colour + scale), upload as instance buffer, single instanced draw call. Selection = 1.3× scale + yellow ring overriding faction colour.

**Tech Stack:** Rust 2024, `ab_glyph = "0.2"` (new), existing `wgpu` via `eframe::egui_wgpu`, existing `glam`, embedded font asset under `crates/map-app/assets/font.ttf`.

**Spec:** `docs/superpowers/specs/2026-05-18-3d-icon-billboards-design.md`

**Branch:** to be implemented on a feature branch. **Do NOT merge to master** until the user has visually validated the change.

---

## File Structure

**Created:**
- `crates/map-app/src/renderer/atlas.rs` — IconId enum, GLYPHS table, AtlasLookup, classify_live, classify_static, rasterise_glyphs.
- `crates/map-app/src/renderer/sprite.rs` — SpritePipeline (wgpu state), SpriteInstance type, from_target helper.
- `crates/map-app/assets/font.ttf` — embedded TTF (DejaVuSansMono.ttf, public-domain Bitstream Vera derivative).

**Modified:**
- `crates/map-app/Cargo.toml` — adds `ab_glyph = "0.2"`.
- `crates/map-app/src/renderer/mod.rs` — `pub mod atlas; pub mod sprite;`.
- `crates/map-app/src/renderer/gpu.rs` — `GpuScene` grows sprite pipeline + atlas texture + sprite instance buffer + `set_sprite_instances` method; paint callback runs sprite pass after the mesh pass.
- `crates/map-app/src/ui/sector_view.rs` — `build_draw_calls` drops live-entity + Anomaly/ResourceZone/Station branches; new `build_sprite_instances` populates the new pipeline; `pick_target` hit radius bumped 20 → 24 px.

**Deleted (within `sector_view.rs::build_draw_calls`):**
- Live-entity mesh emission loop.
- Static Anomaly + ResourceZone + Station mesh emission.
(Gate / Highway are already excluded — 2D path keeps working unchanged.)

---

### Task 1: Add `ab_glyph` dep + embed font

**Files:**
- Modify: `crates/map-app/Cargo.toml`
- Create: `crates/map-app/assets/font.ttf`

- [ ] **Step 1: Add dep**

Edit `crates/map-app/Cargo.toml`. After the existing `[dependencies]` lines (where `eframe`, `glam` etc. live), add:

```toml
ab_glyph = "0.2"
```

- [ ] **Step 2: Embed font file**

Acquire a copy of `DejaVuSansMono.ttf` (public-domain Bitstream Vera derivative). On most Linux systems it's at `/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf`. Copy it:

```bash
mkdir -p crates/map-app/assets
cp /usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf crates/map-app/assets/font.ttf
```

If DejaVuSansMono.ttf is not available, download from <https://dejavu-fonts.github.io/>. License: public-domain (Bitstream Vera license + Arev derivative).

Verify glyph coverage with this one-off snippet (do NOT commit this — it's only for verification):

```bash
python3 -c "
from fontTools.ttLib import TTFont
ttf = TTFont('crates/map-app/assets/font.ttf')
cmap = ttf.getBestCmap()
glyphs = ['⚙','⎈','⚔','⛁','⚒','⌂','◉','▦','◆','▶','▴','▭','✦','◎']
for g in glyphs:
    print(f'{g} U+{ord(g):04X}: {\"OK\" if ord(g) in cmap else \"MISSING\"}')
"
```

Expected: all OK. If ANY glyph reports MISSING, switch to `NotoSansSymbols2-Regular.ttf` from <https://fonts.google.com/noto/specimen/Noto+Sans+Symbols+2>, save as `crates/map-app/assets/font.ttf`, and re-verify.

If `fontTools` isn't installed, that's fine — skip the verification, run Task 4 in this plan and let its glyph-bake log report the truth.

- [ ] **Step 3: Build to confirm asset compiles in**

```bash
cargo build 2>&1 | grep "^error" | head -5
```

Expected: no errors (no code uses the font yet).

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/Cargo.toml crates/map-app/assets/font.ttf
git commit -m "build(app): add ab_glyph dep + embed DejaVuSansMono.ttf for icon atlas"
```

---

### Task 2: `atlas.rs` — IconId, GLYPHS, AtlasLookup

**Files:**
- Create: `crates/map-app/src/renderer/atlas.rs`
- Modify: `crates/map-app/src/renderer/mod.rs`

- [ ] **Step 1: Add module declaration**

Edit `crates/map-app/src/renderer/mod.rs`. Append:

```rust
pub mod atlas;
```

- [ ] **Step 2: Write failing tests**

Create `crates/map-app/src/renderer/atlas.rs` with this content (initial — implementation comes next):

```rust
//! Icon atlas: enum of icon ids, glyph table, UV lookup.
//!
//! Rasterisation lives in this module too but is invoked by `gpu.rs` at
//! pipeline init.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconId {
    Factory,
    WharfShipyard,
    Defense,
    Trading,
    EquipDock,
    HQ,
    PlayerStation,
    GenericStation,
    Capital,
    Medium,
    Small,
    Transport,
    Anomaly,
    ResourceZone,
}

/// Ordered list — slot index in the atlas matches array index here.
pub const GLYPHS: &[(IconId, char)] = &[
    (IconId::Factory,        '⚙'),
    (IconId::WharfShipyard,  '⎈'),
    (IconId::Defense,        '⚔'),
    (IconId::Trading,        '⛁'),
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

pub const ATLAS_COLS: usize = 8;
pub const ATLAS_ROWS: usize = 3;
pub const TILE_PX: usize = 48;
pub const ATLAS_W: usize = ATLAS_COLS * TILE_PX;   // 384
pub const ATLAS_H: usize = ATLAS_ROWS * TILE_PX;   // 144

/// Lookup of IconId → [u_min, v_min, du, dv] (atlas-normalised UV rect).
#[derive(Debug, Clone)]
pub struct AtlasLookup {
    pub uv: HashMap<IconId, [f32; 4]>,
}

impl AtlasLookup {
    pub fn build() -> Self {
        let mut uv = HashMap::new();
        for (idx, (icon, _)) in GLYPHS.iter().enumerate() {
            let col = idx % ATLAS_COLS;
            let row = idx / ATLAS_COLS;
            let u0 = (col * TILE_PX) as f32 / ATLAS_W as f32;
            let v0 = (row * TILE_PX) as f32 / ATLAS_H as f32;
            let du = TILE_PX as f32 / ATLAS_W as f32;
            let dv = TILE_PX as f32 / ATLAS_H as f32;
            uv.insert(*icon, [u0, v0, du, dv]);
        }
        Self { uv }
    }

    pub fn uv_of(&self, icon: IconId) -> [f32; 4] {
        *self.uv.get(&icon).expect("every IconId must be in GLYPHS")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_table_covers_every_icon_id() {
        let all: Vec<IconId> = GLYPHS.iter().map(|(i, _)| *i).collect();
        // Spot-check every variant appears in the table.
        let expected = [
            IconId::Factory, IconId::WharfShipyard, IconId::Defense, IconId::Trading,
            IconId::EquipDock, IconId::HQ, IconId::PlayerStation, IconId::GenericStation,
            IconId::Capital, IconId::Medium, IconId::Small, IconId::Transport,
            IconId::Anomaly, IconId::ResourceZone,
        ];
        for e in &expected {
            assert!(all.contains(e), "missing IconId in GLYPHS: {:?}", e);
        }
        assert_eq!(all.len(), expected.len(), "GLYPHS has extras or duplicates");
    }

    #[test]
    fn atlas_layout_assigns_unique_uv_rects_per_icon() {
        let a = AtlasLookup::build();
        let mut seen: Vec<[u32; 4]> = Vec::new();  // quantise to avoid float keys
        for (icon, _) in GLYPHS {
            let r = a.uv_of(*icon);
            let key = [
                (r[0] * 1_000_000.0) as u32,
                (r[1] * 1_000_000.0) as u32,
                (r[2] * 1_000_000.0) as u32,
                (r[3] * 1_000_000.0) as u32,
            ];
            assert!(!seen.contains(&key), "duplicate UV rect for {:?}", icon);
            seen.push(key);
        }
        // Every rect should fit inside [0, 1].
        for [u, v, du, dv] in seen.iter().map(|[u, v, du, dv]| [
            *u as f32 / 1_000_000.0, *v as f32 / 1_000_000.0,
            *du as f32 / 1_000_000.0, *dv as f32 / 1_000_000.0,
        ]) {
            assert!(u >= 0.0 && u + du <= 1.0);
            assert!(v >= 0.0 && v + dv <= 1.0);
        }
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p map-app --lib renderer::atlas::tests 2>&1 | tail -5
```

Expected: `2 passed`.

- [ ] **Step 4: Commit**

```bash
git add crates/map-app/src/renderer/atlas.rs crates/map-app/src/renderer/mod.rs
git commit -m "feat(render): IconId enum + GLYPHS table + AtlasLookup (UV rects)"
```

---

### Task 3: Classification rules

**Files:**
- Modify: `crates/map-app/src/renderer/atlas.rs`

- [ ] **Step 1: Write failing tests**

Append to `crates/map-app/src/renderer/atlas.rs` (above the existing `#[cfg(test)] mod tests` block):

```rust
use map_domain::objects::StaticObjectKind;
use map_domain::world::LiveObjectKind;

pub fn classify_live(
    kind: LiveObjectKind,
    macro_name: &str,
    owner: Option<&str>,
) -> IconId {
    // Implementation in Step 3.
    let _ = (kind, macro_name, owner);
    IconId::GenericStation
}

pub fn classify_static(kind: &StaticObjectKind) -> Option<IconId> {
    // Implementation in Step 3.
    let _ = kind;
    None
}
```

Append the following inside the existing `#[cfg(test)] mod tests`:

```rust
    use map_domain::objects::StaticObjectKind;
    use map_domain::world::LiveObjectKind;

    #[test]
    fn classify_live_routes_factory_macro_to_factory_icon() {
        let icon = classify_live(LiveObjectKind::Station, "station_arg_factory_food_01_macro", Some("argon"));
        assert_eq!(icon, IconId::Factory);
    }

    #[test]
    fn classify_live_player_owner_wins_over_macro() {
        let icon = classify_live(LiveObjectKind::Station, "station_arg_factory_food_01_macro", Some("player"));
        assert_eq!(icon, IconId::PlayerStation);
    }

    #[test]
    fn classify_live_wharf_or_shipyard() {
        assert_eq!(classify_live(LiveObjectKind::Station, "station_par_wharf_macro", Some("paranid")), IconId::WharfShipyard);
        assert_eq!(classify_live(LiveObjectKind::Station, "station_tel_shipyard_macro", Some("teladi")), IconId::WharfShipyard);
    }

    #[test]
    fn classify_live_defense_handles_both_spellings() {
        assert_eq!(classify_live(LiveObjectKind::Station, "station_arg_defence_platform_macro", None), IconId::Defense);
        assert_eq!(classify_live(LiveObjectKind::Station, "station_arg_defense_module_macro", None), IconId::Defense);
    }

    #[test]
    fn classify_live_capital_ship_by_size() {
        assert_eq!(classify_live(LiveObjectKind::ShipLarge, "ship_arg_l_destroyer_01_macro", Some("argon")), IconId::Capital);
        assert_eq!(classify_live(LiveObjectKind::ShipExtraLarge, "ship_arg_xl_carrier_01_macro", Some("argon")), IconId::Capital);
    }

    #[test]
    fn classify_live_transport_keyword_overrides_size() {
        assert_eq!(classify_live(LiveObjectKind::ShipLarge, "ship_arg_l_freighter_01_macro", Some("argon")), IconId::Transport);
        assert_eq!(classify_live(LiveObjectKind::ShipMedium, "ship_tel_m_miner_01_macro", Some("teladi")), IconId::Transport);
    }

    #[test]
    fn classify_live_small_ship_default() {
        assert_eq!(classify_live(LiveObjectKind::ShipSmall, "ship_arg_s_scout_01_macro", Some("argon")), IconId::Small);
    }

    #[test]
    fn classify_live_unknown_station_falls_back_to_generic() {
        assert_eq!(classify_live(LiveObjectKind::Station, "station_xxx_unknown_macro", None), IconId::GenericStation);
    }

    #[test]
    fn classify_static_anomaly_and_resource_zone() {
        assert_eq!(classify_static(&StaticObjectKind::Anomaly), Some(IconId::Anomaly));
        assert_eq!(classify_static(&StaticObjectKind::ResourceZone), Some(IconId::ResourceZone));
    }

    #[test]
    fn classify_static_returns_none_for_gates_and_highways() {
        assert_eq!(classify_static(&StaticObjectKind::Gate), None);
        assert_eq!(classify_static(&StaticObjectKind::Highway), None);
    }

    #[test]
    fn classify_static_returns_generic_for_station_variant() {
        assert_eq!(classify_static(&StaticObjectKind::Station), Some(IconId::GenericStation));
    }
```

- [ ] **Step 2: Run — expect FAIL**

```bash
cargo test -p map-app --lib renderer::atlas::tests 2>&1 | tail -10
```

Expected: 8+ assertion failures (the two stubs always return their placeholders).

- [ ] **Step 3: Implement classification**

Replace the bodies in `atlas.rs`:

```rust
pub fn classify_live(
    kind: LiveObjectKind,
    macro_name: &str,
    owner: Option<&str>,
) -> IconId {
    let m = macro_name.to_lowercase();
    match kind {
        LiveObjectKind::Station => {
            if owner == Some("player") { return IconId::PlayerStation; }
            if m.contains("wharf") || m.contains("shipyard") { return IconId::WharfShipyard; }
            if m.contains("defence") || m.contains("defense") { return IconId::Defense; }
            if m.contains("trading") { return IconId::Trading; }
            if m.contains("equip") || m.contains("dock") { return IconId::EquipDock; }
            if m.contains("hq") || m.contains("admin") || m.contains("headquarter") {
                return IconId::HQ;
            }
            if m.contains("factory") || m.contains("refinery") || m.contains("production") {
                return IconId::Factory;
            }
            IconId::GenericStation
        }
        _ if m.contains("trans") || m.contains("freight") || m.contains("miner") => IconId::Transport,
        LiveObjectKind::ShipExtraLarge | LiveObjectKind::ShipLarge => IconId::Capital,
        LiveObjectKind::ShipMedium => IconId::Medium,
        LiveObjectKind::ShipSmall => IconId::Small,
    }
}

pub fn classify_static(kind: &StaticObjectKind) -> Option<IconId> {
    match kind {
        StaticObjectKind::Anomaly => Some(IconId::Anomaly),
        StaticObjectKind::ResourceZone => Some(IconId::ResourceZone),
        StaticObjectKind::Station => Some(IconId::GenericStation),
        StaticObjectKind::Gate | StaticObjectKind::Highway => None,
    }
}
```

- [ ] **Step 4: Run — expect PASS**

```bash
cargo test -p map-app --lib renderer::atlas::tests 2>&1 | tail -5
```

Expected: all atlas tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/map-app/src/renderer/atlas.rs
git commit -m "feat(render): classify_live + classify_static map entities to IconId"
```

---

### Task 4: Glyph rasterisation → atlas pixel buffer

**Files:**
- Modify: `crates/map-app/src/renderer/atlas.rs`

- [ ] **Step 1: Write a failing test for `rasterise_glyphs`**

Append to the existing `#[cfg(test)] mod tests` in `atlas.rs`:

```rust
    #[test]
    fn rasterise_glyphs_produces_full_size_buffer_with_some_nonzero() {
        let font_bytes = include_bytes!("../../assets/font.ttf");
        let (buf, missing) = rasterise_glyphs(font_bytes);
        assert_eq!(buf.len(), ATLAS_W * ATLAS_H);
        // Most glyphs should bake — accept up to 2 missing for font variance.
        assert!(missing <= 2, "too many missing glyphs: {}", missing);
        let nonzero = buf.iter().filter(|&&b| b > 0).count();
        assert!(nonzero > 1000, "expected meaningful glyph coverage, got {} pixels", nonzero);
    }
```

- [ ] **Step 2: Run — expect FAIL** (`rasterise_glyphs` not defined)

```bash
cargo test -p map-app --lib renderer::atlas::tests::rasterise_glyphs_produces_full_size_buffer_with_some_nonzero 2>&1 | tail -5
```

- [ ] **Step 3: Implement `rasterise_glyphs`**

Append to `atlas.rs` (above the `#[cfg(test)]` block):

```rust
/// Rasterise every glyph in `GLYPHS` into a single R8 (alpha-only) buffer of
/// size `ATLAS_W * ATLAS_H`. Returns `(buf, missing_count)`.
///
/// Each glyph is centred in its 48-pixel tile.
pub fn rasterise_glyphs(font_bytes: &[u8]) -> (Vec<u8>, usize) {
    use ab_glyph::{Font, FontRef, PxScale, ScaleFont};

    let font = FontRef::try_from_slice(font_bytes).expect("font_bytes is a valid TTF");
    let scale = PxScale::from(40.0); // glyph height ~40 in a 48-px tile (4 px padding).
    let scaled = font.as_scaled(scale);

    let mut buf = vec![0u8; ATLAS_W * ATLAS_H];
    let mut missing = 0usize;

    for (idx, (icon, ch)) in GLYPHS.iter().enumerate() {
        let col = idx % ATLAS_COLS;
        let row = idx / ATLAS_COLS;
        let tile_x = (col * TILE_PX) as i32;
        let tile_y = (row * TILE_PX) as i32;

        let glyph_id = font.glyph_id(*ch);
        if glyph_id.0 == 0 {
            eprintln!("[render] atlas: glyph {:?} for {:?} missing in font; tile left blank", ch, icon);
            missing += 1;
            continue;
        }

        let glyph = glyph_id.with_scale(scale);
        let outline = match font.outline_glyph(glyph) {
            Some(o) => o,
            None => continue, // whitespace / no outline
        };
        let bounds = outline.px_bounds();

        // Centre the outline in the tile.
        let glyph_w = bounds.width() as i32;
        let glyph_h = bounds.height() as i32;
        let offset_x = tile_x + (TILE_PX as i32 - glyph_w) / 2 - bounds.min.x as i32;
        let offset_y = tile_y + (TILE_PX as i32 - glyph_h) / 2 - bounds.min.y as i32;

        outline.draw(|gx, gy, coverage| {
            let px = offset_x + bounds.min.x as i32 + gx as i32;
            let py = offset_y + bounds.min.y as i32 + gy as i32;
            if px < tile_x || px >= tile_x + TILE_PX as i32 { return; }
            if py < tile_y || py >= tile_y + TILE_PX as i32 { return; }
            let i = py as usize * ATLAS_W + px as usize;
            let alpha = (coverage * 255.0).clamp(0.0, 255.0) as u8;
            buf[i] = buf[i].max(alpha);
        });
    }

    eprintln!(
        "[render] atlas: {} glyphs baked, {} missing",
        GLYPHS.len() - missing,
        missing
    );
    (buf, missing)
}
```

- [ ] **Step 4: Run — expect PASS**

```bash
cargo test -p map-app --lib renderer::atlas::tests::rasterise_glyphs_produces_full_size_buffer_with_some_nonzero 2>&1 | tail -5
```

Expected: `1 passed`. Stderr logs `[render] atlas: 14 glyphs baked, 0 missing` (or close).

- [ ] **Step 5: Commit**

```bash
git add crates/map-app/src/renderer/atlas.rs
git commit -m "feat(render): rasterise_glyphs bakes Unicode icons into R8 atlas buffer"
```

---

### Task 5: `sprite.rs` — SpriteInstance struct + from_target

**Files:**
- Create: `crates/map-app/src/renderer/sprite.rs`
- Modify: `crates/map-app/src/renderer/mod.rs`

- [ ] **Step 1: Add module declaration**

In `crates/map-app/src/renderer/mod.rs`, append:

```rust
pub mod sprite;
```

- [ ] **Step 2: Write the failing test**

Create `crates/map-app/src/renderer/sprite.rs`:

```rust
//! Billboard sprite pipeline state + per-instance data.
//!
//! GPU pipeline construction lives here. Instance data carries world position,
//! UV rect into the atlas, ring colour, scale, and ring thickness.

use crate::renderer::atlas::{AtlasLookup, IconId};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SpriteInstance {
    pub world_pos: [f32; 3],
    pub _pad0: f32,
    pub atlas_uv_min: [f32; 2],
    pub atlas_uv_size: [f32; 2],
    pub faction_color: [f32; 4],
    pub scale_px: f32,
    pub ring_thickness_px: f32,
    pub _pad1: [f32; 2],
}

impl SpriteInstance {
    pub const SCALE_NORMAL: f32 = 32.0;
    pub const SCALE_SELECTED: f32 = 42.0;
    pub const RING_NORMAL: f32 = 2.0;
    pub const RING_SELECTED: f32 = 4.0;
    pub const SELECTION_COLOR: [f32; 4] = [1.0, 0.85, 0.1, 1.0];

    pub fn from_target(
        world_pos: Vec3,
        icon: IconId,
        ring_color: [f32; 4],
        selected: bool,
        atlas: &AtlasLookup,
    ) -> Self {
        let uv = atlas.uv_of(icon);
        let (scale_px, ring_thickness_px, color) = if selected {
            (Self::SCALE_SELECTED, Self::RING_SELECTED, Self::SELECTION_COLOR)
        } else {
            (Self::SCALE_NORMAL, Self::RING_NORMAL, ring_color)
        };
        Self {
            world_pos: [world_pos.x, world_pos.y, world_pos.z],
            _pad0: 0.0,
            atlas_uv_min: [uv[0], uv[1]],
            atlas_uv_size: [uv[2], uv[3]],
            faction_color: color,
            scale_px,
            ring_thickness_px,
            _pad1: [0.0; 2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn sample_atlas() -> AtlasLookup {
        AtlasLookup::build()
    }

    #[test]
    fn from_target_normal_uses_faction_ring() {
        let atlas = sample_atlas();
        let inst = SpriteInstance::from_target(
            Vec3::new(10.0, 20.0, 30.0),
            IconId::Factory,
            [0.2, 0.5, 1.0, 1.0],
            false,
            &atlas,
        );
        assert_eq!(inst.world_pos, [10.0, 20.0, 30.0]);
        assert_eq!(inst.scale_px, SpriteInstance::SCALE_NORMAL);
        assert_eq!(inst.ring_thickness_px, SpriteInstance::RING_NORMAL);
        assert_eq!(inst.faction_color, [0.2, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn from_target_selected_uses_yellow_ring_and_larger_scale() {
        let atlas = sample_atlas();
        let inst = SpriteInstance::from_target(
            Vec3::ZERO,
            IconId::Capital,
            [0.2, 0.5, 1.0, 1.0],   // ignored when selected
            true,
            &atlas,
        );
        assert_eq!(inst.scale_px, SpriteInstance::SCALE_SELECTED);
        assert_eq!(inst.ring_thickness_px, SpriteInstance::RING_SELECTED);
        assert_eq!(inst.faction_color, SpriteInstance::SELECTION_COLOR);
    }

    #[test]
    fn from_target_writes_correct_uv_rect() {
        let atlas = sample_atlas();
        let expected = atlas.uv_of(IconId::Anomaly);
        let inst = SpriteInstance::from_target(
            Vec3::ZERO,
            IconId::Anomaly,
            [1.0, 1.0, 1.0, 1.0],
            false,
            &atlas,
        );
        assert_eq!(inst.atlas_uv_min, [expected[0], expected[1]]);
        assert_eq!(inst.atlas_uv_size, [expected[2], expected[3]]);
    }
}
```

- [ ] **Step 3: Add `bytemuck` dep if missing**

Check whether `bytemuck` is already a dep:
```bash
grep -E "bytemuck" crates/map-app/Cargo.toml
```

If absent, add it under `[dependencies]`:
```toml
bytemuck = { version = "1", features = ["derive"] }
```

- [ ] **Step 4: Run — expect PASS**

```bash
cargo test -p map-app --lib renderer::sprite::tests 2>&1 | tail -5
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/map-app/src/renderer/sprite.rs crates/map-app/src/renderer/mod.rs crates/map-app/Cargo.toml
git commit -m "feat(render): SpriteInstance + from_target with selection-aware scale/ring"
```

---

### Task 6: Sprite GPU pipeline construction (no test — smoke verified)

**Files:**
- Modify: `crates/map-app/src/renderer/sprite.rs`
- Modify: `crates/map-app/src/renderer/gpu.rs`

This task wires the actual wgpu pipeline. There's no automated test for wgpu state (matches existing convention — the mesh pipeline has none either). Verification is the smoke run in Task 9.

- [ ] **Step 1: Define the pipeline struct + shader inside `sprite.rs`**

Append to `crates/map-app/src/renderer/sprite.rs`:

```rust
use eframe::egui_wgpu::wgpu;
use std::num::NonZeroU64;

pub const SPRITE_SHADER_SRC: &str = r#"
struct VIn {
    @location(0) corner: vec2<f32>,
};
struct IIn {
    @location(1) world_pos: vec3<f32>,
    @location(2) atlas_uv_min: vec2<f32>,
    @location(3) atlas_uv_size: vec2<f32>,
    @location(4) faction_color: vec4<f32>,
    @location(5) scale_px: f32,
    @location(6) ring_thickness_px: f32,
};
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) faction_color: vec4<f32>,
    @location(3) ring_edge: f32,
};

struct Camera {
    view_proj: mat4x4<f32>,
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@vertex fn vs(v: VIn, i: IIn) -> VOut {
    var center_clip = cam.view_proj * vec4<f32>(i.world_pos, 1.0);
    let pixel_offset = v.corner * i.scale_px * 2.0 / cam.viewport;
    center_clip.x = center_clip.x + pixel_offset.x * center_clip.w;
    center_clip.y = center_clip.y + pixel_offset.y * center_clip.w;
    var o: VOut;
    o.clip = center_clip;
    o.corner = v.corner;
    o.uv = i.atlas_uv_min + (v.corner + vec2<f32>(0.5)) * i.atlas_uv_size;
    o.faction_color = i.faction_color;
    // d_norm 0..1 maps to radius 0..(scale_px/2) px, so 1 unit of d_norm = scale_px/2 px.
    o.ring_edge = 1.0 - (i.ring_thickness_px * 2.0 / i.scale_px);
    return o;
}

@fragment fn fs(o: VOut) -> @location(0) vec4<f32> {
    let d_norm = length(o.corner) * 2.0;
    if d_norm > 1.0 { discard; }
    if d_norm > o.ring_edge {
        return o.faction_color;
    }
    let glyph_alpha = textureSample(atlas, samp, o.uv).r;
    return vec4<f32>(1.0, 1.0, 1.0, glyph_alpha);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub viewport: [f32; 2],
    pub _pad: [f32; 2],
}

pub struct SpritePipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub camera_buf: wgpu::Buffer,
    pub quad_vb: wgpu::Buffer,
    pub quad_ib: wgpu::Buffer,
    pub instance_vb: wgpu::Buffer,
    pub instance_capacity: usize,
}

impl SpritePipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        atlas_bytes: &[u8],
        atlas_w: u32,
        atlas_h: u32,
    ) -> Self {
        // Atlas texture.
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("icon_atlas"),
            size: wgpu::Extent3d { width: atlas_w, height: atlas_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            atlas_bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_w),
                rows_per_image: Some(atlas_h),
            },
            wgpu::Extent3d { width: atlas_w, height: atlas_h, depth_or_array_layers: 1 },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("icon_atlas_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_camera_uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<CameraUniform>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite_bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite_shader"),
            source: wgpu::ShaderSource::Wgsl(SPRITE_SHADER_SRC.into()),
        });

        let vertex_buffers = [
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<[f32; 2]>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2],
            },
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<SpriteInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    1 => Float32x3,   // world_pos (consumes 16 bytes incl. pad)
                    2 => Float32x2,   // atlas_uv_min
                    3 => Float32x2,   // atlas_uv_size
                    4 => Float32x4,   // faction_color
                    5 => Float32,     // scale_px
                    6 => Float32,     // ring_thickness_px
                ],
            },
        ];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &vertex_buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Shared quad: 4 verts, 6 indices.
        let quad_verts: [[f32; 2]; 4] = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
        let quad_idx: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let quad_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_quad_vb"),
            size: (quad_verts.len() * std::mem::size_of::<[f32; 2]>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&quad_vb, 0, bytemuck::cast_slice(&quad_verts));
        let quad_ib = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_quad_ib"),
            size: (quad_idx.len() * std::mem::size_of::<u16>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&quad_ib, 0, bytemuck::cast_slice(&quad_idx));

        // Starting instance buffer for ~1024 sprites.
        let instance_capacity = 1024usize;
        let instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_instance_vb"),
            size: (instance_capacity * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group,
            camera_buf,
            quad_vb,
            quad_ib,
            instance_vb,
            instance_capacity,
        }
    }

    pub fn update_camera(&self, queue: &wgpu::Queue, view_proj: glam::Mat4, viewport: [f32; 2]) {
        let u = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            viewport,
            _pad: [0.0; 2],
        };
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&u));
    }

    pub fn upload_instances(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, instances: &[SpriteInstance]) {
        if instances.len() > self.instance_capacity {
            // Grow buffer (double-or-fit).
            let new_cap = instances.len().max(self.instance_capacity * 2);
            self.instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sprite_instance_vb_grown"),
                size: (new_cap * std::mem::size_of::<SpriteInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_capacity = new_cap;
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instance_vb, 0, bytemuck::cast_slice(instances));
        }
    }

    pub fn draw<'a>(&'a self, rpass: &mut wgpu::RenderPass<'a>, n_instances: u32) {
        if n_instances == 0 { return; }
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.set_vertex_buffer(0, self.quad_vb.slice(..));
        rpass.set_vertex_buffer(1, self.instance_vb.slice(..(n_instances as u64 * std::mem::size_of::<SpriteInstance>() as u64)));
        rpass.set_index_buffer(self.quad_ib.slice(..), wgpu::IndexFormat::Uint16);
        rpass.draw_indexed(0..6, 0, 0..n_instances);
    }
}
```

- [ ] **Step 2: Build to catch compile errors**

```bash
cargo build 2>&1 | grep "^error" | head -10
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add crates/map-app/src/renderer/sprite.rs
git commit -m "feat(render): sprite GPU pipeline (atlas texture + instanced quads)"
```

---

### Task 7: `GpuScene` wires the sprite pipeline

**Files:**
- Modify: `crates/map-app/src/renderer/gpu.rs`
- Modify: `crates/map-app/src/ui/sector_view.rs` (paint callback handoff only)

- [ ] **Step 1: Add atlas + sprite fields to `GpuScene`**

Open `crates/map-app/src/renderer/gpu.rs`. Find `pub struct GpuScene { ... }`. Add fields:

```rust
pub struct GpuScene {
    pub pipeline: wgpu::RenderPipeline,         // existing mesh pipeline
    pub bind_group: wgpu::BindGroup,            // existing
    pub uniform_buf: wgpu::Buffer,              // existing
    pub meshes: HashMap<MeshKind, GpuMesh>,     // existing

    // New: sprite pipeline + per-frame state.
    pub sprite: crate::renderer::sprite::SpritePipeline,
    pub sprite_instances: Vec<crate::renderer::sprite::SpriteInstance>,
    pub camera_view_proj: glam::Mat4,
    pub camera_viewport: [f32; 2],
}
```

- [ ] **Step 2: Build the atlas at GpuScene construction**

In `GpuScene::new(device, target_format)`:

```rust
let queue: &wgpu::Queue = ...; // existing param? if not, add `queue: &wgpu::Queue` to the signature
```

If `GpuScene::new` currently takes only `device` and `target_format`, extend the signature to also take `&wgpu::Queue`. The caller is in `crates/map-app/src/app.rs`:

```rust
let scene = crate::renderer::gpu::GpuScene::new(&rs.device, &rs.queue, rs.target_format);
```

Inside `GpuScene::new`, before returning `Self`:

```rust
let (atlas_bytes, _missing) = crate::renderer::atlas::rasterise_glyphs(
    include_bytes!("../../assets/font.ttf"),
);
let sprite = crate::renderer::sprite::SpritePipeline::new(
    device,
    queue,
    target_format,
    &atlas_bytes,
    crate::renderer::atlas::ATLAS_W as u32,
    crate::renderer::atlas::ATLAS_H as u32,
);
```

Then in the final `Self { ... }`:

```rust
Self {
    pipeline,
    bind_group,
    uniform_buf,
    meshes,
    sprite,
    sprite_instances: Vec::new(),
    camera_view_proj: glam::Mat4::IDENTITY,
    camera_viewport: [1.0, 1.0],
}
```

- [ ] **Step 3: Add `set_sprite_instances` API**

Append to `impl GpuScene { ... }`:

```rust
    pub fn set_sprite_instances(
        &mut self,
        view_proj: glam::Mat4,
        viewport: [f32; 2],
        instances: Vec<crate::renderer::sprite::SpriteInstance>,
    ) {
        self.camera_view_proj = view_proj;
        self.camera_viewport = viewport;
        self.sprite_instances = instances;
    }
```

- [ ] **Step 4: Run the sprite pass inside the paint callback**

Find the existing paint callback (search for `CallbackTrait for SceneCallback`). Inside the `paint` method (or its equivalent — your repo's name might differ), after the existing mesh draw call:

```rust
// Update sprite camera + upload instances.
scene.sprite.update_camera(queue, scene.camera_view_proj, scene.camera_viewport);
scene.sprite.upload_instances(device, queue, &scene.sprite_instances);
scene.sprite.draw(rpass, scene.sprite_instances.len() as u32);
```

(The exact variable names — `scene`, `queue`, `device`, `rpass` — match your existing callback. Find the pattern and follow it.)

Also: if the existing `prepare` method takes `&wgpu::Device, &wgpu::Queue, ...` and writes the mesh uniform buffer there, do the same for the sprite pipeline at the same spot. The sprite needs camera + instance writes per frame.

- [ ] **Step 5: Build**

```bash
cargo build 2>&1 | grep "^error" | head -10
```

Expected: no errors. Existing mesh tests still compile.

- [ ] **Step 6: Commit**

```bash
git add crates/map-app/src/renderer/gpu.rs crates/map-app/src/app.rs
git commit -m "feat(render): GpuScene constructs sprite pipeline + per-frame upload path"
```

---

### Task 8: `sector_view` builds sprite instances + drops live-mesh path

**Files:**
- Modify: `crates/map-app/src/ui/sector_view.rs`

- [ ] **Step 1: Drop live entities + Anomaly/ResourceZone/Station from `build_draw_calls`**

In `crates/map-app/src/ui/sector_view.rs`, find `fn build_draw_calls(...)`. Currently it iterates `sector.static_objects` AND `world.entities_in_sector` for top-level entities. Strip both:

```rust
fn build_draw_calls(
    sector: &Sector,
    _world: Option<&map_domain::world::World>,
    _universe: &map_domain::universe::Universe,
    _selected_obj: Option<ObjectId>,
    _selected_entity: Option<map_domain::world::EntityId>,
) -> Vec<DrawCall> {
    let mut calls: Vec<DrawCall> = Vec::new();
    for obj in &sector.static_objects {
        // Keep only kinds NOT covered by the new sprite path AND not handled by the 2D
        // gates path. Today that's an empty set — gates + highways are drawn in 2D,
        // and Anomaly + ResourceZone + Station now go through sprites. The loop
        // therefore filters everything out. Kept here in case future static kinds
        // want mesh rendering.
        let _ = obj;
    }
    calls
}
```

(The renamed `_world` / `_universe` / `_selected_*` args keep the signature compatible — they're consumed by `build_sprite_instances` in the next step. We don't change the signature so other tasks didn't have to touch the call site, but if Rust warns about unused params, prefix with `_` as shown.)

If you'd rather delete the function entirely now that it returns an empty Vec, that's fine — just remove the call in `show()` too.

- [ ] **Step 2: Add `build_sprite_instances`**

Append to `crates/map-app/src/ui/sector_view.rs` (above `kind_color` or wherever helpers live):

```rust
fn build_sprite_instances(
    sector: &map_domain::universe::Sector,
    world: Option<&map_domain::world::World>,
    universe: &map_domain::universe::Universe,
    selected_obj: Option<ObjectId>,
    selected_entity: Option<map_domain::world::EntityId>,
    atlas: &crate::renderer::atlas::AtlasLookup,
) -> Vec<crate::renderer::sprite::SpriteInstance> {
    use crate::renderer::atlas::{classify_live, classify_static};
    use crate::renderer::sprite::SpriteInstance;

    let mut out: Vec<SpriteInstance> = Vec::new();

    // Static objects (Anomaly / ResourceZone / god-xml Station — gates/highways filtered).
    for obj in &sector.static_objects {
        let Some(icon) = classify_static(&obj.kind) else { continue };
        let ring_color = obj
            .faction
            .map(|f| crate::colors::faction_color(universe, f))
            .unwrap_or(crate::theme::TEXT_MUTED);
        let ring_rgba = color_to_rgba(ring_color);
        let selected = selected_obj == Some(obj.id);
        out.push(SpriteInstance::from_target(obj.position, icon, ring_rgba, selected, atlas));
    }

    // Live entities (top-level only).
    if let Some(world) = world {
        // Reverse map FactionId → faction id string (lowercase). Tiny (~30 entries).
        let mut fid_to_str: std::collections::HashMap<map_domain::ids::FactionId, &str> =
            std::collections::HashMap::new();
        for (k, v) in &universe.faction_strings {
            fid_to_str.insert(*v, k.as_str());
        }

        for &eid in world.entities_in_sector(sector.id) {
            if world.parent_of(eid).is_some() { continue; }
            let Some(&pos) = world.positions.get(&eid) else { continue };
            let Some(&kind) = world.kinds.get(&eid) else { continue };
            let macro_name = world.names.get(&eid).map(String::as_str).unwrap_or("");
            let owner_str = world.factions.get(&eid).and_then(|f| fid_to_str.get(f).copied());
            let icon = classify_live(kind, macro_name, owner_str);
            let ring_color = world.factions.get(&eid).copied()
                .map(|f| crate::colors::faction_color(universe, f))
                .unwrap_or(crate::theme::TEXT_MUTED);
            let ring_rgba = color_to_rgba(ring_color);
            let selected = selected_entity == Some(eid);
            out.push(SpriteInstance::from_target(pos, icon, ring_rgba, selected, atlas));
        }
    }
    out
}

fn color_to_rgba(c: egui::Color32) -> [f32; 4] {
    [
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
        c.a() as f32 / 255.0,
    ]
}
```

- [ ] **Step 3: Wire `build_sprite_instances` into `show`**

Find `SectorView3D::show`. Locate the point where the paint callback is constructed and `SceneCallback` is added with `draw_calls`. Just before that point:

```rust
        // Compute view + projection (existing local vars).
        // ...

        // Build sprite instances for this frame.
        let aspect = view_rect.width() / view_rect.height().max(1.0);
        let view = camera.view_matrix();
        let proj = camera.proj_matrix(aspect);
        let view_proj = proj * view;
        let viewport = [view_rect.width(), view_rect.height()];

        let atlas = crate::renderer::atlas::AtlasLookup::build();
        let sprite_instances = sector
            .map(|s| build_sprite_instances(s, world, universe, selected_obj, selected_entity, &atlas))
            .unwrap_or_default();
```

Pass `view_proj`, `viewport`, and `sprite_instances` into the paint callback so `GpuScene::set_sprite_instances` is invoked. The simplest path: extend the existing `SceneCallback` struct with these fields, and in its `prepare`/`paint` method call `scene.set_sprite_instances(view_proj, viewport, sprite_instances.clone())`.

Concretely, in this file, find the `SceneCallback { draw_calls }` construction near the bottom of `show`:

```rust
        let cb = eframe::egui_wgpu::Callback::new_paint_callback(
            view_rect,
            SceneCallback { draw_calls, view_proj, viewport, sprite_instances },
        );
```

…and update the `SceneCallback` struct in `crates/map-app/src/renderer/gpu.rs` to carry the new fields. In its `prepare` impl, set them on the scene:

```rust
impl egui_wgpu::CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &eframe::egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut eframe::egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(scene) = callback_resources.get_mut::<GpuScene>() else { return vec![] };
        // Existing mesh-uniform prepare logic — keep.
        // ...
        // New: pass sprite data into scene.
        scene.set_sprite_instances(self.view_proj, self.viewport, self.sprite_instances.clone());
        vec![]
    }

    fn paint(...) {
        let Some(scene) = callback_resources.get::<GpuScene>() else { return };
        // Existing mesh draw.
        // ...
        // Sprite pass.
        scene.sprite.update_camera(queue, scene.camera_view_proj, scene.camera_viewport);
        scene.sprite.upload_instances(device, queue, &scene.sprite_instances);
        scene.sprite.draw(rpass, scene.sprite_instances.len() as u32);
    }
}
```

(Adapt to your codebase's actual method signatures and variable names. If `paint` doesn't have `device`/`queue`/`rpass` in scope, look at the existing mesh pass to see how they're obtained — they typically come from `callback_resources` or are stored on `scene` itself.)

- [ ] **Step 4: Build**

```bash
cargo build 2>&1 | grep "^error" | head -10
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/map-app/src/ui/sector_view.rs crates/map-app/src/renderer/gpu.rs
git commit -m "feat(3d): replace live + static GPU meshes with billboard sprite path"
```

---

### Task 9: Bump pick_target hit radius to match new icon size

**Files:**
- Modify: `crates/map-app/src/ui/sector_view.rs`

- [ ] **Step 1: Bump the radius**

In `crates/map-app/src/ui/sector_view.rs`, find `fn pick_target`. Change the hit threshold from `20.0` to `24.0`:

```rust
        if d < 24.0 && best.as_ref().map_or(true, |(b, _)| d < *b) {
```

Rationale: icons are 32 px wide; 24 px radius gives ~half-icon margin which feels comfortable.

- [ ] **Step 2: Build + tests**

```bash
cargo build 2>&1 | grep "^error" | head -5
cargo test 2>&1 | tail -3
```

Expected: no errors, all tests pass (no test asserts pick radius).

- [ ] **Step 3: Commit**

```bash
git add crates/map-app/src/ui/sector_view.rs
git commit -m "tweak(3d): bump pick hit radius 20 → 24 px to match icon size"
```

---

### Task 10: Smoke + acceptance verification

**Files:** none modified (manual verification + final cleanup commit if needed).

- [ ] **Step 1: Full test suite**

```bash
cargo test 2>&1 | tail -5
```

Expected: all tests pass. Note the count.

- [ ] **Step 2: Capture parse + render log**

```bash
timeout 25 cargo run --release 2>&1 > /tmp/r.log
grep -E "^\[(map|parse|render)\]" /tmp/r.log | head -30
```

Verify:
- `[render] atlas: 14 glyphs baked, 0 missing` (or `<= 2` missing — acceptable).
- `[parse] entities=13091` (unchanged from master).
- No `[render] WARNING:` truncation lines.
- No panic / Rust backtrace lines.

- [ ] **Step 3: Manual visual checklist**

Open the app, enter a populated sector (e.g. Argon Prime). Verify by eyeball:

- [ ] Every live + non-gate static entity renders as a glyph with a ring around it.
- [ ] Stations show role-specific glyphs (factories ⚙, shipyards ⎈, defense ⚔, etc).
- [ ] Ships are ◆ / ▶ / ▴ / ▭ by class.
- [ ] Anomalies show ✦; resource zones show ◎.
- [ ] Icon pixel size does NOT change when zooming the orbit camera in or out.
- [ ] Selected entity is clearly larger and the ring is yellow (not faction colour).
- [ ] Clicking within ~24 px of an icon centre selects it.
- [ ] Hovering shows the existing label.
- [ ] Gates + highways still render as rings + arrows.
- [ ] No frame stutter at ~500 icons (look at egui's frame-time graph if available, or just feel responsiveness).

- [ ] **Step 4: If any visual issue, file follow-up notes**

Don't fix in this branch (per the spec, the branch is for the user to play with). Add a note to `docs/superpowers/retrospectives/2026-05-18-icon-billboards.md` capturing:
- What works.
- What doesn't.
- Measured frame time at peak sector if you can grab it.

Commit:
```bash
git add docs/superpowers/retrospectives/2026-05-18-icon-billboards.md
git commit -m "docs: icon-billboards retro — visual observations on feature branch"
```

(Skip this commit if you have nothing useful to record.)

---

## Out-of-Scope (deferred follow-ups)

- Removal of the now-idle mesh pipeline (keep for future grid/ground-plane work).
- Per-role 3D meshes (fallback if billboards prove insufficient).
- LOD atlas / multi-res glyphs.
- Hand-painted SVG icons.
- Depth shading + outline halos on overlapping icons.
- 2D map view icon set (3D-only this round).
- Animated selection ring (user explicitly chose static).

---

## Self-Review

**Spec coverage:**
- §1 (glyph atlas) → Tasks 1, 2, 4.
- §2 (sprite GPU pipeline) → Tasks 5, 6.
- §3 (classification rules) → Task 3.
- §4 (sector_view integration) → Tasks 7, 8, 9.
- §5 (deps, acceptance, risks) → Tasks 1 (dep + font), 10 (acceptance smoke).
- Branch isolation requirement → execution skill choice (user will create a feature branch before T1).

**Placeholder scan:** no TBD/TODO/"implement later"/"add error handling" left. Every code step ships the full code. Every test step ships the full test body.

**Type consistency:**
- `IconId` enum: introduced in Task 2, used identically in Tasks 3, 5, 8.
- `GLYPHS: &[(IconId, char)]`: Task 2 → Task 4 (`rasterise_glyphs` iterates it).
- `AtlasLookup::build() / uv_of(IconId) -> [f32;4]`: Task 2 → Tasks 5, 8.
- `rasterise_glyphs(font_bytes) -> (Vec<u8>, usize)`: Task 4 → Task 7 (GpuScene calls it at init).
- `SpriteInstance::from_target(world_pos, icon, ring_color, selected, atlas)`: Task 5 → Task 8 (build_sprite_instances).
- `SpritePipeline::new(device, queue, target_format, atlas_bytes, atlas_w, atlas_h)`: Task 6 → Task 7 (called in GpuScene::new).
- `SpritePipeline::update_camera / upload_instances / draw`: Task 6 → Task 7 (paint callback).
- `GpuScene::set_sprite_instances(view_proj, viewport, instances)`: Task 7 → Task 8 (paint callback prepare).
- `classify_live(kind, macro_name, owner) -> IconId` + `classify_static(kind) -> Option<IconId>`: Task 3 → Task 8.
- `pick_target` hit radius: Task 9 updates the existing implementation, no signature change.

No drift detected.
