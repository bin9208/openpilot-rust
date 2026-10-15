use super::{
    camera_overlay::CameraOverlayProjection,
    graphics::rect,
    tokens::{CAMERA_BACKGROUND_ALPHA, CAMERA_BACKGROUND_VIGNETTE_ALPHA},
    ClusterUiRenderer, Error, Point, Rect, Result,
};
use crate::{
    camera::LiveRoadCamera,
    config::{cluster_camera_view_is_road_camera, cluster_camera_view_prefers_wide},
    models::ClusterUiState,
};
use num_traits::ToPrimitive;

#[derive(Default)]
pub(super) struct CameraResources {
    pub(super) live: Option<LiveRoadCamera>,
    failed: bool,
    pub(super) signal: openpilot_ui_application::signal_display::Reader,
}
impl ClusterUiRenderer {
    pub(super) fn close_live_road_camera(&mut self) -> Result<()> {
        if let Some(mut camera) = self.camera_resources.live.take() {
            camera.close(self.gfx.backend()?)?;
        }
        Ok(())
    }
    fn ensure_live_camera(&mut self) -> Result<bool> {
        if self.camera_resources.live.is_some() {
            return Ok(true);
        }
        if self.camera_resources.failed || !cfg!(unix) {
            return Ok(false);
        }
        match LiveRoadCamera::new(self.gfx.backend()?) {
            Ok(camera) => self.camera_resources.live = Some(camera),
            Err(error) => {
                println!("Cluster live road camera disabled: {error}");
                self.camera_resources.failed = true;
            }
        }
        Ok(self.camera_resources.live.is_some())
    }
    pub(super) fn select_camera_overlay_stream(
        &mut self,
        state: &ClusterUiState,
        now: f64,
    ) -> Result<bool> {
        if let Some(overlay) = state
            .route_overlay
            .as_ref()
            .filter(|overlay| overlay.video_rgba.is_some())
        {
            return Ok(overlay.camera_stream == "wide");
        }
        if !state.onroad || !self.ensure_live_camera()? {
            return Ok(false);
        }
        let camera = self
            .camera_resources
            .live
            .as_mut()
            .ok_or(Error::Contract("live road camera missing"))?;
        let prefer_wide = cluster_camera_view_prefers_wide(
            state.camera_view_mode,
            state.speed_kph,
            camera.input.is_wide(),
        );
        match camera.select_stream_at(prefer_wide, now) {
            Ok(wide) => Ok(wide),
            Err(error) => {
                println!("Cluster live camera stream selection failed: {error}");
                self.close_live_road_camera()?;
                Ok(false)
            }
        }
    }
    /// # Errors
    /// Returns texture or projection drawing errors while ending the scissor region.
    pub fn draw_camera_background(
        &mut self,
        state: &ClusterUiState,
        projection: Option<&CameraOverlayProjection>,
        now: f64,
    ) -> Result<()> {
        if !cluster_camera_view_is_road_camera(state.camera_view_mode) {
            return Ok(());
        }
        let projection = projection
            .copied()
            .or_else(|| self.camera_overlay_projection(state));
        let Some(projection) = projection else {
            return Ok(());
        };
        let texture = state
            .route_overlay
            .as_ref()
            .filter(|overlay| overlay.video_rgba.is_some())
            .map(|overlay| self.route_video_texture_for_overlay(overlay))
            .transpose()?
            .flatten();
        let rounded = Rect::new(
            projection.dest.x.round_ties_even(),
            projection.dest.y.round_ties_even(),
            projection.dest.width.round_ties_even(),
            projection.dest.height.round_ties_even(),
        );
        self.gfx.begin_scissor(rounded)?;
        let result = (|| {
            let drew = if let Some(texture) = texture {
                self.gfx.texture_pro(
                    texture,
                    projection.source,
                    projection.video_dest,
                    Point::default(),
                    0.0,
                    [255, 255, 255, alpha(CAMERA_BACKGROUND_ALPHA)?],
                )?;
                true
            } else if state.onroad && self.ensure_live_camera()? {
                let camera = self
                    .camera_resources
                    .live
                    .as_mut()
                    .ok_or(Error::Contract("live road camera missing"))?;
                match camera.draw_at(self.gfx.backend()?, rect(projection.video_dest)?, now) {
                    Ok(drew) => drew,
                    Err(error) => {
                        println!("Cluster live road camera failed: {error}");
                        self.close_live_road_camera()?;
                        false
                    }
                }
            } else {
                false
            };
            if drew {
                self.gfx.rectangle(
                    projection.dest,
                    [0, 0, 0, alpha(CAMERA_BACKGROUND_VIGNETTE_ALPHA)?],
                )?;
            }
            if texture.is_none() && self.camera_resources.live.is_some() {
                self.draw_signal_detections(&projection, now)?;
            }
            Ok(())
        })();
        let cleanup = self.gfx.end_scissor();
        result.and(cleanup)
    }
}
fn alpha(value: f64) -> Result<u8> {
    value.to_u8().ok_or(Error::Contract("camera alpha invalid"))
}
