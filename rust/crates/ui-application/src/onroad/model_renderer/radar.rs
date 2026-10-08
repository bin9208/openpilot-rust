use super::{
    drawing::{self, Label},
    input::Input,
    math::{index, integer},
    points::{SamplePoint, ScreenPoint},
    ModelRenderer,
};
use crate::{paint::color, Error};
use openpilot_ui_framework::draw::Draw;
impl ModelRenderer {
    pub(super) fn draw_radar(&self, input: &Input<'_>, draw: &mut dyn Draw) -> Result<(), Error> {
        if self.settings.radar_info <= 0
            || !input.valid("radarState")?
            || !input.valid("modelV2")?
            || input.model.get_lane_lines()?.len() < 3
        {
            return Ok(());
        }
        let line = &self.common.lanes[2].raw;
        for leads in [
            input.radar.get_leads_left()?,
            input.radar.get_leads_right()?,
            input.radar.get_leads_center()?,
        ] {
            for lead in leads {
                let distance = f64::from(lead.get_d_rel());
                if distance <= 2.5 {
                    continue;
                }
                let idx = index(line, distance);
                let Some(raw) = line.get(idx) else {
                    continue;
                };
                let z = f64::from(raw.0[2]) - 0.61;
                let y_rel = f64::from(lead.get_y_rel());
                let Some(side) = self
                    .common
                    .projection
                    .point(SamplePoint([distance, -y_rel, z]))
                else {
                    continue;
                };
                let speed = f64::from(lead.get_v_lead_k());
                let lateral = f64::from(lead.get_v_lat());
                let magnitude = (speed * speed + lateral * lateral).sqrt();
                let signed = if speed >= 0. { magnitude } else { -magnitude };
                if magnitude > 3. {
                    let future_distance = (distance + speed * 0.5).max(2.);
                    let future_y = y_rel + lateral * 0.5;
                    if let Some(future) =
                        self.common
                            .projection
                            .point(SamplePoint([future_distance, -future_y, z]))
                    {
                        let tint = if signed > 0. {
                            color(0, 203, 0, 255)
                        } else {
                            color(255, 0, 0, 255)
                        };
                        draw.line(drawing::point(side), drawing::point(future), 3., tint)?;
                        draw.circle(
                            drawing::point(ScreenPoint([
                                f64::from(integer(future.0[0])?),
                                f64::from(integer(future.0[1])?),
                            ])),
                            10.,
                            tint,
                        )?;
                    }
                    let metric = self.context.ui.borrow().realtime.value.is_metric;
                    let value = format!("{:.0}", signed * if metric { 3.6 } else { 2.2369363 });
                    let bg = if !lead.get_radar() {
                        color(0, 0, 255, 255)
                    } else if f64::from(lead.get_model_prob()) == 0.01 {
                        color(0, 203, 0, 255)
                    } else if signed > 0. {
                        color(255, 175, 3, 255)
                    } else {
                        color(255, 0, 0, 255)
                    };
                    let [x, y] = side.0;
                    drawing::text_box(
                        draw,
                        Label::center(
                            &value,
                            ScreenPoint([f64::from(integer(x)?), f64::from(integer(y)?)]),
                            40.,
                        ),
                        bg,
                    )?;
                    if self.settings.radar_info >= 2 {
                        drawing::text(
                            draw,
                            Label::center(
                                &format!("{y_rel:.1}"),
                                ScreenPoint([f64::from(integer(x)?), f64::from(integer(y - 40.)?)]),
                                30.,
                            ),
                        )?;
                        drawing::text(
                            draw,
                            Label::center(
                                &format!("{distance:.1}"),
                                ScreenPoint([f64::from(integer(x)?), f64::from(integer(y + 30.)?)]),
                                30.,
                            ),
                        )?;
                    }
                } else if self.settings.radar_info >= 3 {
                    drawing::text(
                        draw,
                        Label::center(
                            "*",
                            ScreenPoint([
                                f64::from(integer(side.0[0])?),
                                f64::from(integer(side.0[1])?),
                            ]),
                            40.,
                        ),
                    )?;
                }
            }
        }
        Ok(())
    }
}
