use super::{
    vehicle::{vehicle_frame, vehicle_overlay_label},
    CameraOverlayProjection,
};
use crate::{
    renderer::{
        tokens::{CAMERA_OVERLAY_FRAME_ROUND_SEGMENTS, LEAD_ONE_DISTANCE_FONT_SIZE},
        with_alpha,
        world::{
            camera_overlay_vehicle_coin_colors, helpers::Bounds, vehicle_has_lead_one_metrics,
            MetricLabel, MetricLine,
        },
        ClusterUiRenderer, Error, Result,
    },
    scene::{
        math::{clamp, rgba_with_alpha},
        VehicleBox,
    },
};

impl ClusterUiRenderer {
    /// # Errors
    /// Returns marker, font measurement or metric label drawing errors.
    pub fn draw_camera_overlay_vehicle<'a>(
        &mut self,
        vehicle: &'a VehicleBox,
        projection: &CameraOverlayProjection,
        shift: f64,
        info_mode: i64,
        labels: Option<&mut Vec<MetricLabel<'a>>>,
    ) -> Result<()> {
        let Some(frame) = vehicle_frame(vehicle, projection, shift, self.camera_overlay.z_offset_m)
        else {
            return Ok(());
        };
        let marker = frame.marker;
        let emphasized = vehicle.primary || vehicle.cut_in;
        let [_, _, ring] =
            camera_overlay_vehicle_coin_colors(vehicle, frame.lead_one, frame.lead_two);
        let confidence = clamp(vehicle.confidence, 0., 1.);
        self.gfx.rounded_lines(
            marker,
            0.20,
            i32::try_from(CAMERA_OVERLAY_FRAME_ROUND_SEGMENTS)
                .map_err(|_| Error::Contract("camera overlay round segments invalid"))?,
            if emphasized { 3.5 } else { 3.0 },
            rgba_with_alpha(ring, 180. + 65. * confidence),
        )?;
        let text = vehicle_overlay_label(vehicle, info_mode, self.is_metric);
        if !text.is_empty() {
            let size = if vehicle_has_lead_one_metrics(vehicle) {
                LEAD_ONE_DISTANCE_FONT_SIZE
            } else if emphasized {
                17.
            } else {
                15.
            };
            let y = if frame.lead_one {
                marker.y - 18.
            } else if frame.lead_two {
                marker.y + marker.height + 16.
            } else {
                marker.y - 12.
            };
            let label = MetricLabel {
                vehicle,
                lines: vec![MetricLine {
                    text,
                    x: marker.x + marker.width * 0.5,
                    y: y.max(14.),
                    size,
                }],
                color: with_alpha(ring, if emphasized { 255 } else { 230 }),
                shadow_offset: 0.,
            };
            if let Some(labels) = labels {
                labels.push(label);
            } else {
                self.draw_vehicle_metric_labels(
                    &[label],
                    Some(Bounds::new(
                        projection.dest.x,
                        projection.dest.y,
                        projection.dest.x + projection.dest.width,
                        projection.dest.y + projection.dest.height,
                    )),
                )?;
            }
        }
        Ok(())
    }
}
