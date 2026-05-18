//! Billboard sprite pipeline state + per-instance data.
//!
//! GPU pipeline construction added in T6. This file currently exposes the
//! per-instance struct and the `from_target` helper.

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
            [0.2, 0.5, 1.0, 1.0],
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
