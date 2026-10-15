//! Final-source alternating display layers; camera and interactive work stay live.
#[derive(Clone, Copy)]
pub struct Frame {
    pub pixels: (i32, i32),
    pub logical: (f32, f32),
    pub index: i64,
    pub enabled: bool,
    pub urgent: bool,
}
/// Only framebuffer/coverage operations cross the graphics boundary. The native
/// caller's viewport, projection, scale and scissor are deliberately retained.
pub trait Backend {
    type Target;
    type Error;
    fn framebuffer(&mut self) -> u32;
    fn flush(&mut self);
    fn allocate(&mut self, size: (i32, i32)) -> Result<Self::Target, Self::Error>;
    fn complete(&mut self, target: &Self::Target) -> Result<bool, Self::Error>;
    fn release(&mut self, target: Self::Target) -> Result<(), Self::Error>;
    fn bind(&mut self, framebuffer: u32);
    fn target_framebuffer(&mut self, target: &Self::Target) -> Result<u32, Self::Error>;
    fn begin_coverage(&mut self);
    fn end_coverage(&mut self);
    fn composite(&mut self, target: &Self::Target, logical: (f32, f32)) -> Result<(), Self::Error>;
}
struct Painting<'a, B: Backend> {
    backend: &'a mut B,
    previous: u32,
}
impl<B: Backend> Drop for Painting<'_, B> {
    fn drop(&mut self) {
        self.backend.end_coverage();
        self.backend.flush();
        self.backend.bind(self.previous);
    }
}
pub struct Layer<T, K> {
    phase: i64,
    target: Option<T>,
    size: (i32, i32),
    destination: (f32, f32),
    key: Option<([f32; 4], K)>,
    frame: i64,
    painted_at: f64,
    ready: bool,
    failed: bool,
}
impl<T, K: PartialEq> Layer<T, K> {
    pub fn new(phase: i64) -> Self {
        Self {
            phase,
            target: None,
            size: (0, 0),
            destination: (0.0, 0.0),
            key: None,
            frame: -1,
            painted_at: 0.0,
            ready: false,
            failed: false,
        }
    }
    pub fn invalidate(&mut self) {
        self.ready = false;
    }
    pub fn close<B: Backend<Target = T>>(&mut self, backend: &mut B) -> Result<(), B::Error> {
        if let Some(target) = self.target.take() {
            backend.release(target)?;
        }
        self.ready = false;
        Ok(())
    }
    pub fn draw<B: Backend<Target = T>>(
        &mut self,
        backend: &mut B,
        rect: [f32; 4],
        key: K,
        frame: Frame,
        now: impl FnOnce() -> f64,
        paint: impl FnOnce(&mut B) -> Result<(), B::Error>,
    ) -> Result<(), B::Error> {
        if !frame.enabled || frame.urgent || self.failed {
            self.invalidate();
            return paint(backend);
        }
        if self.target.is_none() || self.size != frame.pixels {
            let previous = backend.framebuffer();
            backend.flush();
            self.close(backend)?;
            let target = backend.allocate(frame.pixels);
            // Even a failed native allocation must return to the caller's FBO.
            let target = match target {
                Ok(target) => target,
                Err(error) => {
                    backend.bind(previous);
                    return Err(error);
                }
            };
            let complete = backend.complete(&target);
            self.target = Some(target);
            backend.bind(previous);
            self.size = frame.pixels;
            if !complete? {
                self.close(backend)?;
                backend.bind(previous);
                self.failed = true;
                return paint(backend);
            }
            self.destination = frame.logical;
        }
        let now = now();
        let layout_key = (rect, key);
        let age = now - self.painted_at;
        #[expect(
            clippy::manual_range_contains,
            reason = "source comparisons do not expire a NaN age"
        )]
        let repaint = !self.ready
            || self.key.as_ref() != Some(&layout_key)
            || age < 0.0
            || age >= 0.1
            || (frame.index != self.frame && frame.index.rem_euclid(2) == self.phase);
        let target = self.target.as_ref().expect("complete overlay target");
        if repaint {
            let previous = backend.framebuffer();
            backend.flush();
            let framebuffer = backend.target_framebuffer(target)?;
            backend.bind(framebuffer);
            backend.begin_coverage();
            {
                let painting = Painting { backend, previous };
                paint(painting.backend)?;
            }
            self.key = Some(layout_key);
            self.painted_at = now;
            self.frame = frame.index;
            self.ready = true;
        }
        backend.composite(target, self.destination)
    }
}
#[cfg(test)]
mod tests;
