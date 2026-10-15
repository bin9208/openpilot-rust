//! Added source signal candidate overlay, confined to the live camera image.
use super::{
    camera_overlay::CameraOverlayProjection, ClusterUiRenderer, Rect, Result, StrokeStyle,
};
use openpilot_ui_application::signal_display::project_box;

impl ClusterUiRenderer {
    pub(super) fn draw_signal_detections(
        &mut self,
        projection: &CameraOverlayProjection,
        now: f64,
    ) -> Result<()> {
        let camera = self
            .camera_resources
            .live
            .as_ref()
            .and_then(|camera| camera.input.display_frame());
        let tracks = self
            .camera_resources
            .signal
            .read(now, camera, signal_assist_enabled);
        if !self.camera_resources.signal.enabled {
            return Ok(());
        }
        let dest = projection.dest;
        let video = projection.video_dest;
        let font_size = (self.height / 480. * 17.).max(14.);
        let stroke = StrokeStyle {
            width: 2,
            ..StrokeStyle::default()
        };
        let mut visible = 0;
        for track in &tracks {
            let [x, y, w, h] =
                project_box(track.bounds, [video.x, video.y, video.width, video.height]);
            if x + w < dest.x
                || y + h < dest.y
                || x > dest.x + dest.width
                || y > dest.y + dest.height
            {
                continue;
            }
            visible += 1;
            let (mut color, letter): ([u8; 4], &str) = match track.state.as_str() {
                "red" => ([255, 65, 65, 255], "R"),
                "green" => ([50, 255, 120, 255], "G"),
                _ => ([255, 210, 65, 255], "?"),
            };
            if track.held {
                for channel in &mut color[..3] {
                    // Source int(channel * .65) truncates positive color values.
                    *channel = u8::try_from(u16::from(*channel) * 65 / 100).unwrap_or_default();
                }
            }
            let rect = Rect::new(x - 2., y - 2., w + 4., h + 4.);
            self.gfx.rectangle_lines_ex(rect, 4., [0, 0, 0, 255])?;
            self.gfx.rectangle_lines_ex(rect, 2., color)?;
            let qualifier = if track.held {
                "~"
            } else if track.state != "unknown" && !track.confirmed {
                "?"
            } else {
                ""
            };
            self.draw_text_with_stroke(
                &format!("{letter}{qualifier}"),
                (dest.x + 4.).max(x.min(dest.x + dest.width - 90.)),
                (dest.y + font_size * 2.).max(y - font_size * 0.7),
                font_size,
                color,
                stroke,
            )?;
        }
        let korean = self.language == "ko";
        let mut caption = if korean {
            format!("신호 후보 {visible}")
        } else {
            format!("SIGNAL CANDIDATES {visible}")
        };
        if tracks.iter().any(|track| track.held) {
            caption.push_str(if korean {
                " · ~ 이전 판독"
            } else {
                " · ~ PREVIOUS"
            });
        }
        if self
            .camera_resources
            .live
            .as_ref()
            .is_some_and(|camera| camera.input.is_wide())
        {
            caption = if korean {
                "신호 관찰: 일반 카메라에서 표시"
            } else {
                "SIGNAL VIEW: ROAD CAMERA ONLY"
            }
            .into();
        }
        self.draw_text_with_stroke(
            &caption,
            dest.x + 12.,
            dest.y + font_size,
            font_size,
            [230, 230, 230, 255],
            stroke,
        )?;
        self.camera_resources
            .signal
            .record_draw(now, camera, visible);
        Ok(())
    }
}

fn signal_assist_enabled() -> bool {
    openpilot_params::Params::for_runtime()
        .ok()
        .and_then(|params| params.get("SignalAssistMode").ok().flatten())
        .and_then(|bytes| openpilot_ui_application::params::numeric::integer(&bytes))
        .is_some_and(|mode| matches!(mode, 1 | 2))
}
