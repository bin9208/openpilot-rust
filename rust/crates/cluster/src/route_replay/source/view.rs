use super::{Error, Reader, Source};
use crate::{
    config::cluster_camera_view_prefers_wide,
    models::{ClusterUiState, RouteOverlay},
    route_replay::{corner, overlay, video, Frame},
    runtime_args::CornerSource,
    scene::math::clamp,
};

impl Source {
    /// # Errors
    /// Returns preload, empty timeline or video errors.
    pub fn state_at(
        &mut self,
        mut time: f64,
        repeat: bool,
        include_overlay: bool,
        camera_mode: i64,
    ) -> Result<ClusterUiState, Error> {
        if repeat && self.end && self.duration() > 0.0 {
            time = time.rem_euclid(self.duration());
        }
        self.ensure_loaded(time)?;
        let frame = if self.duration() <= 0.0 {
            time = 0.0;
            self.timeline.playback_frame(0.0)
        } else {
            if !repeat || self.end {
                time = clamp(time, 0.0, self.duration());
            }
            self.timeline.sample(time)
        }
        .ok_or_else(|| Error::Invalid("route contains no carState frames".into()))?;
        let mut state = self.frame_state(&frame);
        if include_overlay {
            state.route_overlay =
                Some(self.route_overlay(&frame, &state, time, repeat, camera_mode)?);
        }
        Ok(state)
    }
    fn frame_state(&self, frame: &Frame) -> ClusterUiState {
        let mut state = frame.to_state();
        if self.config.yaw_gain != 0.0 || self.config.lateral_offset != 0.0 {
            state.radar_points = state
                .radar_points
                .iter()
                .map(|point| {
                    corner::adjust_point(
                        point,
                        frame.state.vision_yaw_rate_rps,
                        self.config.yaw_gain,
                        self.config.lateral_offset,
                        frame.state.lane_width_m,
                    )
                })
                .collect();
        }
        state
    }
    fn select_stream(&mut self, mode: i64, speed: f64, time: f64) -> Result<(), Error> {
        let prefer_wide = cluster_camera_view_prefers_wide(mode, speed, self.stream == 1);
        let road = video::segment_at(&self.streams[0], time).is_some();
        let wide = video::segment_at(&self.streams[1], time).is_some();
        let next = usize::from((prefer_wide || !road) && wide);
        if next != self.stream {
            self.reader.close()?;
            self.stream = next;
            self.reader = Reader::new(
                self.streams[next].clone(),
                ["road", "wide"][next],
                self.config.video.clone(),
            );
        }
        Ok(())
    }
    fn route_overlay(
        &mut self,
        frame: &Frame,
        state: &ClusterUiState,
        time: f64,
        repeat: bool,
        mode: i64,
    ) -> Result<RouteOverlay, Error> {
        let time = clamp(
            if repeat && self.duration() > 0.0 {
                time.rem_euclid(self.duration())
            } else {
                time
            },
            0.0,
            self.duration(),
        );
        self.select_stream(mode, state.speed_kph, time)?;
        let segment = video::segment_at(&self.streams[self.stream], time)
            .and_then(|segment| segment.index)
            .map_or_else(|| "--".into(), |index| index.to_string());
        let mut overlay = RouteOverlay {
            camera_stream: ["road", "wide"][self.stream].into(),
            cutin_status: Some(frame.metadata.cutin_debug_text.clone()),
            data_lines: overlay::data_lines(frame, state, time, self.duration(), &segment),
            ..RouteOverlay::default()
        };
        if let Some(video) = self.reader.frame_at(time)? {
            overlay.video_rgba = Some(video.rgba.to_vec());
            overlay.video_width = i64::from(video.width);
            overlay.video_height = i64::from(video.height);
            overlay.video_frame_id = Some(video.frame_id);
        } else {
            overlay.video_status = Some(self.reader.status_text());
        }
        Ok(overlay)
    }
    #[must_use]
    pub fn status_text(&self, time: f64, repeat: bool) -> String {
        let shown = clamp(
            if repeat && self.end && self.duration() > 0.0 {
                time.rem_euclid(self.duration())
            } else {
                time
            },
            0.0,
            self.duration(),
        );
        let frame = self.timeline.status_frame(shown);
        let radar = frame
            .as_ref()
            .map_or(0, |frame| frame.state.radar_points.len());
        let detected = frame
            .as_ref()
            .map_or(0, |frame| frame.state.detected_vehicles.len());
        let corner = match self.config.preload.corner_source {
            CornerSource::Live => "live",
            CornerSource::Raw => "raw",
            CornerSource::Stable => "stable",
        };
        let yaw = if self.config.yaw_gain == 0.0 {
            String::new()
        } else {
            format!(" yawHead={:+.2}", self.config.yaw_gain)
        };
        let offset = if self.config.lateral_offset == 0.0 {
            String::new()
        } else {
            format!(" yCenter={:.2}m", self.config.lateral_offset)
        };
        format!(
            "route t={shown:6.1}/{:6.1}s files={}/{} radar={radar} detected={detected} corner={corner}{yaw}{offset}",
            self.duration(),
            self.loaded,
            self.files.len()
        )
    }
}
