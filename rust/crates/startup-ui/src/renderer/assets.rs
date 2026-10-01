use super::*;
impl Renderer {
    pub(super) fn load_texture(
        &mut self,
        path: &Path,
        size: i32,
        premultiply: bool,
    ) -> Result<u32, Error> {
        let mut image = ffi::image(
            path.to_str()
                .ok_or(Error::Contract("texture path is not UTF-8"))?,
        )?;
        if premultiply {
            image.pin_mut().premultiply();
        }
        let width = if self.config.scale != 1.0 {
            crate::number::integer(crate::number::float(size) * self.config.scale)?
                .min(image.width())
        } else {
            size
        };
        let height = if self.config.scale != 1.0 {
            crate::number::integer(crate::number::float(size) * self.config.scale)?
                .min(image.height())
        } else {
            size
        };
        if image.width() != width || image.height() != height {
            let scale = (f64::from(width) / f64::from(image.width()))
                .min(f64::from(height) / f64::from(image.height()));
            use num_traits::ToPrimitive;
            let w = (f64::from(image.width()) * scale)
                .to_i32()
                .ok_or(Error::Contract("image width out of range"))?;
            let h = (f64::from(image.height()) * scale)
                .to_i32()
                .ok_or(Error::Contract("image height out of range"))?;
            image.pin_mut().resize(w, h);
        }
        Ok(self.surface.pin_mut().texture(
            image.pin_mut(),
            if self.config.scale != 1.0 { size } else { 0 },
            if self.config.scale != 1.0 { size } else { 0 },
        )?)
    }
    pub fn load_asset(
        &mut self,
        path: &Path,
        options: TextureOptions,
    ) -> Result<(u32, i32, i32), Error> {
        let mut image = ffi::image(
            path.to_str()
                .ok_or(Error::Contract("asset path is not UTF-8"))?,
        )?;
        if options.premultiply {
            image.pin_mut().premultiply();
        }
        if let (Some(logical_width), Some(logical_height)) = (options.width, options.height) {
            if logical_width <= 0 || logical_height <= 0 {
                return Err(Error::Contract("asset size must be positive"));
            }
            let (width, height) = if self.config.scale != 1.0 {
                (
                    crate::number::integer(
                        crate::number::float(logical_width) * self.config.scale,
                    )?
                    .min(image.width()),
                    crate::number::integer(
                        crate::number::float(logical_height) * self.config.scale,
                    )?
                    .min(image.height()),
                )
            } else {
                (logical_width, logical_height)
            };
            if options.keep_aspect {
                let ratio = (f64::from(width) / f64::from(image.width()))
                    .min(f64::from(height) / f64::from(image.height()));
                use num_traits::ToPrimitive;
                let actual_width = (f64::from(image.width()) * ratio)
                    .to_i32()
                    .ok_or(Error::Contract("asset width out of range"))?;
                let actual_height = (f64::from(image.height()) * ratio)
                    .to_i32()
                    .ok_or(Error::Contract("asset height out of range"))?;
                image.pin_mut().resize(actual_width, actual_height);
            } else {
                image.pin_mut().resize(width, height);
            }
        } else if !options.keep_aspect {
            return Err(Error::Contract("resize requires both dimensions"));
        }
        if options.flip_x {
            image.pin_mut().flip_horizontal();
        }
        let (width, height) = if self.config.scale != 1.0 {
            match (options.width, options.height) {
                (Some(width), Some(height)) => (width, height),
                _ => (image.width(), image.height()),
            }
        } else {
            (image.width(), image.height())
        };
        let id = self
            .surface
            .pin_mut()
            .texture(image.pin_mut(), width, height)?;
        Ok((id, width, height))
    }
    pub fn pixel_texture(&mut self, width: i32, height: i32, rgba: &[u8]) -> Result<u32, Error> {
        Ok(self.surface.pin_mut().pixel_texture(width, height, rgba)?)
    }
    pub fn tinted_texture(
        &mut self,
        id: u32,
        source: Rect,
        destination: Rect,
        origin: Point,
        rotation: f32,
        tint: u32,
    ) -> Result<(), Error> {
        Ok(self.surface.pin_mut().tinted_texture(
            id,
            convert(source),
            convert(destination),
            ffi::Point {
                x: origin.x,
                y: origin.y,
            },
            rotation,
            tint,
        )?)
    }
}
