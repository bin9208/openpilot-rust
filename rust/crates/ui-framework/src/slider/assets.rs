use super::*;
#[cfg(feature = "native")]
impl SliderAssets {
    pub fn larger(canvas: &mut crate::canvas::Canvas, green: bool) -> Result<Self, Error> {
        let load = |canvas: &mut crate::canvas::Canvas, name: &str, width, height| {
            canvas.texture(
                name,
                openpilot_startup_ui::renderer::TextureOptions {
                    width: Some(width),
                    height: Some(height),
                    ..Default::default()
                },
            )
        };
        let circle = if green {
            "slider_green_rounded_rectangle"
        } else {
            "slider_black_rounded_rectangle"
        };
        Ok(Self {
            background: load(
                canvas,
                "icons_mici/setup/small_slider/slider_bg_larger.png",
                520,
                115,
            )?,
            circle: load(
                canvas,
                &format!("icons_mici/setup/small_slider/{circle}.png"),
                180,
                115,
            )?,
            pressed: load(
                canvas,
                &format!("icons_mici/setup/small_slider/{circle}_pressed.png"),
                180,
                115,
            )?,
            arrow: load(
                canvas,
                "icons_mici/setup/small_slider/slider_arrow.png",
                64,
                55,
            )?,
        })
    }
    pub fn big(
        canvas: &mut crate::canvas::Canvas,
        arrow: Texture,
        red: bool,
    ) -> Result<Self, Error> {
        let load = |canvas: &mut crate::canvas::Canvas, name: &str, width, height| {
            canvas.texture(
                name,
                openpilot_startup_ui::renderer::TextureOptions {
                    width: Some(width),
                    height: Some(height),
                    ..Default::default()
                },
            )
        };
        let circle = if red {
            "button_circle_red"
        } else {
            "button_circle"
        };
        Ok(Self {
            background: load(canvas, "icons_mici/buttons/slider_bg.png", 520, 180)?,
            circle: load(
                canvas,
                &format!("icons_mici/buttons/{circle}.png"),
                180,
                180,
            )?,
            pressed: load(
                canvas,
                &format!("icons_mici/buttons/{circle}_pressed.png"),
                180,
                180,
            )?,
            arrow,
        })
    }
}
