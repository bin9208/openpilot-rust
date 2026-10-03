use super::assets::Assets;
use openpilot_ui_framework::{
    assets::Texture,
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use openpilot_wifi::{Network, SecurityType};
pub(super) fn strength(value: i32) -> usize {
    match (f64::from(value) / 100.0 * 2.0).round_ties_even() {
        2.0 => 2,
        1.0 => 1,
        _ => 0,
    }
}
pub(super) fn secured(network: &Network) -> bool {
    !matches!(
        network.security_type,
        SecurityType::Open | SecurityType::Unsupported
    )
}
pub(super) struct WifiIcon {
    state: WidgetState,
    pub network: Network,
    pub missing: bool,
    slash: Texture,
    strength: [Texture; 3],
    lock: Texture,
}
impl WifiIcon {
    pub fn new(network: Network, assets: &Assets) -> Result<Self, Error> {
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 53.0,
            height: 41.0,
        };
        Ok(Self {
            state,
            network,
            missing: false,
            slash: assets.get(
                "icons_mici/settings/network/wifi_strength_slash.png",
                (48, 42),
            )?,
            strength: [
                assets.get(
                    "icons_mici/settings/network/wifi_strength_low.png",
                    (48, 36),
                )?,
                assets.get(
                    "icons_mici/settings/network/wifi_strength_medium.png",
                    (48, 36),
                )?,
                assets.get(
                    "icons_mici/settings/network/wifi_strength_full.png",
                    (48, 36),
                )?,
            ],
            lock: assets.get("icons_mici/settings/network/new/lock.png", (21, 27))?,
        })
    }
}
impl Widget for WifiIcon {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let icon = if self.missing {
            self.slash
        } else {
            self.strength[strength(self.network.strength)]
        };
        let rect = self.state.rect;
        icon.draw(
            draw,
            Point {
                x: rect.x,
                y: float(f64::from(rect.y) + f64::from(rect.height) - f64::from(icon.height)),
            },
            1.0,
            WHITE,
        )?;
        if secured(&self.network) {
            self.lock.draw(
                draw,
                Point {
                    x: float(
                        f64::from(rect.x) + f64::from(rect.width) - f64::from(self.lock.width),
                    ),
                    y: float(
                        f64::from(rect.y) + f64::from(rect.height) - f64::from(self.lock.height)
                            + 6.0,
                    ),
                },
                1.0,
                WHITE,
            )?;
        }
        Ok(RenderResult::None)
    }
}
