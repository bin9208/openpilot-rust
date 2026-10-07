use super::ModelRenderer;
use crate::{
    onroad::model_renderer::math::{byte, interp},
    Error,
};
impl ModelRenderer {
    pub(super) fn update_gradient(&mut self) -> Result<(), Error> {
        if !self.common.experimental {
            return Ok(());
        }
        let maximum = (self.common.path.projected.len() / 2).min(self.common.acceleration.len());
        let mut colors = Vec::new();
        let mut stops = Vec::new();
        let mut i = 0;
        while i < maximum {
            let y = self.common.path.projected[i].y;
            if f64::from(y) < f64::from(self.widget.rect.y)
                || f64::from(y) > f64::from(self.widget.rect.y) + f64::from(self.widget.rect.height)
            {
                i += 1;
                continue;
            }
            let position = 1_f32 - (y - self.widget.rect.y) / self.widget.rect.height;
            let acceleration = self.common.acceleration[i];
            let hue = (60_f32 + acceleration * 35.).clamp(0., 120.);
            let saturation = (acceleration * 1.5).abs().min(1.);
            let lightness = interp(f64::from(saturation), &[0., 1.], &[0.95, 0.62])?;
            let alpha = interp(f64::from(position), &[0.75 / 2., 0.75], &[0.4, 0.])?;
            colors.push(experimental_color(
                hue / 360.,
                lightness,
                saturation,
                alpha,
            )?);
            stops.push(f64::from(position));
            i += if i + 2 < maximum { 2 } else { 1 };
        }
        self.gradient.colors = colors;
        self.gradient.stops = stops;
        Ok(())
    }
}
fn experimental_color(h: f32, l: f64, s: f32, a: f64) -> Result<u32, Error> {
    let rgb = if s == 0. {
        [l; 3]
    } else {
        let s = f64::from(s);
        let m2 = l + s - l * s;
        let m1 = 2. * l - m2;
        let value = |h: f32| {
            let h = h.rem_euclid(1.);
            if h < 1_f32 / 6. {
                m1 + (m2 - m1) * f64::from(h) * 6.
            } else if h < 0.5 {
                m2
            } else if h < 2_f32 / 3. {
                m1 + (m2 - m1) * f64::from(2_f32 / 3. - h) * 6.
            } else {
                m1
            }
        };
        [value(h + 1_f32 / 3.), value(h), value(h - 1_f32 / 3.)]
    };
    Ok(crate::paint::color(
        byte(rgb[0] * 255.)?,
        byte(rgb[1] * 255.)?,
        byte(rgb[2] * 255.)?,
        byte(a * 255.)?,
    ))
}
