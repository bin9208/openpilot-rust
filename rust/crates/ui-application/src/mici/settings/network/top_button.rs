use super::{
    assets::Assets,
    icon::{secured, strength},
};
use crate::mici::widgets::big_button::BigButton;
use openpilot_ui_framework::{
    assets::Texture,
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    network::WifiSession,
    text_layout::float,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use openpilot_wifi::ConnectStatus;
pub(super) struct TopButton {
    button: BigButton,
    session: WifiSession,
    wifi: WidgetHandle,
    slash: Texture,
    strength: [Texture; 3],
    lock: Texture,
    draw_lock: bool,
}
impl TopButton {
    pub fn new(session: WifiSession, wifi: WidgetHandle, assets: &Assets) -> Result<Self, Error> {
        let mut button = assets.button("wi-fi")?;
        button.value = "not connected".into();
        button.scroll = true;
        let slash = assets.get(
            "icons_mici/settings/network/wifi_strength_slash.png",
            (64, 56),
        )?;
        button.icon = Some(slash);
        Ok(Self {
            button,
            session,
            wifi,
            slash,
            strength: [
                assets.get(
                    "icons_mici/settings/network/wifi_strength_low.png",
                    (64, 47),
                )?,
                assets.get(
                    "icons_mici/settings/network/wifi_strength_medium.png",
                    (64, 47),
                )?,
                assets.get(
                    "icons_mici/settings/network/wifi_strength_full.png",
                    (64, 47),
                )?,
            ],
            lock: assets.get("icons_mici/settings/network/new/lock.png", (28, 36))?,
            draw_lock: false,
        })
    }
}
impl Widget for TopButton {
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
        let snapshot = self.session.snapshot();
        let mut network = snapshot
            .networks
            .iter()
            .find(|network| Some(network.ssid.as_str()) == snapshot.wifi_state.ssid.as_deref());
        match snapshot.wifi_state.status {
            ConnectStatus::Connecting => {
                self.button.text = openpilot_wifi::normalize_ssid(
                    snapshot
                        .wifi_state
                        .ssid
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("wi-fi"),
                );
                self.button.value = if snapshot.tethering_active {
                    "starting"
                } else {
                    "connecting..."
                }
                .into();
            }
            ConnectStatus::Connected => {
                self.button.text = openpilot_wifi::normalize_ssid(
                    snapshot
                        .wifi_state
                        .ssid
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("wi-fi"),
                );
                self.button.value = if snapshot.ipv4_address.is_empty() {
                    "obtaining IP...".into()
                } else {
                    snapshot.ipv4_address.clone()
                };
            }
            ConnectStatus::Disconnected => {
                network = None;
                self.button.text = "wi-fi".into();
                self.button.value = "not connected".into();
            }
        }
        if let Some(network) = network {
            self.button.icon = Some(self.strength[strength(network.strength)]);
            self.draw_lock = secured(network);
        } else if snapshot.tethering_active {
            self.button.icon = Some(self.strength[2]);
            self.draw_lock = true;
        } else {
            self.button.icon = Some(self.slash);
            self.draw_lock = false;
        }
        Ok(())
    }
    fn mouse_release(
        &mut self,
        position: Point,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.button.mouse_release(position, frame, draw)?;
        frame
            .navigation
            .push(NavigationRequest::Push(self.wifi.clone()));
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let Self {
            button,
            lock,
            draw_lock,
            ..
        } = self;
        button.paint_with(frame, draw, |button, frame, draw, y| {
            button.draw_content(frame, draw, y)?;
            if *draw_lock {
                let icon = button
                    .icon
                    .ok_or(Error::Contract("network status icon missing"))?;
                let rect = button.state.rect;
                lock.draw(
                    draw,
                    Point {
                        x: float(
                            f64::from(rect.x) + f64::from(rect.width)
                                - 30.0
                                - f64::from(lock.width)
                                + 7.0,
                        ),
                        y: float(y + 30.0 + f64::from(icon.height) - f64::from(lock.height) + 8.0),
                    },
                    1.0,
                    WHITE,
                )?;
            }
            Ok(())
        })?;
        Ok(RenderResult::None)
    }
}
