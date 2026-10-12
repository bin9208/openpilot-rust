use super::*;

pub struct RawTextDraw<'a> {
    pub font: u32,
    pub text: &'a str,
    pub position: Point,
    pub size: f32,
    pub spacing: f32,
    pub color: u32,
}
struct WindowSettings {
    flags: Option<u32>,
    target_fps: i32,
}

impl Renderer {
    pub fn raw_window(
        dimensions: (i32, i32),
        title: &str,
        flags: u32,
        target_fps: i32,
    ) -> Result<Self, Error> {
        Self::raw_window_config(
            dimensions,
            title,
            WindowSettings {
                flags: Some(flags),
                target_fps,
            },
        )
    }
    /// # Errors
    /// Returns invalid dimensions or native initialization failure with current raylib flags.
    pub fn raw_window_inheriting_flags(
        dimensions: (i32, i32),
        title: &str,
        target_fps: i32,
    ) -> Result<Self, Error> {
        Self::raw_window_config(
            dimensions,
            title,
            WindowSettings {
                flags: None,
                target_fps,
            },
        )
    }
    fn raw_window_config(
        dimensions: (i32, i32),
        title: &str,
        settings: WindowSettings,
    ) -> Result<Self, Error> {
        if dimensions.0 <= 0 || dimensions.1 <= 0 {
            return Err(Error::Contract("window dimensions must be positive"));
        }
        let mut surface = match settings.flags {
            Some(flags) => ffi::create_with_trace(dimensions.0, dimensions.1, title, flags, 4)?,
            None => ffi::create_inheriting_flags(dimensions.0, dimensions.1, title, 4)?,
        };
        let font = surface.pin_mut().default_font()?;
        if settings.target_fps > 0 {
            surface.pin_mut().target_fps(settings.target_fps);
        }
        Ok(Self {
            texture_releases: Rc::default(),
            resource_releases: Rc::default(),
            surface,
            config: Config {
                big: false,
                large_viewport: false,
                pc: true,
                scale: 1.0,
            },
            dimensions: (
                crate::number::float(dimensions.0),
                crate::number::float(dimensions.1),
            ),
            polygon_shader: None,
            camera_shaders: [None; 6],
            normal: font,
            medium: font,
            pretendard: font,
            display: None,
            fallback: false,
            fonts: HashMap::new(),
            comma: None,
            track: None,
            _thread: PhantomData,
        })
    }

    pub fn raw_font(&mut self, path: &Path, size: i32, points: &[i32]) -> Result<u32, Error> {
        if size <= 0 {
            return Err(Error::Contract("font size must be positive"));
        }
        let path = path
            .to_str()
            .ok_or(Error::Contract("font path is not UTF-8"))?;
        ffi::trace_level(6);
        let font = self
            .surface
            .pin_mut()
            .font(path, size, points, false, false);
        ffi::trace_level(4);
        let font = font?;
        self.surface.pin_mut().font_bilinear(font)?;
        Ok(font)
    }

    pub fn raw_font_measure(
        &self,
        font: u32,
        text: &str,
        size: f32,
        spacing: f32,
    ) -> Result<Point, Error> {
        let value = self.surface.measure(font, text, size, spacing)?;
        Ok(Point {
            x: value.x,
            y: value.y,
        })
    }

    pub fn raw_text(&mut self, draw: RawTextDraw<'_>) -> Result<(), Error> {
        Ok(self.surface.pin_mut().text(
            draw.font,
            draw.text,
            ffi::Point {
                x: draw.position.x,
                y: draw.position.y,
            },
            draw.size,
            draw.spacing,
            draw.color,
        )?)
    }

    pub fn rectangle(&mut self, rect: Rect, color: u32) {
        self.surface.pin_mut().rectangle_rec(convert(rect), color);
    }
    pub fn rectangle_lines_ex(&mut self, rect: Rect, thickness: f32, color: u32) {
        self.surface
            .pin_mut()
            .rectangle_lines_ex(convert(rect), thickness, color);
    }

