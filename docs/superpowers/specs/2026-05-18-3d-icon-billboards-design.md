# 3D View — Icon Billboards

**Date:** 2026-05-18
**Status:** spec (approved, pre-plan)
**Branch:** to be implemented on a feature branch; **do not merge to master** until visually validated by the user.
**Trigger:** Current 3D view renders every entity as a generic GPU box or sphere with faction tint. User cannot tell a wharf from a factory from a destroyer at a glance, and the yellow selection tint is easy to miss in dense scenes. Goal: replace the boxes/spheres with type-revealing icons and make selection unmissable.

## Goals

1. Every top-level live entity + non-gate static object renders as a 2D billboard icon facing the camera, at constant pixel size, with a faction-coloured ring outside a white glyph.
2. The glyph shows the entity's role (factory / wharf / defense / capital / fighter / anomaly / etc.) so the user can read a sector without hovering.
3. Selection is unmistakable: 1.3× scale + bright yellow ring instead of faction ring.
4. Implementation must scale to hundreds of icons per sector via a single instanced GPU draw call.

## Non-Goals

- Hand-painted SVG icon library (Unicode glyphs sufficient for v1).
- Per-role 3D meshes (would replace billboards entirely — deferred fallback if billboards prove insufficient).
- LOD / multi-resolution atlas (constant-pixel sizing already covers zoom range).
- Depth shading, outline halos on overlap, or other visual polish beyond ring + glyph.
- Changes to 2D map view, side panel, gate rendering, or hover labels.

## Architecture Summary

```
┌──────────────────────────────────────────────────────────────┐
│ Startup (GpuScene::new)                                       │
│  ab_glyph rasterizes each IconId's char at 48 px →            │
│   256×128 R8 wgpu::Texture (the atlas)                        │
│   IconId → uv_rect table built alongside                      │
└──────────────────────┬───────────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────────────┐
│ Per-frame (SectorView3D::show)                                │
│  build_sprite_instances(sector, world, universe, sel_*) →     │
│    Vec<SpriteInstance>                                        │
│  GpuScene::set_sprite_instances(...)                          │
│  one instanced draw call inside the existing paint callback   │
└──────────────────────────────────────────────────────────────┘
```

`GpuScene` keeps the existing mesh pipeline (used by nothing once the change lands, but small enough to keep for future use — see Out of Scope). New `sprite` pipeline lives alongside.

## Section 1 — Glyph atlas

### Crate addition

```toml
# crates/map-app/Cargo.toml
ab_glyph = "0.2"
```

### Embedded font

`crates/map-app/assets/font.ttf` — embed `DejaVuSansMono.ttf` (Bitstream Vera derivative, public-domain). Loaded via `include_bytes!("../../assets/font.ttf")`.

If DejaVuSansMono lacks any of the glyphs (verify at bake time), substitute with `NotoSansSymbols2-Regular.ttf` (SIL OFL) — keep one font, whichever covers all chosen glyphs.

### IconId enum + glyph map

```rust
// crates/map-app/src/renderer/atlas.rs

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
```

### Atlas layout

- Tile size: **48×48 px** per glyph.
- Atlas grid: **8 columns × 3 rows = 24 slots** (14 used).
- Final atlas dimensions: **384×144 px**, stored as R8Unorm.
- Total GPU memory: 54 KB. Negligible.

### Baking

At `GpuScene::new`:

```rust
let font = ab_glyph::FontRef::try_from_slice(FONT_BYTES).unwrap();
let mut buf = vec![0u8; ATLAS_W * ATLAS_H];
let mut uv_rects: HashMap<IconId, [f32; 4]> = HashMap::new();
let scale = ab_glyph::PxScale::from(40.0);  // 48-px tile, glyph fills ~40 px centred

for (idx, (icon, ch)) in GLYPHS.iter().enumerate() {
    let col = idx % ATLAS_COLS;
    let row = idx / ATLAS_COLS;
    let glyph_id = font.glyph_id(*ch);
    if glyph_id.0 == 0 {
        eprintln!("[render] atlas: glyph {:?} for {:?} missing in font; will render blank", ch, icon);
    } else {
        // rasterize using ab_glyph's scale_font + outline_glyph; copy alpha into buf at (col*48, row*48).
        // Center glyph in its 48×48 tile.
    }
    let u0 = (col * TILE_PX) as f32 / ATLAS_W as f32;
    let v0 = (row * TILE_PX) as f32 / ATLAS_H as f32;
    let du = TILE_PX as f32 / ATLAS_W as f32;
    let dv = TILE_PX as f32 / ATLAS_H as f32;
    uv_rects.insert(*icon, [u0, v0, du, dv]);
}

let texture = device.create_texture(&wgpu::TextureDescriptor {
    label: Some("icon_atlas"),
    size: wgpu::Extent3d { width: ATLAS_W as u32, height: ATLAS_H as u32, depth_or_array_layers: 1 },
    mip_level_count: 1,
    sample_count: 1,
    dimension: wgpu::TextureDimension::D2,
    format: wgpu::TextureFormat::R8Unorm,
    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    view_formats: &[],
});
queue.write_texture(/* ... buf into texture ... */);

eprintln!("[render] atlas: {} glyphs baked, {} missing", baked, missing);
```

