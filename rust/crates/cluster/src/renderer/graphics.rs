use super::{Error, IntoColor, Point, Rect, Result, Ring, TextureView};
use num_traits::ToPrimitive;
use openpilot_startup_ui::{
    draw::{Draw, RoundedOutline},
    renderer::Renderer,
};

#[derive(Default)]
pub struct Graphics {
    pub(super) native: Option<Renderer>,
}

pub(super) fn scalar(value: f64) -> Result<f32> {
    value
        .to_f32()
        .ok_or(Error::Contract("graphics coordinate out of range"))
}
pub(super) fn point(value: Point) -> Result<openpilot_startup_ui::geometry::Point> {
    Ok(openpilot_startup_ui::geometry::Point {
        x: scalar(value.x)?,
        y: scalar(value.y)?,
    })
}
pub(super) fn rect(value: Rect) -> Result<openpilot_startup_ui::geometry::Rect> {
    Ok(openpilot_startup_ui::geometry::Rect {
        x: scalar(value.x)?,
        y: scalar(value.y)?,
        width: scalar(value.width)?,
        height: scalar(value.height)?,
    })
}
pub(super) fn color(value: impl IntoColor) -> u32 {
    u32::from_le_bytes(value.rgba())
}

impl Graphics {
    #[must_use]
    pub fn triangle_strip2d_available(&self) -> bool {
        self.native
            .as_ref()
            .is_some_and(Renderer::triangle_strip2d_available)
    }
    /// # Errors
    /// Returns an invalid capacity or native point-buffer allocation error.
    pub fn points2d(
        &mut self,
        capacity: usize,
    ) -> Result<openpilot_startup_ui::renderer::PointBuffer2D> {
        self.backend()?;
        Ok(Renderer::points2d(capacity)?)
    }
    /// # Errors
    /// Returns an update exceeding the retained point-buffer capacity.
    pub fn update_points2d(
        &mut self,
        buffer: &mut openpilot_startup_ui::renderer::PointBuffer2D,
        values: &[openpilot_startup_ui::renderer::RawPoint],
    ) -> Result<()> {
        self.backend()?;
        Ok(Renderer::update_points2d(buffer, values)?)
    }
    /// # Errors
    /// Returns an unopened surface or an invalid uploaded-point range.
    pub fn triangle_strip2d(
        &mut self,
        buffer: &openpilot_startup_ui::renderer::PointBuffer2D,
        offset: usize,
        count: usize,
        tint: impl IntoColor,
    ) -> Result<()> {
        Ok(self
            .backend()?
            .triangle_strip2d(buffer, offset, count, color(tint))?)
    }
    /// # Errors
    /// Returns a closed renderer; the borrow keeps its native resources alive.
    pub fn backend(&mut self) -> Result<&mut Renderer> {
        self.native
            .as_mut()
            .ok_or(Error::Contract("cluster window is not open"))
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn clear(&mut self, tint: impl IntoColor) -> Result<()> {
        Ok(self.backend()?.clear(color(tint))?)
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn rectangle(&mut self, value: Rect, tint: impl IntoColor) -> Result<()> {
        self.backend()?.rectangle(rect(value)?, color(tint));
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn rounded(
        &mut self,
        value: Rect,
        roundness: f64,
        segments: i32,
        tint: impl IntoColor,
    ) -> Result<()> {
        self.backend()?.rounded_segments(
            rect(value)?,
            scalar(roundness)?,
            segments,
            color(tint),
            false,
        );
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn rounded_lines(
        &mut self,
        value: Rect,
        roundness: f64,
        segments: i32,
        thickness: f64,
        tint: impl IntoColor,
    ) -> Result<()> {
        self.backend()?.rounded_outline(
            rect(value)?,
            RoundedOutline {
                roundness: scalar(roundness)?,
                segments,
                thickness: scalar(thickness)?,
                color: color(tint),
            },
        );
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn line(&mut self, a: Point, b: Point, width: f64, tint: impl IntoColor) -> Result<()> {
        self.backend()?
            .line(point(a)?, point(b)?, scalar(width)?, color(tint));
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn circle(&mut self, center: Point, radius: f64, tint: impl IntoColor) -> Result<()> {
        self.backend()?
            .circle(point(center)?, scalar(radius)?, color(tint));
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn circle_lines(&mut self, center: Point, radius: f64, tint: impl IntoColor) -> Result<()> {
        let x = center
            .x
            .to_i32()
            .ok_or(Error::Contract("circle center invalid"))?;
        let y = center
            .y
            .to_i32()
            .ok_or(Error::Contract("circle center invalid"))?;
        self.backend()?
            .circle_lines((x, y), scalar(radius)?, color(tint));
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn rectangle_lines_ex(
        &mut self,
        value: Rect,
        thickness: f64,
        tint: impl IntoColor,
    ) -> Result<()> {
        self.backend()?
            .rectangle_lines_ex(rect(value)?, scalar(thickness)?, color(tint));
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn triangle(&mut self, a: Point, b: Point, c: Point, tint: impl IntoColor) -> Result<()> {
        self.backend()?
            .triangle([point(a)?, point(b)?, point(c)?], color(tint));
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn ring(&mut self, value: Ring) -> Result<()> {
        self.backend()?.ring(openpilot_startup_ui::draw::Ring {
            center: point(value.center)?,
            inner: scalar(value.inner)?,
            outer: scalar(value.outer)?,
            start: scalar(value.start)?,
            end: scalar(value.end)?,
            segments: value.segments,
            color: color(value.color),
        });
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn begin_scissor(&mut self, value: Rect) -> Result<()> {
        Ok(self.backend()?.scissor(Some(rect(value)?))?)
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn end_scissor(&mut self) -> Result<()> {
        Ok(self.backend()?.scissor(None)?)
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn texture_pro(
        &mut self,
        texture: TextureView,
        source: Rect,
        destination: Rect,
        origin: Point,
        rotation: f64,
        tint: impl IntoColor,
    ) -> Result<()> {
        Ok(self.backend()?.tinted_texture(
            texture.id,
            rect(source)?,
            rect(destination)?,
            point(origin)?,
            scalar(rotation)?,
            color(tint),
        )?)
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn push_matrix(&mut self) -> Result<()> {
        self.backend()?.push_matrix();
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn pop_matrix(&mut self) -> Result<()> {
        self.backend()?.pop_matrix();
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn scale(&mut self, x: f64, y: f64, z: f64) -> Result<()> {
        self.backend()?.scale([scalar(x)?, scalar(y)?, scalar(z)?]);
        Ok(())
    }
    /// # Errors
    /// Returns coordinate conversion or native graphics errors.
    pub fn translate(&mut self, x: f64, y: f64, z: f64) -> Result<()> {
        self.backend()?
            .translate([scalar(x)?, scalar(y)?, scalar(z)?]);
        Ok(())
    }
}
