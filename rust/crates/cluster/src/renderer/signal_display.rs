//! Added source signal candidate overlay, confined to the live camera image.
use super::{
    camera_overlay::CameraOverlayProjection, ClusterUiRenderer, Rect, Result, StrokeStyle,
};
use openpilot_ui_application::signal_display::project_box;

impl ClusterUiRenderer {
    fn draw_brake_observation(
        &mut self,
        projection: &CameraOverlayProjection,
        brake: Option<&openpilot_ui_application::lead_brake::Observation>,
    ) -> Result<()> {
        if let Some(observation) = brake {
            if let Some(bounds) = observation.bounds {
                let video = projection.video_dest;
                let [x, y, w, h] =
                    project_box(bounds, [video.x, video.y, video.width, video.height]);
                let color = if observation.candidate {
                    [255, 100, 40, 255]
                } else {
                    [180, 180, 180, 255]
                };
                self.gfx.rectangle_lines_ex(
                    Rect::new(x - 2., y - 2., w + 4., h + 4.),
                    2.,
                    color,
                )?;
                self.draw_text_with_stroke(
                    if observation.candidate {
                        "BRAKE?"
                    } else {
                        "BRK?"
                    },
                    x,
                    (projection.dest.y + 20.).max(y - 15.),
                    (self.height / 480. * 17.).max(14.),
                    color,
                    StrokeStyle {
                        width: 2,
                        ..StrokeStyle::default()
                    },
                )?;
            }
        }
        Ok(())
    }
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
        let brake = self.camera_resources.brake.read(now, camera, || {
            signal_assist_enabled()
                && openpilot_params::Params::for_runtime()
                    .ok()
                    .and_then(|p| p.get_bool("LeadBrakeLights").ok())
                    .unwrap_or(false)
        });
        self.draw_brake_observation(projection, brake.as_ref())?;
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
        let caption = signal_caption(
            self.language == "ko",
            visible,
            tracks.iter().any(|t| t.held),
            self.camera_resources
                .live
                .as_ref()
                .is_some_and(|c| c.input.is_wide()),
            brake.as_ref().map(|b| b.candidate),
        );
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

fn signal_caption(
    korean: bool,
    visible: usize,
    held: bool,
    wide: bool,
    brake: Option<bool>,
) -> String {
    let mut caption = if korean {
        format!("신호 후보 {visible}")
    } else {
        format!("SIGNAL CANDIDATES {visible}")
    };
    if held {
        caption.push_str(if korean {
            " · ~ 이전 판독"
        } else {
            " · ~ PREVIOUS"
        });
    }
    if wide {
        caption = if korean {
            "신호 관찰: 일반 카메라에서 표시"
        } else {
            "SIGNAL VIEW: ROAD CAMERA ONLY"
        }
        .into();
    }
    caption.push_str(&summary_caption(korean));
    if let Some(candidate) = brake {
        caption.push_str(if candidate { " | BRAKE?" } else { " | BRK ?" });
    }
    caption
}

fn signal_assist_enabled() -> bool {
    openpilot_params::Params::for_runtime()
        .ok()
        .and_then(|params| params.get("SignalAssistMode").ok().flatten())
        .and_then(|bytes| openpilot_ui_application::params::numeric::integer(&bytes))
        .is_some_and(|mode| matches!(mode, 1 | 2))
}

fn summary_caption(korean: bool) -> String {
    let (color, held, online) = openpilot_ui_application::signal_display::signal_display_status();
    let result = ["?", "R", "G", "R+G"][usize::from(color)];
    let status = match (korean, held, online) {
        (true, true, _) => "이전 판독",
        (true, false, true) => "판독 중",
        (true, false, false) => "입력 대기",
        (false, true, _) => "PREVIOUS",
        (false, false, true) => "LIVE",
        (false, false, false) => "WAITING",
    };
    format!(" | {result} {status}")
}