### Tests

- `glyph_table_covers_every_icon_id` — every `IconId` variant present in `GLYPHS`.
- `atlas_layout_assigns_unique_uv_rects_per_icon` — sanity on the rect builder.

Visual atlas inspection deferred to manual smoke (look at the icons in-app).

## Section 2 — Sprite GPU pipeline

### Shader (`sprite.wgsl`)

```wgsl
struct VIn {
    @location(0) corner: vec2<f32>,            // -0.5..+0.5
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
    @location(3) ring_edge: f32,               // pre-computed: 1 - thickness/scale
};

struct Camera {
    view_proj: mat4x4<f32>,
    viewport: vec2<f32>,                       // px
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
    // Ring band thickness in normalised units = ring_thickness_px / (scale_px/2).
    o.ring_edge = 1.0 - (i.ring_thickness_px * 2.0 / i.scale_px);
    return o;
}

@fragment fn fs(o: VOut) -> @location(0) vec4<f32> {
    let d_norm = length(o.corner) * 2.0;       // 0 centre → 1 quad edge
    if d_norm > 1.0 { discard; }
    if d_norm > o.ring_edge {
        return o.faction_color;
    }
    let glyph_alpha = textureSample(atlas, samp, o.uv).r;
    return vec4<f32>(1.0, 1.0, 1.0, glyph_alpha);
}
```

### Pipeline + buffers

- Bind group layout: uniform `Camera` (binding 0), texture (binding 1), sampler (binding 2).
- Vertex layout: per-vertex `corner: vec2`, per-instance `world_pos / atlas_uv_min / atlas_uv_size / faction_color / scale_px / ring_thickness_px`.
- Shared quad index buffer: 6 indices (two triangles).
- Instance buffer reallocated when capacity needs to grow; otherwise reused via `queue.write_buffer`.
- Blending: standard alpha — `SrcAlpha`, `OneMinusSrcAlpha`.
- No depth attachment (matches existing mesh pipeline + egui paint callback constraint).

### Single draw call

```rust
rpass.set_pipeline(&self.sprite_pipeline);
rpass.set_bind_group(0, &self.sprite_bind_group, &[]);
rpass.set_vertex_buffer(0, self.sprite_quad_vb.slice(..));
rpass.set_vertex_buffer(1, self.sprite_instance_vb.slice(..n_bytes));
rpass.set_index_buffer(self.sprite_quad_ib.slice(..), wgpu::IndexFormat::Uint16);
rpass.draw_indexed(0..6, 0, 0..(n_instances as u32));
```

### Tests

- Pipeline construction is verified by app smoke run only (no automated tests for wgpu pipelines in this repo; matches existing convention).
- Unit test on `SpriteInstance::from_target` exists to assert atlas UV resolution + scale/ring values for normal vs selected entities.

## Section 3 — Classification rules

`atlas.rs` exports:

```rust
pub fn classify_live(
    kind: LiveObjectKind,
    macro_name: &str,
    owner: Option<&str>,
) -> IconId;

pub fn classify_static(kind: &StaticObjectKind) -> Option<IconId>;
```

Body:

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

### Tests

- `classify_live_routes_factory_macro_to_factory_icon`
- `classify_live_player_owner_wins_over_macro` (owner=player + macro="argon_factory" → PlayerStation)
- `classify_live_transport_keyword_overrides_size` (kind=ShipLarge + macro="ship_arg_l_freighter_01" → Transport)
- `classify_static_returns_none_for_gates_and_highways`
- `classify_static_returns_generic_for_station_variant`

## Section 4 — sector_view integration

### Files modified

- `crates/map-app/src/ui/sector_view.rs` — `build_draw_calls` drops live-entity + Anomaly/ResourceZone/Station mesh paths. New `build_sprite_instances` populates the sprite pipeline. `pick_target` hit radius bumped from 20 → 24 px to match the 32 px icon (half quad + small margin).
- `crates/map-app/src/renderer/gpu.rs` — `GpuScene` grows `sprite_*` fields + `set_sprite_instances` method; paint callback runs the existing mesh pass (now only used for gates indirectly — see note below) followed by the sprite pass.

