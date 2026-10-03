use super::*;
use crate::{mici::onroad::debug_plot::data, paint::color};
pub(super) struct Plot {
    queue: [[f64; 400]; 3],
    size: usize,
    index: usize,
    minimum: f64,
    maximum: f64,
    mode: i32,
}
impl Default for Plot {
    fn default() -> Self {
        Self {
            queue: [[0.0; 400]; 3],
            size: 0,
            index: 0,
            minimum: 0.0,
            maximum: 0.0,
            mode: -1,
        }
    }
}
impl Plot {
    pub(super) fn draw(
        &mut self,
        context: &Context,
        draw: &mut dyn Draw,
        rect: Rect,
        mode: i32,
    ) -> Result<(), Error> {
        let sm = context.messages.borrow();
        if mode == 0
            || !sm
                .state
                .topic("carState")
                .map_err(crate::Error::from)?
                .alive
            || !sm
                .state
                .topic("longitudinalPlan")
                .map_err(crate::Error::from)?
                .alive
        {
            return Ok(());
        }
        if mode != self.mode {
            *self = Self::default();
            self.mode = mode;
        }
        let (values, _) = data::values(&sm.state, mode)?;
        let title = match mode {
            0 | 1 => "1.Accel (Y:a_ego, G:a_target, O:a_out)",
            2 => "2.Speed/Accel(Y:speed_0, G:v_ego, O:a_ego)",
            3 => "3.Model(Y:pos_32, G:vel_32, O:vel_0)",
            4 => "4.Lead(Y:accel, G:a_lead, O:v_rel)",
            5 => "5.Lead(Y:a_ego, G:a_lead, O:j_lead)",
            6 => "6.Steer(Y:actual, G:desire, O:output)",
            7 => "7.SteerA (Y:Actual, G:Target, O:Offset*10)",
            8 => "8.Curvature (*10000)",
            _ => "no data",
        };
        self.index = (self.index + 1) % 400;
        for (series, value) in self.queue.iter_mut().zip(values) {
            series[self.index] = value;
        }
        self.size = (self.size + 1).min(400);
        self.minimum = f64::INFINITY;
        self.maximum = f64::NEG_INFINITY;
        for series in &self.queue {
            let values = &series[..self.size];
            let minimum = values
                .iter()
                .copied()
                .reduce(|a, b| if b < a { b } else { a })
                .unwrap_or(f64::INFINITY);
            let maximum = values
                .iter()
                .copied()
                .reduce(|a, b| if b > a { b } else { a })
                .unwrap_or(f64::NEG_INFINITY);
            if minimum < self.minimum {
                self.minimum = minimum;
            }
            if maximum > self.maximum {
                self.maximum = maximum;
            }
        }
        if self.minimum == f64::INFINITY || self.minimum > -2.0 {
            self.minimum = -2.0;
        }
        if self.maximum == f64::NEG_INFINITY || self.maximum < 2.0 {
            self.maximum = 2.0;
        }
        if rect.width < 1200.0 {
            return Ok(());
        }
        let x = f64::from(rect.x) + 350.0;
        let y = f64::from(rect.y) + 40.0;
        let range = self.maximum - self.minimum;
        let ratio = if range < 1.0 { 300.0 } else { 300.0 / range };
        for (series, tint) in [
            color(253, 249, 0, 255),
            color(0, 228, 48, 255),
            color(255, 165, 0, 255),
        ]
        .into_iter()
        .enumerate()
        {
            let mut previous = None;
            let mut latest = None;
            for i in 0..self.size {
                let value = self.queue[series][(self.index + 400 - i) % 400];
                let px = x + f64::from(
                    i32::try_from(self.size - i).map_err(|_| Error::Contract("HUD plot width"))?,
                ) * 2.0;
                let py = y + 300.0 - (value - self.minimum) * ratio;
                let point = Point {
                    x: math::float(px),
                    y: math::float(py),
                };
                if let Some(previous) = previous {
                    draw.line(previous, point, 3.0, tint)?;
                } else {
                    latest = Some((value, [px + 50.0, py + if series > 0 { 40.0 } else { 0.0 }]));
                }
                previous = Some(point);
            }
            if let Some((value, position)) = latest {
                Hud::text(
                    draw,
                    &format!("{value:.2}"),
                    position,
                    40.0,
                    tint,
                    [2.0, 4.0],
                )?;
            }
        }
        Hud::text(
            draw,
            title,
            [x + 400.0, y - 20.0],
            25.0,
            common::WHITE,
            [2.0, 4.0],
        )
    }
}
