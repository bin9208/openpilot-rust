use super::*;
impl Renderer {
    pub fn rounded_segments(
        &mut self,
        rect: Rect,
        roundness: f32,
        segments: i32,
        color: u32,
        border: bool,
    ) {
        self.surface
            .pin_mut()
            .rounded_segments(convert(rect), roundness, segments, color, border);
    }
    pub fn circle(&mut self, center: Point, radius: f32, color: u32) {
        self.surface.pin_mut().circle(
            ffi::Point {
                x: center.x,
                y: center.y,
            },
            radius,
            color,
        );
    }
    pub fn circle_gradient(&mut self, center: Point, radius: f32, colors: [u32; 2]) {
        self.surface.pin_mut().circle_gradient(
            ffi::Point {
                x: center.x,
                y: center.y,
            },
            radius,
            colors[0],
            colors[1],
        );
    }
    pub fn gradient(&mut self, rect: Rect, colors: [u32; 4]) {
        self.surface
            .pin_mut()
            .gradient(convert(rect), colors[0], colors[1], colors[2], colors[3]);
    }
    pub fn line(&mut self, start: Point, end: Point, thick: f32, color: u32) {
        self.surface.pin_mut().line(
            ffi::Point {
                x: start.x,
                y: start.y,
            },
            ffi::Point { x: end.x, y: end.y },
            thick,
            color,
        );
    }
    pub fn measure_raw(
        &self,
        font: Font,
        text: &str,
        size: f32,
        spacing: f32,
    ) -> Result<Point, Error> {
        let value = self
            .surface
            .measure(self.font_id(font), text, size, spacing)?;
        Ok(Point {
            x: value.x,
            y: value.y,
        })
    }
    pub fn begin(&mut self) {
        self.surface.pin_mut().begin(self.config.scale);
    }
    pub fn end(&mut self) {
        self.surface.pin_mut().end(self.config.scale);
    }
    pub fn screenshot(&self, path: &Path) -> Result<(), Error> {
        Ok(self.surface.screenshot(
            path.to_str()
                .ok_or(Error::Contract("screenshot path is not UTF-8"))?,
        )?)
    }
    pub fn should_close(&self) -> bool {
        self.surface.should_close()
    }
    pub fn frame_time(&self) -> f32 {
        self.surface.frame_time()
    }
    pub fn time(&self) -> f64 {
        self.surface.time()
    }
    pub fn wheel(&self) -> f32 {
        self.surface.wheel()
    }
    pub fn sample(&self, slot: i32) -> (Point, bool) {
        let sample = self.surface.sample(slot);
        (
            Point {
                x: sample.x / self.config.scale,
                y: sample.y / self.config.scale,
            },
            sample.down,
        )
    }
}
