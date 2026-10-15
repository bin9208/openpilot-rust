mod textures;
pub mod transform;
use crate::{
    context::{Context, Event},
    state::Status,
};
use openpilot_msgq::{VisionClient, VisionMetadata, VisionStream};
use openpilot_startup_ui::camera::{CameraDraw, Style};
use openpilot_ui_framework::{
    callback::Callback,
    canvas::Canvas,
    draw::Draw,
    geometry::Rect,
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::Cell, rc::Rc, time::Duration};
use textures::Textures;
pub use transform::Transform;

pub struct Config {
    pub name: String,
    pub stream: VisionStream,
    pub compact: bool,
}
struct Target {
    stream: VisionStream,
    client: VisionClient,
}
pub struct CameraView {
    overlay_identity: u64,
    state: WidgetState,
    context: Context,
    config: Config,
    client: Option<VisionClient>,
    target: Option<Target>,
    textures: Textures,
    last_attempt: f64,
    pub available_streams: Vec<VisionStream>,
    pub transform: Transform,
    pub background: Option<u32>,
    transition: Rc<Cell<Option<bool>>>,
    callback: Callback<()>,
    enhance_driver: bool,
    prepared: bool,
}
fn error(error: openpilot_msgq::Error) -> Error {
    Error::Io(std::io::Error::other(error))
}
impl CameraView {
    pub fn new(context: Context, config: Config, canvas: &mut Canvas) -> Result<Self, Error> {
        let client = VisionClient::new(&config.name, config.stream, true).map_err(error)?;
        let enhance_driver = config.stream == VisionStream::Driver;
        let style = if config.compact {
            Style::Compact {
                engaged: false,
                driver: config.stream == VisionStream::Driver,
            }
        } else {
            Style::Large
        };
        canvas.renderer.camera_shader(style, !context.pc)?;
        let textures = Textures::new(!context.pc, canvas)?;
        let transition = Rc::new(Cell::new(None));
        let pending = transition.clone();
        let ui = context.ui.clone();
        let callback = Callback::new(move |()| pending.set(Some(ui.borrow().started)));
        context.listen(Event::Offroad, callback.clone());
        Ok(Self {
            overlay_identity: 1,
            state: WidgetState::default(),
            context,
            config,
            client: Some(client),
            target: None,
            textures,
            last_attempt: 0.0,
            available_streams: Vec::new(),
            transform: Transform::Fit,
            background: None,
            transition,
            callback,
            enhance_driver,
            prepared: false,
        })
    }
    pub fn overlay_camera(&self) -> crate::overlay_state::Camera {
        crate::overlay_state::Camera {
            identity: self.overlay_identity,
            connected: self.client.as_ref().is_some_and(VisionClient::is_connected),
            stream: self.config.stream as i32,
        }
    }
    pub fn stream(&self) -> VisionStream {
        self.config.stream
    }
    pub fn frame(&self) -> Option<VisionMetadata> {
        self.textures.frame
    }
    pub fn switch_stream(&mut self, stream: VisionStream) -> Result<(), Error> {
        if self.config.stream == stream
            || self
                .target
                .as_ref()
                .is_some_and(|target| target.stream == stream)
        {
            return Ok(());
        }
        self.target = Some(Target {
            stream,
            client: VisionClient::new(&self.config.name, stream, true).map_err(error)?,
        });
        Ok(())
    }
    /// Reset the passive stream without entering driver-view mode or retaining old EGL images.
    pub fn reset_stream(&mut self, draw: &mut dyn Draw) -> Result<(), Error> {
        self.textures = Textures::new(!self.context.pc, draw)?;
        self.overlay_identity = self.overlay_identity.wrapping_add(1);
        self.client =
            Some(VisionClient::new(&self.config.name, self.config.stream, true).map_err(error)?);
        self.target = None;
        self.prepared = false;
        self.last_attempt = 0.0;
        self.available_streams.clear();
        self.transition.take();
        Ok(())
    }
    pub fn discard_frame(&mut self) {
        self.textures.frame = None;
    }
    /// Poll existing VisionIPC once without waiting; callers own their polling cadence.
    pub fn poll(&mut self, now: f64, draw: &mut dyn Draw) -> Result<(), Error> {
        if self.ensure_connection(now, draw)? {
            self.receive(draw)?;
        }
        Ok(())
    }
    /// Bind on every draw, including repeated frames shared with another external camera.
    pub fn draw_region(
        &mut self,
        draw: &mut dyn Draw,
        source: Rect,
        destination: Rect,
    ) -> Result<(), Error> {
        if self.textures.bind(draw)? {
            let style = if self.config.compact {
                Style::Compact {
                    engaged: self.context.ui.borrow().status != Status::Disengaged,
                    driver: self.enhance_driver,
                }
            } else {
                Style::Large
            };
            draw.camera(CameraDraw {
                luma: self
                    .textures
                    .luma
                    .as_ref()
                    .ok_or(Error::Contract("camera texture missing"))?
                    .id(),
                chroma: self.textures.chroma.as_ref().map(|t| t.id()),
                source,
                destination,
                style,
            })?;
        }
        Ok(())
    }
    pub fn close(&mut self) {
        self.textures.close();
        self.client = None;
        self.target = None;
        self.available_streams.clear();
        self.prepared = false;
    }
    pub fn prepare(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<(), Error> {
        self.transition()?;
        self.switch(draw)?;
        if self.ensure_connection(frame.now, draw)? {
            self.receive(draw)?;
        }
        self.prepared = true;
        Ok(())
    }
    fn transition(&mut self) -> Result<(), Error> {
        if let Some(started) = self.transition.take() {
            if started && self.frame().is_some() {
                self.available_streams.clear();
                self.overlay_identity = self.overlay_identity.wrapping_add(1);
                self.client = Some(
                    VisionClient::new(&self.config.name, self.config.stream, true)
                        .map_err(error)?,
                );
                self.textures.frame = None;
            }
            if self.config.compact {
                self.textures.frame = None;
            }
        }
        Ok(())
    }
    fn switch(&mut self, draw: &mut dyn Draw) -> Result<(), Error> {
        let Some(mut target) = self.target.take() else {
            return Ok(());
        };
        if !target.client.is_connected() && !target.client.connect().map_err(error)? {
            self.target = Some(target);
            return Ok(());
        }
        let layout = target
            .client
            .layout()
            .ok_or(Error::Contract("target camera has no buffers"))?;
        if let Some(frame) = target.client.receive(Duration::ZERO).map_err(error)? {
            self.textures.initialize(layout, draw)?;
            self.textures.receive(&frame, draw)?;
        } else {
            self.target = Some(target);
            return Ok(());
        }
        self.config.stream = target.stream;
        self.overlay_identity = self.overlay_identity.wrapping_add(1);
        self.client = Some(target.client);
        Ok(())
    }
    fn ensure_connection(&mut self, now: f64, draw: &mut dyn Draw) -> Result<bool, Error> {
        let Some(client) = &mut self.client else {
            return Ok(false);
        };
        if !client.is_connected() {
            self.textures.frame = None;
            self.available_streams.clear();
            if now - self.last_attempt < 0.2 {
                return Ok(false);
            }
            self.last_attempt = now;
            if !client.connect().map_err(error)? {
                return Ok(false);
            }
            self.textures.initialize(
                client
                    .layout()
                    .ok_or(Error::Contract("camera has no buffers"))?,
                draw,
            )?;
            self.available_streams =
                VisionClient::available_streams(&self.config.name).map_err(error)?;
        }
        Ok(true)
    }
    fn receive(&mut self, draw: &mut dyn Draw) -> Result<(), Error> {
        let client = self
            .client
            .as_mut()
            .ok_or(Error::Contract("camera client is closed"))?;
        if let Some(frame) = client.receive(Duration::ZERO).map_err(error)? {
            self.textures.receive(&frame, draw)?;
        } else if !client.is_connected() {
            self.textures.frame = None;
        }
        Ok(())
    }
}
impl Drop for CameraView {
    fn drop(&mut self) {
        self.context
            .callbacks
            .borrow_mut()
            .retain(|(_, callback)| !callback.same(&self.callback));
    }
}
impl Widget for CameraView {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if !self.prepared {
            self.prepare(frame, draw)?;
        }
        self.prepared = false;
        let rect = self.state.rect;
        let Some(metadata) = self.frame() else {
            if let Some(color) = self.background {
                draw.rounded(rect, 0.0, color)?;
            }
            return Ok(RenderResult::None);
        };
        let width =
            i32::try_from(metadata.width).map_err(|_| Error::Contract("camera width overflow"))?;
        let height = i32::try_from(metadata.height)
            .map_err(|_| Error::Contract("camera height overflow"))?;
        self.draw_region(
            draw,
            Rect {
                x: 0.0,
                y: 0.0,
                width: float(
                    f64::from(width)
                        * if self.config.stream == VisionStream::Driver {
                            -1.0
                        } else {
                            1.0
                        },
                ),
                height: float(f64::from(height)),
            },
            self.transform.destination(rect, (width, height)),
        )?;
        Ok(RenderResult::None)
    }
}
