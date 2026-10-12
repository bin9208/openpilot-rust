use super::{
    graphics::{color, point, rect},
    ClusterUiRenderer, FrameClock, Point, Rect, RenderedFrame, Result,
};
use crate::models::ClusterUiState;
use openpilot_startup_ui::renderer::{RawImage, RenderTarget, TargetPaint, TextureFilter};

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct CaptureOptions {
    pub portrait_upload: bool,
    pub output_width: Option<i32>,
    pub output_height: Option<i32>,
}
#[derive(Default)]
pub(super) struct OutputResources {
    portrait: Option<RenderTarget>,
    pub hidden: bool,
}
impl OutputResources {
    pub fn clear(&mut self) {
        self.portrait = None;
    }
}

impl ClusterUiRenderer {
    /// # Errors
    /// Returns state rendering, GPU readback, or portrait-target allocation errors.
    pub fn capture_state_at(
        &mut self,
        state: &ClusterUiState,
        clock: FrameClock,
        options: CaptureOptions,
    ) -> Result<RenderedFrame> {
        let mut image = self.render_to_image_at(state, clock, options)?;
        image.rgba_format();
        let (width, height) = image.dimensions();
        let rgba = image.rgba_view()?.to_vec();
        Ok(RenderedFrame {
            width,
            height,
            rgba,
        })
    }
    fn render_to_image_at(
        &mut self,
        state: &ClusterUiState,
        clock: FrameClock,
        options: CaptureOptions,
    ) -> Result<RawImage> {
        self.open(self.output_resources.hidden)?;
        self.prepare_trip_report_cache(state, clock.monotonic)?;
        let start = self.profile_start();
        self.ensure_capture_target()?;
        self.profile_add("render_to_image.get_capture_target", start);
        let start = self.profile_start();
        self.gfx.backend()?.begin_capture()?;
        let result = self.render_at(state, clock, None);
        self.gfx.backend()?.end_capture();
        result?;
        self.profile_add("render_to_image.draw_to_target", start);
        self.flush_pending_strokes()?;
        if options.portrait_upload {
            self.capture_portrait_image(options)
        } else {
            let start = self.profile_start();
            let mut image = self.gfx.backend()?.capture_image()?;
            self.profile_add("render_to_image.readback_texture", start);
            let start = self.profile_start();
            image.flip_vertical();
            self.profile_add("render_to_image.flip_vertical", start);
            Ok(image)
        }
    }
    /// # Errors
    /// Returns rendering, readback, or clock conversion errors.
    pub fn capture_state(
        &mut self,
        state: &ClusterUiState,
        options: CaptureOptions,
    ) -> Result<RenderedFrame> {
        self.capture_state_at(state, FrameClock::now()?, options)
    }
    pub(super) fn ensure_capture_target(&mut self) -> Result<()> {
        if !self.gfx.backend()?.has_render_target() {
            let start = self.profile_start();
            self.gfx
                .backend()?
                .ensure_render_target((self.options.width, self.options.height))?;
            self.profile_add("render_target.alloc_capture", start);
            let filter = self.profile_start();
            self.gfx
                .backend()?
                .capture_filter(TextureFilter::Bilinear)?;
            self.profile_add("render_target.filter_capture", filter);
        }
        Ok(())
    }
    fn capture_portrait_image(&mut self, options: CaptureOptions) -> Result<RawImage> {
        let size = (
            options
                .output_width
                .filter(|value| *value != 0)
                .unwrap_or(self.options.height),
            options
                .output_height
                .filter(|value| *value != 0)
                .unwrap_or(self.options.width),
        );
        let start = self.profile_start();
        let target = if let Some(target) = self
            .output_resources
            .portrait
            .take()
            .filter(|target| (target.width, target.height) == size)
        {
            target
        } else {
            let allocation = self.profile_start();
            let target = self.gfx.backend()?.render_target(size)?;
            self.profile_add("render_target.alloc_portrait_upload", allocation);
            let filter = self.profile_start();
            self.gfx
                .backend()?
                .target_filter(&target, TextureFilter::Bilinear)?;
            self.profile_add("render_target.filter_portrait_upload", filter);
            target
        };
        self.profile_add("render_to_image.get_portrait_upload_target", start);
        let start = self.profile_start();
        self.gfx.backend()?.begin_target(&target)?;
        let result = (|| -> Result<()> {
            self.gfx.clear(self.current_theme().bg)?;
            let info = self.gfx.backend()?.capture_info()?;
            self.gfx.backend()?.capture_texture(TargetPaint {
                source: rect(Rect::new(
                    0.0,
                    0.0,
                    f64::from(info.width),
                    f64::from(info.height),
                ))?,
                destination: rect(Rect::new(0.0, self.width, self.width, self.height))?,
                origin: point(Point::default())?,
                rotation: -90.0,
                tint: color([255, 255, 255]),
            })?;
            Ok(())
        })();
        self.gfx.backend()?.end_target();
        if let Err(error) = result {
            self.output_resources.portrait = Some(target);
            return Err(error);
        }
        self.profile_add("render_to_image.gpu_upload_transform", start);
        let start = self.profile_start();
        let image = self.gfx.backend()?.target_image(&target);
        self.output_resources.portrait = Some(target);
        let image = image?;
        self.profile_add("render_to_image.readback_upload_texture", start);
        Ok(image)
    }
    /// # Errors
    /// Returns rendering or PNG encoding errors after releasing the image.
    pub fn render_to_png_bytes_at(
        &mut self,
        state: &ClusterUiState,
        clock: FrameClock,
        portrait_upload: bool,
    ) -> Result<Vec<u8>> {
        let start = self.profile_start();
        let image = self.render_to_image_at(
            state,
            clock,
            CaptureOptions {
                portrait_upload,
                ..Default::default()
            },
        )?;
        self.profile_add("render_to_png.render_to_image", start);
        let start = self.profile_start();
        let encoded = image.encode(".png");
        self.profile_add("render_to_png.export_png", start);
        let start = self.profile_start();
        drop(image);
        self.profile_add("render_to_png.unload_image", start);
        Ok(encoded?)
    }
    /// # Errors
    /// Returns rendering, PNG encoding, or clock conversion errors.
    pub fn render_to_png_bytes(
        &mut self,
        state: &ClusterUiState,
        portrait_upload: bool,
    ) -> Result<Vec<u8>> {
        self.render_to_png_bytes_at(state, FrameClock::now()?, portrait_upload)
    }
    /// # Errors
    /// Returns rendering, image conversion, or callback errors; the image always drops.
    pub fn with_rgba_buffer_at<T>(
        &mut self,
        state: &ClusterUiState,
        clock: FrameClock,
        options: CaptureOptions,
        callback: impl FnOnce(&mut Self, &[u8], (i32, i32)) -> Result<T>,
    ) -> Result<T> {
        let start = self.profile_start();
        let mut image = self.render_to_image_at(state, clock, options)?;
        self.profile_add("render_to_rgba.render_to_image", start);
        let result = (|| -> Result<T> {
            if !image.is_rgba() {
                let start = self.profile_start();
                image.rgba_format();
                self.profile_add("render_to_rgba.image_format", start);
            }
            let start = self.profile_start();
            let pixels = image.rgba_view()?;
            let dimensions = image.dimensions();
            self.profile_add("render_to_rgba.buffer_view", start);
            callback(self, pixels, dimensions)
        })();
        let start = self.profile_start();
        drop(image);
        self.profile_add("render_to_rgba.unload_image", start);
        result
    }
    /// # Errors
    /// Returns rendering, image conversion, or clock errors.
    pub fn render_to_rgba_bytes_at(
        &mut self,
        state: &ClusterUiState,
        clock: FrameClock,
        options: CaptureOptions,
    ) -> Result<RenderedFrame> {
        self.with_rgba_buffer_at(
            state,
            clock,
            options,
            |renderer, pixels, (width, height)| {
                let start = renderer.profile_start();
                let rgba = pixels.to_vec();
                renderer.profile_add("render_to_rgba.copy_bytes", start);
                Ok(RenderedFrame {
                    width,
                    height,
                    rgba,
                })
            },
        )
    }
    /// # Errors
    /// Returns rendering, conversion, or clock errors.
    pub fn render_to_rgba_bytes(
        &mut self,
        state: &ClusterUiState,
        options: CaptureOptions,
    ) -> Result<RenderedFrame> {
        self.render_to_rgba_bytes_at(state, FrameClock::now()?, options)
    }
    /// # Errors
    /// Returns rendering or image-file export errors.
    pub fn render_to_file(&mut self, state: &ClusterUiState, path: &std::path::Path) -> Result<()> {
        let image =
            self.render_to_image_at(state, FrameClock::now()?, CaptureOptions::default())?;
        Ok(image.export_file(path)?)
    }
}