    pub fn triangle(&mut self, points: [Point; 3], color: u32) {
        self.surface.pin_mut().triangle(
            ffi::Point {
                x: points[0].x,
                y: points[0].y,
            },
            ffi::Point {
                x: points[1].x,
                y: points[1].y,
            },
            ffi::Point {
                x: points[2].x,
                y: points[2].y,
            },
            color,
        );
    }

    pub fn push_matrix(&mut self) {
        self.surface.pin_mut().push_matrix();
    }
    pub fn pop_matrix(&mut self) {
        self.surface.pin_mut().pop_matrix();
    }
    pub fn scale(&mut self, values: [f32; 3]) {
        self.surface
            .pin_mut()
            .scale(values[0], values[1], values[2]);
    }
    pub fn translate(&mut self, values: [f32; 3]) {
        self.surface
            .pin_mut()
            .translate(values[0], values[1], values[2]);
    }
    pub fn begin_capture(&mut self) -> Result<(), Error> {
        self.release_dynamic_textures();
        self.release_resources()?;
        Ok(self.surface.pin_mut().begin_capture()?)
    }
    pub fn end_capture(&mut self) {
        self.surface.pin_mut().end_capture();
    }
    pub fn capture_info(&self) -> Result<TargetInfo, Error> {
        let value = self.surface.capture_info()?;
        Ok(TargetInfo {
            framebuffer: value.framebuffer,
            texture: value.texture,
            width: value.width,
            height: value.height,
        })
    }
    pub fn capture_texture(&mut self, paint: TargetPaint) -> Result<(), Error> {
        Ok(self
            .surface
            .pin_mut()
            .capture_draw(super::resources::texture_paint(paint))?)
    }
    pub fn raw_begin(&mut self) -> Result<(), Error> {
        self.release_dynamic_textures();
        self.release_resources()?;
        self.surface.pin_mut().frame_begin();
        Ok(())
    }
    pub fn raw_end(&mut self) {
        self.surface.pin_mut().frame_end();
    }
    pub fn raw_image_text(
        &mut self,
        image: &mut RawImage,
        draw: RawTextDraw<'_>,
    ) -> Result<(), Error> {
        Ok(self.surface.pin_mut().image_text(
            image.image.pin_mut(),
            draw.text,
            ffi::TextPaint {
                font: draw.font,
                position: ffi::Point {
                    x: draw.position.x,
                    y: draw.position.y,
                },
                size: draw.size,
                spacing: draw.spacing,
                color: draw.color,
            },
        )?)
    }
    pub fn dynamic_filtered_image(
        &mut self,
        pixels: crate::draw::PixelBuffer<'_>,
        filter: TextureFilter,
    ) -> Result<DynamicTexture, Error> {
        let texture = self.dynamic_pixels(pixels.dimensions.0, pixels.dimensions.1, pixels.rgba)?;
        self.surface
            .pin_mut()
            .texture_filter(texture.id(), filter as i32)?;
        Ok(texture)
    }
    pub fn dynamic_texture_filter(
        &mut self,
        texture: &DynamicTexture,
        filter: TextureFilter,
    ) -> Result<(), Error> {
        if !std::rc::Weak::ptr_eq(&texture.releases, &Rc::downgrade(&self.texture_releases)) {
            return Err(Error::Contract("texture belongs to another surface"));
        }
        Ok(self
            .surface
            .pin_mut()
            .texture_filter(texture.id(), filter as i32)?)
    }
    pub fn update_dynamic_pixels(
        &mut self,
        texture: &DynamicTexture,
        pixels: crate::draw::PixelBuffer<'_>,
    ) -> Result<(), Error> {
        if !std::rc::Weak::ptr_eq(&texture.releases, &Rc::downgrade(&self.texture_releases))
            || pixels.dimensions != (texture.width, texture.height)
        {
            return Err(Error::Contract(
                "texture update surface or dimensions mismatch",
            ));
        }
        Ok(self
            .surface
            .pin_mut()
            .pixel_update(texture.id(), pixels.rgba)?)
    }
    pub fn dynamic_logical_size(
        &mut self,
        texture: &mut DynamicTexture,
        dimensions: (i32, i32),
    ) -> Result<(), Error> {
        if !std::rc::Weak::ptr_eq(&texture.releases, &Rc::downgrade(&self.texture_releases)) {
            return Err(Error::Contract("texture belongs to another surface"));
        }
        self.surface
            .pin_mut()
            .texture_size(texture.id, dimensions.0, dimensions.1)?;
        texture.width = dimensions.0;
        texture.height = dimensions.1;
        Ok(())
    }
    pub fn export_rgba(dimensions: (i32, i32), bytes: &[u8], path: &Path) -> Result<(), Error> {
        Ok(ffi::export_rgba(
            dimensions.0,
            dimensions.1,
            bytes,
            path.to_str()
                .ok_or(Error::Contract("image path is not UTF-8"))?,
        )?)
    }
    pub fn capture_image(&self) -> Result<RawImage, Error> {
        Ok(RawImage {
            image: self.surface.capture_image()?,
        })
    }
    pub fn target_image(&self, target: &super::RenderTarget) -> Result<RawImage, Error> {
        self.check_resource(&target.releases)?;
        Ok(RawImage {
            image: self.surface.target_image(target.id)?,
        })
    }
    pub fn capture_filter(&mut self, filter: TextureFilter) -> Result<(), Error> {
        let value = match filter {
            TextureFilter::Point => 0,
            TextureFilter::Bilinear => 1,
            TextureFilter::Trilinear => 2,
        };
        Ok(self.surface.pin_mut().capture_filter(value)?)
    }
    pub fn raw_text_batch(&mut self, text: &str, paints: &[RawTextPaint]) -> Result<(), Error> {
        Ok(self.surface.pin_mut().text_batch(text, paints)?)
    }
}

