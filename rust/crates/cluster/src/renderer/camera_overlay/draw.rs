use super::{
    projection, projection::source_rect, screen_xy, vehicle::vehicle_draw_key,
    CameraOverlayProjection, OverlayState,
};
use crate::{
    config::{DESIGN_HEIGHT, DESIGN_WIDTH},
    models::ClusterUiState,
    renderer::{
        tokens::{
            CAMERA_BACKGROUND_H, CAMERA_BACKGROUND_W, CAMERA_BACKGROUND_X, CAMERA_BACKGROUND_Y,
        },
        world::helpers::Bounds,
        ClusterUiRenderer, Point, Rect, Result,
    },
    scene::{ClusterScene, Vec3},
};
use num_traits::AsPrimitive;
impl ClusterUiRenderer {
    #[must_use]
    pub fn camera_overlay_state(&self) -> &OverlayState {
        &self.camera_overlay
    }
    pub fn camera_overlay_state_mut(&mut self) -> &mut OverlayState {
        &mut self.camera_overlay
    }
    #[must_use]
    pub fn camera_overlay_content_rect(&self) -> Rect {
        let design_width: f64 = DESIGN_WIDTH.as_();
        let design_height: f64 = DESIGN_HEIGHT.as_();
        let sx = self.width / design_width;
        let sy = self.height / design_height;
        source_rect(Rect::new(
            (CAMERA_BACKGROUND_X + self.driving_panel_offset_design_x()) * sx,
            CAMERA_BACKGROUND_Y * sy,
            CAMERA_BACKGROUND_W * sx,
            CAMERA_BACKGROUND_H * sy,
        ))
    }
    #[must_use]
    pub fn camera_overlay_projection(
        &self,
        state: &ClusterUiState,
    ) -> Option<CameraOverlayProjection> {
        Some(projection(
            state,
            self.camera_overlay_content_rect(),
            self.camera_overlay.wide,
            self.camera_overlay.pitch_offset_deg,
        ))
    }
    #[must_use]
    pub fn camera_overlay_screen_xy(
        &self,
        point: Vec3,
        view: &CameraOverlayProjection,
        shift: f64,
    ) -> Option<Point> {
        screen_xy(point, view, shift, self.camera_overlay.z_offset_m)
    }
    /// # Errors
    /// Returns projected strip, radar, vehicle or label drawing errors.
    pub fn draw_camera_projected_overlay(
        &mut self,
        scene: &ClusterScene,
        state: &ClusterUiState,
        view: Option<&CameraOverlayProjection>,
    ) -> Result<()> {
        let computed;
        let view = if let Some(view) = view {
            view
        } else {
            computed = self.camera_overlay_projection(state);
            let Some(view) = &computed else {
                return Ok(());
            };
            view
        };
        let dest = view.dest;
        self.gfx.begin_scissor(Rect::new(
            dest.x.round_ties_even(),
            dest.y.round_ties_even(),
            dest.width.round_ties_even(),
            dest.height.round_ties_even(),
        ))?;
        let result = self.draw_projected_scene(scene, state, view);
        self.gfx.end_scissor()?;
        result
    }
    fn draw_projected_scene(
        &mut self,
        scene: &ClusterScene,
        state: &ClusterUiState,
        view: &CameraOverlayProjection,
    ) -> Result<()> {
        let start = self.profile_start();
        for strip in scene
            .highlight_lanes
            .iter()
            .chain(&scene.road_edges)
            .chain(&scene.lane_markings)
            .chain(&scene.planned_path)
        {
            self.draw_camera_overlay_strip(strip, view, scene.scene_shift_x_m)?;
        }
        self.profile_add("render_world.camera_projected_overlay.strips", start);
        let start = self.profile_start();
        for point in &scene.radar_points {
            self.draw_camera_overlay_radar_point(
                point,
                view,
                scene.scene_shift_x_m,
                state.radar_info_mode,
            )?;
        }
        self.profile_add("render_world.camera_projected_overlay.radar", start);
        let start = self.profile_start();
        let mut vehicles: Vec<_> = scene.vehicles.iter().collect();
        vehicles.sort_by(|a, b| {
            let a = vehicle_draw_key(a);
            let b = vehicle_draw_key(b);
            a.0.cmp(&b.0)
                .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        });
        let mut labels = Vec::new();
        for vehicle in vehicles {
            self.draw_camera_overlay_vehicle(
                vehicle,
                view,
                scene.scene_shift_x_m,
                state.radar_info_mode,
                Some(&mut labels),
            )?;
        }
        self.draw_vehicle_metric_labels(
            &labels,
            Some(Bounds::new(
                view.dest.x,
                view.dest.y,
                view.dest.x + view.dest.width,
                view.dest.y + view.dest.height,
            )),
        )?;
        self.profile_add("render_world.camera_projected_overlay.vehicles", start);
        Ok(())
    }
}
