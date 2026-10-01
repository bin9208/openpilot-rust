use super::{Phase, WifiManagerUi};
use crate::{
    draw::{Draw, LIGHTGRAY, WHITE},
    geometry::{Point, Rect},
    label::gui_label,
    text::Font,
    text_layout::{float, Horizontal, TextStyle, Vertical},
    widget::{Frame, RenderResult, Widget},
    Error,
};
use num_traits::ToPrimitive;
use openpilot_wifi::{Network, SecurityType};
fn label(draw: &mut dyn Draw, rect: Rect, text: &str, size: f64) -> Result<(), Error> {
    gui_label(
        draw,
        rect,
        text,
        TextStyle {
            font: Font::Normal,
            size,
            spacing: 0.0,
            color: u32::from_le_bytes([255, 255, 255, 229]),
        },
        (Horizontal::Center, Vertical::Middle),
        true,
    )
}
impl WifiManagerUi {
    pub(super) fn render_networks(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        if self.networks.is_empty() {
            label(
                draw,
                rect,
                &self
                    .model
                    .borrow()
                    .context
                    .text("Scanning Wi-Fi networks..."),
                72.0,
            )?;
            return Ok(RenderResult::None);
        }
        if self.prompt(frame)? {
            return Ok(RenderResult::None);
        }
        let count = self
            .networks
            .len()
            .to_f64()
            .ok_or(Error::Contract("network count overflow"))?;
        self.scroll
            .update(rect, float(count * 160.0), frame.events, float(frame.wheel));
        draw.scissor(Some(rect))?;
        for index in 0..self.networks.len() {
            let y = f64::from(rect.y)
                + index
                    .to_f64()
                    .ok_or(Error::Contract("network index overflow"))?
                    * 160.0
                + self.scroll.offset;
            let item = Rect {
                y: float(y),
                height: 160.0,
                ..rect
            };
            if item.x >= rect.x + rect.width
                || item.x + item.width <= rect.x
                || item.y >= rect.y + rect.height
                || item.y + item.height <= rect.y
            {
                continue;
            }
            let network = self.networks[index].clone();
            self.draw_item(frame, draw, (item, &network))?;
            if index + 1 < self.networks.len() {
                let y = (item.y + item.height - 1.0).trunc();
                draw.line(
                    Point {
                        x: item.x.trunc(),
                        y,
                    },
                    Point {
                        x: (item.x + item.width).trunc(),
                        y,
                    },
                    1.0,
                    LIGHTGRAY,
                )?;
            }
        }
        draw.scissor(None)?;
        Ok(RenderResult::None)
    }
    fn draw_item(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
        (rect, network): (Rect, &Network),
    ) -> Result<(), Error> {
        let signal = Rect {
            x: rect.x + rect.width - 50.0,
            y: rect.y + 55.0,
            width: 50.0,
            height: 50.0,
        };
        let security = Rect {
            x: signal.x - 100.0,
            ..signal
        };
        let model = self.model.borrow();
        let phase = model.phase;
        let selected = model
            .network
            .as_ref()
            .is_some_and(|value| value.ssid == network.ssid);
        let snapshot = model.context.session.snapshot();
        let button = self
            .buttons
            .get_mut(&network.ssid)
            .ok_or(Error::Contract("network button missing"))?;
        let status = if phase == Phase::Connecting && model.network.is_some() {
            if selected {
                button.state.enabled = false.into();
                model.context.text("CONNECTING...")
            } else {
                String::new()
            }
        } else if phase == Phase::Forgetting && model.network.is_some() {
            if selected {
                button.state.enabled = false.into();
                model.context.text("FORGETTING...")
            } else {
                String::new()
            }
        } else {
            button.state.enabled = (network.security_type != SecurityType::Unsupported).into();
            String::new()
        };
        drop(model);
        button.state.interaction_gate = self.scroll.touch_valid();
        button.set_rect(Rect {
            width: rect.width - self.button_width * 2.0,
            height: 160.0,
            ..rect
        });
        button.render(frame, draw)?;
        if !status.is_empty() {
            label(
                draw,
                Rect {
                    x: security.x - 410.0,
                    y: rect.y,
                    width: 410.0,
                    height: 160.0,
                },
                &status,
                48.0,
            )?;
        } else if snapshot.saved_ssids.contains(&network.ssid) {
            let button = self
                .forget
                .get_mut(&network.ssid)
                .ok_or(Error::Contract("forget button missing"))?;
            button.state.interaction_gate = self.scroll.touch_valid();
            button.set_rect(Rect {
                x: security.x - self.button_width - 50.0,
                y: rect.y + 40.0,
                width: self.button_width,
                height: 80.0,
            });
            button.render(frame, draw)?;
        }
        let phase = self.model.borrow().phase;
        let icon = if snapshot.connected_ssid.as_deref() == Some(network.ssid.as_str())
            && phase != Phase::Connecting
        {
            Some(self.icons[4])
        } else {
            match network.security_type {
                SecurityType::Unsupported => Some(self.icons[5]),
                SecurityType::Open => None,
                SecurityType::Wpa | SecurityType::Wpa2 | SecurityType::Wpa3 => Some(self.icons[6]),
            }
        };
        if let Some(icon) = icon {
            icon.draw(
                draw,
                Point {
                    x: security.x,
                    y: security.y + (50.0 - icon.height) / 2.0,
                },
                1.0,
                WHITE,
            )?;
        }
        let strength = (f64::from(network.strength) / 33.0)
            .round_ties_even()
            .clamp(0.0, 3.0)
            .to_usize()
            .ok_or(Error::Contract("signal strength out of range"))?;
        self.icons[strength].draw(
            draw,
            Point {
                x: signal.x,
                y: signal.y,
            },
            1.0,
            WHITE,
        )?;
        Ok(())
    }
}
