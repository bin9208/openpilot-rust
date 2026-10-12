//! `LiveRoadCamera`'s source-owned `VisionIPC` and NV12 upload path (MIT).
mod external;
pub mod input;
mod textures;
use input::Input;
use num_traits::ToPrimitive;
use openpilot_startup_ui::egl::Context;
use openpilot_startup_ui::{geometry::Rect, renderer::Renderer};
use std::{path::Path, rc::Rc};
use textures::Textures;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Transport(#[from] openpilot_msgq::Error),
    #[error(transparent)]
    Render(#[from] openpilot_startup_ui::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("cluster camera: {0}")]
    Contract(&'static str),
}

pub struct LiveRoadCamera {
    pub input: Input,
    textures: Textures,
    external: Option<external::External>,
}
impl LiveRoadCamera {
    /// # Errors
    /// Returns native `VisionIPC` or shader initialization failure.
    pub fn new(renderer: &mut Renderer) -> Result<Self, Error> {
        Self::for_platform("camerad", renderer, Path::new("/TICI").is_file())
    }
    /// # Errors
    /// Returns queue or graphics failure after source EGL initialization fallback.
    pub fn for_platform(name: &str, renderer: &mut Renderer, board: bool) -> Result<Self, Error> {
        if board {
            match Context::new() {
                Ok(context) => return Self::with_external(name, renderer, Rc::new(context)),
                Err(error) => eprintln!(
                    "Cluster road camera EGL initialization failed; using NV12 GPU upload: {error}"
                ),
            }
        }
        Self::for_server(name, renderer)
    }
    /// An owned server name permits the same owner to run in isolated IPC QA.
    /// # Errors
    /// Returns native `VisionIPC` or shader initialization failure.
    pub fn for_server(name: &str, renderer: &mut Renderer) -> Result<Self, Error> {
        renderer.camera_shader(openpilot_startup_ui::camera::Style::Cluster, false)?;
        Ok(Self {
            input: Input::for_server(name)?,
            textures: Textures::default(),
            external: None,
        })
    }
    /// # Errors
    /// Returns native queue, external shader or placeholder texture failure.
    pub fn with_external(
        name: &str,
        renderer: &mut Renderer,
        context: Rc<Context>,
    ) -> Result<Self, Error> {
        let mut input = Input::for_server(name)?;
        input.external();
        Ok(Self {
            input,
            textures: Textures::default(),
            external: Some(external::External::new(context, renderer)?),
        })
    }
    #[must_use]
    pub const fn zero_copy(&self) -> bool {
        self.external.is_some()
    }
    /// # Errors
    /// Returns target camera connection or receive failure.
    pub fn select_stream_at(&mut self, prefer_wide: bool, now: f64) -> Result<bool, Error> {
        let wide = self.input.select_stream(prefer_wide, now)?;
        if let Some(external) = &mut self.external {
            external.synchronize(self.input.generation());
        }
        Ok(wide)
    }
    /// # Errors
    /// Propagates camera IPC and drawing failures at the original draw boundary.
    pub fn draw(&mut self, renderer: &mut Renderer, destination: Rect) -> Result<bool, Error> {
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        let seconds = now
            .tv_sec
            .to_f64()
            .ok_or(Error::Contract("monotonic seconds exceed floating range"))?
            + now.tv_nsec.to_f64().ok_or(Error::Contract(
                "monotonic nanoseconds exceed floating range",
            ))? / 1e9;
        self.draw_at(renderer, destination, seconds)
    }
    /// Same draw with a supplied source monotonic clock for deterministic QA.
    /// # Errors
    /// Propagates camera IPC and drawing failures.
    pub fn draw_at(
        &mut self,
        renderer: &mut Renderer,
        destination: Rect,
        now: f64,
    ) -> Result<bool, Error> {
        self.input.poll(now)?;
        if let Some(external) = &mut self.external {
            match external.draw(&self.input, renderer, destination)? {
                external::Outcome::Drawn => return Ok(true),
                external::Outcome::Waiting => return Ok(false),
                external::Outcome::Fallback => {
                    self.external = None;
                    renderer.release_dynamic_textures();
                    renderer.release_cluster_camera_shader(true)?;
                    renderer.camera_shader(openpilot_startup_ui::camera::Style::Cluster, false)?;
                    self.input.enable_copy()?;
                    println!(
                        "Cluster road camera EGL import failed; falling back to NV12 GPU upload"
                    );
                }
            }
        }
        self.textures.draw(&self.input, renderer, destination)
    }
    /// # Errors
    /// Returns external shader release failure after dropping owned camera resources.
    pub fn close(&mut self, renderer: &mut Renderer) -> Result<(), Error> {
        let external = self.external.is_some();
        self.external = None;
        self.textures.clear();
        self.input.close();
        renderer.release_dynamic_textures();
        renderer.release_cluster_camera_shader(external)?;
        Ok(())
    }
}