### Behaviour

- Gates + highways: rendered exactly as today by `draw_gates_2d` (no change).
- Anomaly + ResourceZone: become sprite icons (no more sphere mesh).
- Live stations + ships: become sprite icons (no more box/sphere mesh).
- Static `<station>` from god.xml (only `parse_god_xml` `<object>` parent — usually 0 today): would become GenericStation icon if any survive.

### Result

After the change, the GPU mesh pipeline emits **zero** draw calls in a populated sector. The pipeline is kept in place for future use (depth-cued grid plane, etc.) but the per-frame mesh draw becomes a no-op. Acceptable; not optimised away in this task.

### Sprite instance helper

```rust
impl SpriteInstance {
    pub fn from_target(
        world_pos: Vec3,
        icon: IconId,
        faction_color: egui::Color32,
        selected: bool,
        atlas: &AtlasLookup,
    ) -> Self {
        let uv = atlas.uv(icon);
        let (scale_px, ring_thickness_px, ring_color) = if selected {
            (
                42.0,
                4.0,
                [1.0, 0.85, 0.1, 1.0],   // yellow override
            )
        } else {
            (
                32.0,
                2.0,
                color_to_rgba(faction_color),
            )
        };
        Self {
            world_pos,
            atlas_uv_min: [uv[0], uv[1]],
            atlas_uv_size: [uv[2], uv[3]],
            faction_color: ring_color,
            scale_px,
            ring_thickness_px,
            _pad: [0.0; 2],
        }
    }
}
```

## Section 5 — Dependencies, acceptance, risks

### Dependencies

- New crate dep: `ab_glyph = "0.2"`.
- New embedded asset: `crates/map-app/assets/font.ttf` (~700 KB). Add `crates/map-app/assets/` to the repo. License attribution noted in commit message.

### Acceptance Criteria

- [ ] Startup logs: `[render] atlas: 14 glyphs baked, 0 missing`. If any glyph is missing, the warning fires and the affected entity falls back to a blank tile (alpha 0) — visible as a faction-coloured ring with no inner glyph.
- [ ] Opening a populated sector (e.g. Argon Prime) shows every live + non-gate-static entity as an icon with faction-coloured ring.
- [ ] Icon size is constant on zoom: visually verify the icon's pixel size does not change when the camera zooms in or out.
- [ ] Selected entity is 1.3× scale + yellow ring (NOT faction colour). Visibly distinct.
- [ ] Click within 24 px of an icon's centre selects it (existing pick logic, radius bumped).
- [ ] Hover label still functions (no changes to that code path).
- [ ] Gates + highways unchanged (rings + arrows).
- [ ] `cargo test` passes including the new classification + atlas tests.
- [ ] No `[render] WARNING: scene has N draw calls but GPU cap is M; truncating` lines (sprite pipeline uses its own buffer; mesh cap unchanged).
- [ ] Manual smoke: at ~500 icons in a busy sector, frame rate visibly steady (no stutter).

### Risks

- **Font glyph coverage.** `ab_glyph::Font::glyph_id` returns 0 for missing glyphs. Mitigation: log + leave the tile blank. Acceptable v1.
- **Atlas lifetime.** Built once at GpuScene init. If `cc.wgpu_render_state` is rebuilt (window recreation), the atlas is rebuilt with it. Verified by existing pattern with the mesh pipeline.
- **Owner string lookup for player.** `classify_live` needs the owner as a `&str`. `World.factions[eid]` is `FactionId`. `Universe.faction_strings` (string → FactionId) needs a reverse lookup. Cheap on ~30 factions; lookup happens only for stations. Mitigation: build a small `HashMap<FactionId, &str>` once per `build_sprite_instances` call.
- **Selection ring vs faction ring conflict.** Selection overrides faction ring colour. Faction is still readable from the hover label + side panel. Acceptable.
- **Mesh pipeline becomes idle.** Static + live mesh paths gone, only sprite + 2D drawing remain. Mesh pipeline kept in code for future grid / ground plane. Slight cruft; tagged for follow-up cleanup if grid never lands.

### Out of Scope (deferred)

- Per-role 3D meshes (fallback if icons prove insufficient on user testing).
- LOD atlas / multi-res glyphs.
- Hand-painted icons (Unicode glyphs sufficient for v1).
- Depth shading, outline halo on dense overlap.
- 2D map view icon set (this spec is 3D-only).
- Removal of the unused mesh pipeline (keep for future spatial features).