pub type RawTextPaint = ffi::TextPaint;

#[derive(Clone, Copy, Debug)]
pub enum TextureFilter {
    Point = 0,
    Bilinear = 1,
    Trilinear = 2,
}
pub struct RawImage {
    image: cxx::UniquePtr<ffi::Image>,
}
impl RawImage {
    pub fn blank(dimensions: (i32, i32), color: u32) -> Result<Self, Error> {
        Ok(Self {
            image: ffi::image_blank(dimensions.0, dimensions.1, color)?,
        })
    }
    pub fn rgba(&mut self) -> Result<Vec<u8>, Error> {
        Ok(self.image.pin_mut().rgba()?)
    }
    #[must_use]
    pub fn dimensions(&self) -> (i32, i32) {
        (self.image.width(), self.image.height())
    }
    #[must_use]
    pub fn is_rgba(&self) -> bool {
        self.image.format() == 7
    }
    pub fn rgba_format(&mut self) {
        self.image.pin_mut().rgba_format();
    }
    pub fn flip_vertical(&mut self) {
        self.image.pin_mut().flip_vertical();
    }
    /// # Errors
    /// Returns invalid RGBA metadata; the borrowed bytes cannot outlive this image.
    /// ```compile_fail
    /// use openpilot_startup_ui::renderer::RawImage;
    /// let image = RawImage::blank((2, 2), 0).unwrap();
    /// let pixels = image.rgba_view().unwrap();
    /// drop(image);
    /// std::hint::black_box(pixels);
    /// ```
    pub fn rgba_view(&self) -> Result<&[u8], Error> {
        Ok(self.image.rgba_view()?)
    }
    pub fn encode(&self, extension: &str) -> Result<Vec<u8>, Error> {
        Ok(self.image.encode(extension)?)
    }
    pub fn export_file(&self, path: &Path) -> Result<(), Error> {
        Ok(self.image.export_file(
            path.to_str()
                .ok_or(Error::Contract("image path is not UTF-8"))?,
        )?)
    }
}
impl DecodedImage {
    pub fn encoded(extension: &str, bytes: &[u8]) -> Result<Self, Error> {
        let mut image = ffi::image_encoded(extension, bytes)?;
        let rgba = image.pin_mut().rgba()?;
        Ok(Self {
            width: image.width(),
            height: image.height(),
            rgba,
        })
    }
}
