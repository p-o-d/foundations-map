use glam::{Mat4, Vec3};

/// Bounding-box centre of a set of points — the "sector content centre" that
/// `fit_all` frames and that gate arrows point toward. Empty slice → origin.
pub fn content_center(positions: &[Vec3]) -> Vec3 {
    let Some(&first) = positions.first() else {
        return Vec3::ZERO;
    };
    let (mut lo, mut hi) = (first, first);
    for &p in positions {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (lo + hi) * 0.5
}

pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 100.0,
            // Default yaw = π puts the eye on the -Z (south) side looking north.
            // With the left-handed view/proj below this yields east→right,
            // north(+Z)→up — matching the 2D map + in-game orientation.
            yaw: std::f32::consts::PI,
            // Near top-down (80°) to mirror the in-game sector map: the flat
            // Y≈0 object plane is viewed almost perpendicular, so the N–S spread
            // is not foreshortened into a thin band. Clamp ceiling is 85°.
            pitch: 1.4,
        }
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + self.distance * Vec3::new(cp * sy, sp, cp * cy)
    }

    pub fn view_matrix(&self) -> Mat4 {
        // Left-handed to match X4's left-handed engine coords. Using *_rh here
        // mirror-flips the scene (parity flip) relative to the game.
        Mat4::look_at_lh(self.eye(), self.target, Vec3::Y)
    }

    pub fn proj_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_lh(60f32.to_radians(), aspect, 0.1, 2_000_000.0)
    }

    pub fn rotate(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(-85f32.to_radians(), 85f32.to_radians());
    }

    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (1.0 - delta * 0.1)).clamp(1.0, 5_000_000.0);
    }

    /// Frame the whole sector — orbit the *content* centre (bounding-box centre
    /// of the given objects) at a distance large enough to see them all. X4's
    /// sector map frames the gate/zone layout, which is generally offset from the
    /// sector macro origin (0,0,0); centring on the origin instead makes the
    /// whole sector look shifted. Use this when no specific selection drives the
    /// camera.
    pub fn fit_all(&mut self, positions: &[Vec3]) {
        if positions.is_empty() {
            self.target = Vec3::ZERO;
            self.distance = 100.0;
            self.yaw = std::f32::consts::PI;
            self.pitch = 1.4;
            return;
        }
        let center = content_center(positions);
        // Radius = farthest object from the content centre.
        let radius = positions
            .iter()
            .map(|p| (*p - center).length())
            .fold(0.0f32, f32::max);
        self.target = center;
        self.distance = ((radius + 1.0) / 30f32.to_radians().tan()).max(10.0);
        self.yaw = std::f32::consts::PI;
        // Match Default: near top-down so the sector plane fills the frame.
        self.pitch = 1.4;
    }

    /// Orbit around a single point (selected object/entity). Keeps yaw/pitch
    /// so the camera glides in rather than snapping orientation.
    pub fn focus_on(&mut self, point: Vec3) {
        self.target = point;
        self.distance = 30.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn default_eye_is_above_origin() {
        let cam = OrbitCamera::default();
        let eye = cam.eye();
        assert!(eye.y > 0.0, "eye must be above target");
        assert!(
            (eye - cam.target).length() > 0.0,
            "eye must not be at target"
        );
    }

    #[test]
    fn view_matrix_looks_at_target() {
        let cam = OrbitCamera::default();
        let view = cam.view_matrix();
        let t_view = view.transform_point3(cam.target);
        assert!(
            t_view.z > 0.0,
            "target must be in front (positive z in LH view)"
        );
    }

    #[test]
    fn proj_matrix_maps_center_to_zero() {
        let cam = OrbitCamera::default();
        let proj = cam.proj_matrix(16.0 / 9.0);
        // In glam's column-major perspective_lh, z_axis.w == 1.0 (the perspective divide term).
        // An orthographic matrix would have 0.0 there, so this confirms it's perspective.
        assert!(
            proj.z_axis.w != 0.0,
            "projection must be perspective (non-zero z_axis.w)"
        );
    }

    #[test]
    fn rotate_updates_yaw_pitch() {
        let mut cam = OrbitCamera::default();
        let old_yaw = cam.yaw;
        cam.rotate(0.1, 0.0);
        assert!((cam.yaw - old_yaw - 0.1).abs() < 1e-5);
    }

    #[test]
    fn pitch_clamps_to_avoid_gimbal() {
        let mut cam = OrbitCamera::default();
        cam.rotate(0.0, 10.0);
        assert!(cam.pitch <= 85f32.to_radians() + 1e-4);
    }

    #[test]
    fn zoom_changes_distance() {
        let mut cam = OrbitCamera::default();
        let old = cam.distance;
        cam.zoom(-1.0);
        assert!(cam.distance > old);
    }

    #[test]
    fn fit_all_centers_on_objects() {
        let mut cam = OrbitCamera::default();
        let pts = vec![Vec3::new(10.0, 0.0, 0.0), Vec3::new(-10.0, 0.0, 0.0)];
        cam.fit_all(&pts);
        assert!((cam.target - Vec3::ZERO).length() < 1e-3);
        assert!(
            cam.distance > 10.0,
            "must be far enough to see ±10 unit spread"
        );
    }

    #[test]
    fn fit_all_empty_resets_to_default() {
        let mut cam = OrbitCamera::default();
        cam.fit_all(&[]);
        assert_eq!(cam.target, Vec3::ZERO);
    }

    #[test]
    fn focus_on_targets_point() {
        let mut cam = OrbitCamera::default();
        let pt = Vec3::new(42.0, 5.0, -17.0);
        cam.focus_on(pt);
        assert!((cam.target - pt).length() < 1e-3);
        assert!(cam.distance > 0.0);
    }

    /// Helper: project a world point to screen NDC (y-up). Returns (ndc_x, ndc_y).
    fn ndc(cam: &OrbitCamera, p: Vec3) -> (f32, f32) {
        let vp = cam.proj_matrix(1.6) * cam.view_matrix();
        let clip = vp * p.extend(1.0);
        (clip.x / clip.w, clip.y / clip.w)
    }

    #[test]
    fn cardinal_directions_map_correctly() {
        // X4 axes: +X east, +Z north, +Y up. Screen: +ndc_x right, +ndc_y up.
        let mut cam = OrbitCamera::default();
        // Symmetric points so the content centre is the origin.
        cam.fit_all(&[Vec3::new(100.0, 0.0, 100.0), Vec3::new(-100.0, 0.0, -100.0)]);
        let (nx, ny) = ndc(&cam, Vec3::new(0.0, 0.0, 50.0)); // north
        let (sx, sy) = ndc(&cam, Vec3::new(0.0, 0.0, -50.0)); // south
        let (ex, _ey) = ndc(&cam, Vec3::new(50.0, 0.0, 0.0)); // east
        let (wx, _wy) = ndc(&cam, Vec3::new(-50.0, 0.0, 0.0)); // west
        assert!(ny > 0.0, "north (+Z) must be above center, got ndc_y={ny}");
        assert!(sy < 0.0, "south (-Z) must be below center, got ndc_y={sy}");
        assert!(
            ex > 0.0,
            "east (+X) must be right of center, got ndc_x={ex}"
        );
        assert!(wx < 0.0, "west (-X) must be left of center, got ndc_x={wx}");
        assert!(
            nx.abs() < 0.05 && sx.abs() < 0.05,
            "N/S must stay horizontally centered"
        );
    }

    #[test]
    fn fit_all_centres_on_content_not_origin() {
        // X4 frames the content, which is usually offset from the macro origin.
        // Two objects clustered to the NE must orbit their midpoint, not (0,0,0).
        let mut cam = OrbitCamera::default();
        cam.fit_all(&[Vec3::new(120.0, 0.0, 140.0), Vec3::new(100.0, 0.0, 160.0)]);
        assert!((cam.target - Vec3::new(110.0, 0.0, 150.0)).length() < 1e-3);
    }
}
