use super::{ModelRenderer, RadarItem};
use crate::{
    onroad::model_renderer::{
        drawing::{self, Anchor, Label},
        input::Input,
        math::{clip, decimal, float, index, integer},
        points::{SamplePoint, ScreenPoint},
    },
    paint::color,
    Error,
};
use openpilot_ui_framework::{draw::Draw, geometry::Rect};
impl ModelRenderer {
    pub(super) fn update_radar(&mut self, input: &Input<'_>, draw: &dyn Draw) -> Result<(), Error> {
        self.radar_items.clear();
        let line = &self.common.lanes[2].raw;
        if line.is_empty() {
            return Ok(());
        }
        let rect = self.widget.rect;
        let xmin = f64::from(rect.x);
        let xmax = xmin + f64::from(rect.width);
        let ymin = f64::from(rect.y);
        let ymax = ymin + f64::from(rect.height);
        for leads in [
            input.radar.get_leads_left()?,
            input.radar.get_leads_right()?,
            input.radar.get_leads_center()?,
        ] {
            for lead in leads {
                let distance = f64::from(lead.get_d_rel());
                let y_rel = f64::from(lead.get_y_rel());
                if distance <= 2.5 {
                    continue;
                }
                let Some(raw) = line.get(index(line, distance)) else {
                    continue;
                };
                let z = f64::from(raw.0[2]) - 0.61;
                let Some(point) = self
                    .common
                    .projection
                    .point(SamplePoint([distance, -y_rel, z]))
                else {
                    continue;
                };
                let [x, y] = point.0;
                let speed = f64::from(lead.get_v_lead_k());
                let lateral = f64::from(lead.get_v_lat());
                let magnitude = (speed * speed + lateral * lateral).sqrt();
                let signed = if speed >= 0. { magnitude } else { -magnitude };
                if magnitude <= 3. {
                    self.radar_items.push(RadarItem {
                        x,
                        y,
                        width: 18.,
                        height: 18.,
                        text: "*".into(),
                        color: color(255, 255, 255, 230),
                        star: true,
                    });
                    continue;
                }
                let speed = signed
                    * if self.context.ui.borrow().realtime.value.is_metric {
                        3.6
                    } else {
                        2.2369363
                    };
                let text = decimal(speed, 0);
                let width = f64::from(draw.measure_default(&text, 22)? + 12);
                let height = 26.;
                let x = clip(x - width * 0.5, xmin, xmax - width);
                let y = clip(y - height * 0.5, ymin, ymax - height);
                let tint = if !lead.get_radar() {
                    color(0, 0, 255, 220)
                } else if (f64::from(lead.get_model_prob()) - 0.01).abs() < 0.001 {
                    color(0, 203, 0, 220)
                } else if signed > 0. {
                    color(255, 175, 3, 220)
                } else {
                    color(255, 0, 0, 220)
                };
                self.radar_items.push(RadarItem {
                    x,
                    y,
                    width,
                    height,
                    text,
                    color: tint,
                    star: false,
                });
            }
        }
        Ok(())
    }
    pub(super) fn draw_radar(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        for item in &self.radar_items {
            if item.star {
                draw.measure_default(&item.text, 22)?;
                drawing::text(
                    draw,
                    Label {
                        color: item.color,
                        border: 1.,
                        ..Label::center(&item.text, ScreenPoint([item.x, item.y]), 22.)
                    },
                )?;
                continue;
            }
            draw.rounded_segments(
                Rect {
                    x: float(item.x),
                    y: float(item.y),
                    width: float(item.width),
                    height: float(item.height),
                },
                0.28,
                8,
                item.color,
                false,
            )?;
            let width = f64::from(draw.measure_default(&item.text, 22)?);
            let x = integer(item.x + (item.width - width) / 2.)?;
            let y = integer(item.y + (item.height - 22.) / 2. - 1.)?;
            drawing::text(
                draw,
                Label {
                    anchor: Anchor::LeftTop,
                    border: 1.,
                    ..Label::center(&item.text, ScreenPoint([f64::from(x), f64::from(y)]), 22.)
                },
            )?;
        }
        Ok(())
    }
}
