use crate::{mici::widgets::big_button::BigButton, paint};
use openpilot_ui_framework::{assets::Texture, canvas::Canvas, Error};
use std::collections::BTreeMap;
pub(super) struct Assets {
    textures: BTreeMap<(String, (i32, i32)), Texture>,
}
impl Assets {
    pub fn new(canvas: &mut Canvas) -> Result<Self, Error> {
        let mut textures = BTreeMap::new();
        for (path, size) in [
            ("icons_mici/buttons/button_rectangle.png", (402, 180)),
            (
                "icons_mici/buttons/button_rectangle_pressed.png",
                (402, 180),
            ),
            (
                "icons_mici/buttons/button_rectangle_disabled.png",
                (402, 180),
            ),
            ("icons_mici/buttons/toggle_pill_disabled.png", (84, 66)),
            ("icons_mici/buttons/toggle_pill_enabled.png", (84, 66)),
            (
                "icons_mici/settings/network/wifi_strength_slash.png",
                (48, 42),
            ),
            (
                "icons_mici/settings/network/wifi_strength_low.png",
                (48, 36),
            ),
            (
                "icons_mici/settings/network/wifi_strength_medium.png",
                (48, 36),
            ),
            (
                "icons_mici/settings/network/wifi_strength_full.png",
                (48, 36),
            ),
            ("icons_mici/settings/network/new/lock.png", (21, 27)),
            (
                "icons_mici/settings/network/wifi_strength_slash.png",
                (64, 56),
            ),
            (
                "icons_mici/settings/network/wifi_strength_low.png",
                (64, 47),
            ),
            (
                "icons_mici/settings/network/wifi_strength_medium.png",
                (64, 47),
            ),
            (
                "icons_mici/settings/network/wifi_strength_full.png",
                (64, 47),
            ),
            ("icons_mici/settings/network/new/lock.png", (28, 36)),
            ("icons_mici/setup/driver_monitoring/dm_check.png", (32, 32)),
            (
                "icons_mici/settings/network/new/forget_button.png",
                (84, 84),
            ),
            (
                "icons_mici/settings/network/new/forget_button_pressed.png",
                (84, 84),
            ),
            ("icons_mici/settings/network/new/trash.png", (29, 35)),
            ("icons_mici/settings/network/new/trash.png", (54, 64)),
            ("icons_mici/settings/network/tethering.png", (64, 54)),
            (
                "icons_mici/settings/horizontal_scroll_indicator.png",
                (96, 48),
            ),
        ] {
            textures.insert((path.into(), size), paint::texture(canvas, path, size)?);
        }
        Ok(Self { textures })
    }
    pub fn get(&self, path: &str, size: (i32, i32)) -> Result<Texture, Error> {
        self.textures
            .get(&(path.into(), size))
            .copied()
            .ok_or(Error::Contract("network asset was not loaded"))
    }
    pub fn button(&self, text: &str) -> Result<BigButton, Error> {
        BigButton::new(text, |path, size| self.get(path, size))
    }
}
