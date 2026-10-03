use super::{
    assets::Assets,
    forget::Forget,
    icon::WifiIcon,
    model::{CardState, Model},
};
use crate::mici::widgets::big_button::BigButton;
use openpilot_ui_framework::{
    assets::Texture,
    draw::Draw,
    geometry::{Point, Rect},
    text::Font,
    text_layout::{float, Vertical},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use openpilot_wifi::{SecurityType, Snapshot};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub(super) struct Card {
    button: BigButton,
    icon: WifiIcon,
    forget: Forget,
    check: Texture,
    model: Rc<Model>,
    pub shared: Rc<RefCell<CardState>>,
    forget_pressed: Rc<Cell<bool>>,
}
impl Card {
    pub fn new(
        model: Rc<Model>,
        shared: Rc<RefCell<CardState>>,
        assets: &Assets,
    ) -> Result<Self, Error> {
        let network = shared.borrow().network.clone();
        let mut button = assets.button(&openpilot_wifi::normalize_ssid(&network.ssid))?;
        button.scroll = true;
        button.font_size = Some(48.0);
        let forget_pressed = Rc::new(Cell::new(false));
        let pressed = forget_pressed.clone();
        let motion = model.clone();
        button.state.touch_valid = Some(Box::new(move || {
            !pressed.get() && !motion.pending_move.get()
        }));
        let weak = Rc::downgrade(&model);
        let ssid = network.ssid.clone();
        button.state.click = Some(Box::new(move || {
            if let Some(model) = weak.upgrade() {
                model.report(model.connect(&ssid));
            }
        }));
        let weak = Rc::downgrade(&model);
        let ssid = network.ssid.clone();
        let forget = Rc::new(move || {
            if let Some(model) = weak.upgrade() {
                model.report(model.forget(&ssid));
            }
        });
        Ok(Self {
            button,
            icon: WifiIcon::new(network, assets)?,
            forget: Forget::new(model.context.clone(), forget, assets)?,
            check: assets.get("icons_mici/setup/driver_monitoring/dm_check.png", (32, 32))?,
            model,
            shared,
            forget_pressed,
        })
    }
    fn connected(&self, snapshot: &Snapshot) -> bool {
        snapshot.connected_ssid.as_deref() == Some(self.shared.borrow().network.ssid.as_str())
    }
    fn connecting(&self, snapshot: &Snapshot) -> bool {
        snapshot.connecting_to_ssid.as_deref() == Some(self.shared.borrow().network.ssid.as_str())
    }
    fn show_forget(&self, snapshot: &Snapshot) -> bool {
        let state = self.shared.borrow();
        !state.network.is_tethering
            && !state.forgetting
            && ((snapshot.saved_ssids.contains(&state.network.ssid) && !state.wrong_password)
                || self.connecting(snapshot))
    }
}
impl Widget for Card {
    fn state(&self) -> &WidgetState {
        &self.button.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.button.state
    }
    fn set_position(&mut self, x: f32, y: f32) {
        self.button.set_position(x, y);
    }
    fn set_rect(&mut self, rect: Rect) {
        self.button.set_rect(rect);
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let snapshot = self.model.session.snapshot();
        let connecting = self.connecting(&snapshot);
        let connected = self.connected(&snapshot);
        let state = self.shared.borrow();
        self.icon.network = state.network.clone();
        self.icon.missing = state.missing;
        self.button.shake_start = state.shake_start;
        let unavailable = state.missing
            || connecting
            || connected
            || state.forgetting
            || state.network.security_type == SecurityType::Unsupported;
        self.button.state.enabled = (!unavailable).into();
        self.button.sub_label.color =
            u32::from_le_bytes([255, 255, 255, if unavailable { 149 } else { 229 }]);
        self.button.sub_label.font = if unavailable {
            Font::Regular
        } else {
            Font::SemiBold
        };
        self.button.value = if state.forgetting {
            "forgetting..."
        } else if connecting {
            if state.network.is_tethering {
                "starting..."
            } else {
                "connecting..."
            }
        } else if connected {
            if state.network.is_tethering {
                "tethering"
            } else {
                "connected"
            }
        } else if state.missing {
            "not in range"
        } else if unavailable {
            "unsupported"
        } else if state.wrong_password {
            "wrong password"
        } else {
            "connect"
        }
        .into();
        Ok(())
    }
    fn mouse_release(
        &mut self,
        position: Point,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        if self.show_forget(&self.model.session.snapshot())
            && self.forget.state.rect.contains(position)
        {
            return Ok(());
        }
        self.button.mouse_release(position, frame, draw)
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let snapshot = self.model.session.snapshot();
        let show_forget = self.show_forget(&snapshot);
        let connected = self.connected(&snapshot) && !self.shared.borrow().forgetting;
        let Self {
            button,
            icon,
            forget,
            check,
            forget_pressed,
            ..
        } = self;
        button.paint_with(frame, draw, |button, frame, draw, y| {
            let rect = button.state.rect;
            button.label.text = button.text.clone().into();
            button.label.size = 48.0;
            button.label.scroll = true;
            button.label.vertical = Vertical::Top;
            button.label.color = u32::from_le_bytes([255, 255, 255, 229]);
            button.label.set_rect(Rect {
                x: rect.x + 98.0,
                y: float(y + 23.0),
                width: 276.0,
                height: rect.height - 46.0,
            });
            button.label.render(frame, draw)?;
            if !button.value.is_empty() {
                let mut x = f64::from(rect.x) + 40.0;
                let bottom = y + f64::from(rect.height) - 23.0;
                let width = 322.0
                    - if show_forget {
                        f64::from(forget.state.rect.width)
                    } else {
                        0.0
                    };
                button.sub_label.text = button.value.clone().into();
                let height = button.sub_label.content_height(draw, width);
                if connected {
                    check.draw(
                        draw,
                        Point {
                            x: float(x),
                            y: float(
                                (bottom - height + (height - f64::from(check.height)) / 2.0)
                                    .trunc(),
                            ),
                        },
                        1.0,
                        u32::from_le_bytes([255, 255, 255, 149]),
                    )?;
                    x += f64::from(check.width) + 14.0;
                }
                button.sub_label.set_rect(Rect {
                    x: float(x),
                    y: float(bottom - height),
                    width: float(width),
                    height: float(height),
                });
                button.sub_label.render(frame, draw)?;
            }
            icon.set_position(rect.x + 30.0, float(y + 30.0));
            icon.render(frame, draw)?;
            if show_forget {
                forget.set_position(
                    rect.x + rect.width - forget.state.rect.width,
                    float(y + f64::from(rect.height) - f64::from(forget.state.rect.height)),
                );
                forget.state.interaction_gate = button.state.interaction_gate;
                forget.render(frame, draw)?;
            }
            Ok(())
        })?;
        forget_pressed.set(forget.state.is_pressed());
        Ok(RenderResult::None)
    }
}
