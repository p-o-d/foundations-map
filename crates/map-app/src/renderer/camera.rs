use glam::{Mat4, Vec3};

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
            yaw: 0.0,
            pitch: 0.3,
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
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Y)
    }

    pub fn proj_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(60f32.to_radians(), aspect, 0.1, 2_000_000.0)
    }

    pub fn rotate(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(-85f32.to_radians(), 85f32.to_radians());
    }

    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (1.0 - delta * 0.1)).clamp(1.0, 5_000_000.0);
    }

    /// Frame the whole sector — orbit the sector centre at a distance large
    /// enough to see every object. Use this when no specific selection should
    /// drive the camera.
    pub fn fit_all(&mut self, positions: &[Vec3]) {
        if positions.is_empty() {
            self.target = Vec3::ZERO;
            self.distance = 100.0;
            return;
        }
        let max_r = positions.iter().map(|p| p.length()).fold(0.0f32, f32::max);
        self.target = Vec3::ZERO;
        self.distance = ((max_r + 1.0) / 30f32.to_radians().tan()).max(10.0);
        self.yaw = 0.0;
        self.pitch = 0.3;
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
            t_view.z < 0.0,
            "target must be in front (negative z in RH view)"
        );
    }

    #[test]
    fn proj_matrix_maps_center_to_zero() {
        let cam = OrbitCamera::default();
        let proj = cam.proj_matrix(16.0 / 9.0);
        // In glam's column-major perspective_rh, z_axis.w == -1.0 (the perspective divide term).
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

    #[test]
    fn fit_all_with_single_position_still_centres_on_sector_origin() {
        // Regression: fit_all is the "frame sector" path, NOT a select-object
        // path. A sector with a single object (e.g. only one gate) must still
        // orbit Vec3::ZERO, not the object.
        let mut cam = OrbitCamera::default();
        cam.fit_all(&[Vec3::new(100.0, 0.0, 0.0)]);
        assert!((cam.target - Vec3::ZERO).length() < 1e-3);
    }
}
