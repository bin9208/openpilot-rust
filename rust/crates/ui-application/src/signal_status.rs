//! Render the planner's actual suspension/evidence/action, never a UI road guess.
use crate::{paint::color, state::messages, Error};
use openpilot_messaging::state::State;
use openpilot_ui_framework::{draw::Draw, geometry::Point, text_layout::float};
#[derive(Clone, Copy, Default, serde::Serialize, PartialEq, Eq)]
pub struct Status {
    pub paused: bool,
    pub color: u8,
    pub action: u8,
    #[serde(skip)]
    pub held: bool,
    #[serde(skip)]
    pub visible: bool,
    #[serde(skip)]
    pub requested: bool,
}
impl Status {
    pub fn new(valid: bool, alive: bool, suspended: bool, color: u8, action: u8) -> Self {
        let paused = valid && alive && suspended;
        if !valid || !alive || paused {
            return Self {
                paused,
                color: if (1..=3).contains(&color) { color } else { 0 },
                action: 0,
                ..Self::default()
            };
        }
        Self {
            paused,
            color: if (1..=3).contains(&color) { color } else { 0 },
            action: if (1..=3).contains(&action) { action } else { 0 },
            ..Self::default()
        }
    }
    pub fn from_messages(state: &State) -> Result<Self, Error> {
        let topic = state.topic("longitudinalPlan")?;
        let plan = messages::longitudinal_plan(state)?;
        let (color, held, online) = crate::signal_display::signal_display_status();
        let mut result = Self::new(
            topic.valid,
            topic.alive,
            plan.get_signal_detection_suspended(),
            color,
            plan.get_signal_assist_action(),
        );
        result.held = held;
        result.requested = crate::signal_display::display_requested();
        result.visible = display_visible(topic.valid, topic.alive, result.requested, online);
        Ok(result)
    }
    pub fn assisted(self) -> bool {
        self.color != 0 || self.action != 0
    }
    pub fn draw_assist(
        self,
        draw: &mut dyn Draw,
        cx: f64,
        cy: f64,
        radius: f64,
    ) -> Result<(), openpilot_ui_framework::Error> {
        if !self.requested || !(self.assisted() || self.visible) {
            return Ok(());
        }
        use num_traits::ToPrimitive;
        let center = (
            cx.to_i32()
                .ok_or(openpilot_ui_framework::Error::Contract("signal x overflow"))?,
            cy.to_i32()
                .ok_or(openpilot_ui_framework::Error::Contract("signal y overflow"))?,
        );
        let colors = [
            color(160, 165, 170, 255),
            color(255, 75, 75, 255),
            color(60, 255, 120, 255),
            color(255, 185, 40, 255),
        ];
        let [r, g, b] = crate::signal_display::ring_rgb(self.color, self.held);
        for offset in [-3., -2., -1.] {
            draw.circle_lines(center, float(radius + offset), color(r, g, b, 255))?;
        }
        if self.action != 0 {
            let center = Point {
                x: float(f64::from(center.0)),
                y: float(f64::from(center.1)),
            };
            draw.circle(center, float(radius * 0.32), color(20, 25, 30, 255))?;
            draw.circle(
                center,
                float(radius * 0.23),
                colors[match self.action {
                    1 => 1,
                    2 => 3,
                    _ => 2,
                }],
            )?;
        }
        Ok(())
    }
}
pub fn display_visible(valid: bool, alive: bool, requested: bool, online: bool) -> bool {
    requested && ((valid && alive) || online)
}
pub fn draw_pause(
    draw: &mut dyn Draw,
    cx: f64,
    cy: f64,
    radius: f64,
) -> Result<(), openpilot_ui_framework::Error> {
    use num_traits::ToPrimitive;
    let center = (
        cx.to_i32()
            .ok_or(openpilot_ui_framework::Error::Contract("signal x overflow"))?,
        cy.to_i32()
            .ok_or(openpilot_ui_framework::Error::Contract("signal y overflow"))?,
    );
    let cx = f64::from(center.0);
    let cy = f64::from(center.1);
    draw.circle(
        Point {
            x: float(cx),
            y: float(cy),
        },
        float(radius),
        color(75, 80, 85, 245),
    )?;
    draw.circle_lines(center, float(radius), color(215, 220, 225, 255))?;
    for side in [-1., 1.] {
        let x = float(cx + side * radius * 0.27);
        draw.line(
            Point {
                x,
                y: float(cy - radius * 0.45),
            },
            Point {
                x,
                y: float(cy + radius * 0.45),
            },
            float((radius * 0.18).max(3.)),
            color(235, 235, 235, 255),
        )?;
    }
    Ok(())
}
