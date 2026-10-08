use super::{LeadRectangle, ModelRenderer};
use crate::{
    onroad::model_renderer::{
        drawing,
        input::Input,
        math::{clip, index},
        points::{SamplePoint, ScreenPoint},
    },
    paint::color,
    Error,
};
use openpilot_ui_framework::draw::Draw;
impl ModelRenderer {
    pub(super) fn update_lead(&mut self, input: &Input<'_>) -> Result<(), Error> {
        self.lead = None;
        let lead = input.radar.get_lead_one()?;
        if !lead.get_status() {
            return Ok(());
        }
        let distance = f64::from(lead.get_d_rel());
        let y = f64::from(lead.get_y_rel());
        let idx = index(&self.common.path.raw, distance);
        let z = self
            .common
            .path
            .raw
            .get(idx)
            .map_or(0., |p| f64::from(p.0[2]));
        let left = self.common.projection.point(SamplePoint([
            distance,
            -y - 1.2,
            z + self.common.path_height,
        ]));
        let right = self.common.projection.point(SamplePoint([
            distance,
            -y + 1.2,
            z + self.common.path_height,
        ]));
        if let (Some(left), Some(right)) = (left, right) {
            let center = ScreenPoint([
                (left.0[0] + right.0[0]) * 0.5,
                (left.0[1] + right.0[1]) * 0.5,
            ]);
            let width = clip((right.0[0] - left.0[0]).abs(), 60., 400.);
            let point = if let Some(previous) = self.lead_filter {
                ScreenPoint(std::array::from_fn(|i| {
                    previous.0[i] + (center.0[i] - previous.0[i]) * 0.2
                }))
            } else {
                center
            };
            self.lead_filter = Some(point);
            let rect = self.widget.rect;
            let xmin = f64::from(rect.x);
            let xmax = xmin + f64::from(rect.width);
            let ymin = f64::from(rect.y);
            let ymax = ymin + f64::from(rect.height);
            let left = clip(point.0[0] - width * 0.5, xmin, xmax);
            let right = clip(point.0[0] + width * 0.5, xmin, xmax);
            let bottom = clip(point.0[1], ymin, ymax);
            let top = clip(point.0[1] - width * 0.8, ymin, ymax);
            let tint = if !lead.get_radar() {
                color(0, 120, 255, 255)
            } else if matches!(lead.get_radar_track_id(), 0 | 1) {
                color(201, 34, 49, 255)
            } else {
                color(255, 115, 0, 255)
            };
            self.lead = Some(LeadRectangle {
                corners: [
                    ScreenPoint([left, top]),
                    ScreenPoint([right, top]),
                    ScreenPoint([right, bottom]),
                    ScreenPoint([left, bottom]),
                ],
                color: tint,
            });
        } else {
            self.lead_filter = None;
        }
        Ok(())
    }
    pub(super) fn draw_lead(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        if let Some(lead) = &self.lead {
            drawing::outline(draw, &lead.corners.map(drawing::point), (lead.color, 4.))?;
        }
        Ok(())
    }
}
