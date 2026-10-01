mod overlay;
mod publisher;
use super::driver_state::DriverState;
use crate::{
    context::{Action, Context},
    onroad::camera::{CameraView, Config, Transform},
    paint,
};
use openpilot_msgq::VisionStream;
use openpilot_ui_framework::{
    assets::Texture,
    canvas::Canvas,
    draw::Draw,
    geometry::Point,
    text::Font,
    text_layout::{float, Horizontal},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};

pub struct Preview {
    state: WidgetState,
    context: Context,
    camera: CameraView,
    pub driver: DriverState,
    eyes: [Texture; 3],
    publisher: Option<openpilot_msgq::Publisher>,
    setup: bool,
}
impl Preview {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        Self::with_camera(context, canvas, "camerad", 20.0, true)
    }
    pub fn with_camera(
        context: Context,
        canvas: &mut Canvas,
        name: &str,
        fps: f64,
        setup: bool,
    ) -> Result<Self, Error> {
        let mut camera = CameraView::new(
            context.clone(),
            Config {
                name: name.into(),
                stream: VisionStream::Driver,
                compact: true,
            },
            canvas,
        )?;
        camera.transform = Transform::DriverCompact;
        let mut driver = DriverState::new(
            context.clone(),
            canvas,
            if setup { 120 } else { 200 },
            !setup,
            setup,
            fps,
        )?;
        driver.force_active = setup;
        let eyes = [
            paint::texture(canvas, "icons_mici/onroad/eye_fill.png", (74, 74))?,
            paint::texture(canvas, "icons_mici/onroad/eye_orange.png", (74, 74))?,
            paint::texture(canvas, "icons_mici/onroad/glasses.png", (171, 171))?,
        ];
        Ok(Self {
            state: WidgetState::default(),
            context,
            camera,
            driver,
            eyes,
            publisher: None,
            setup,
        })
    }
    pub fn driver_orientation(&mut self) -> Result<Vec<f64>, Error> {
        self.driver.read()?;
        Ok(self.driver.data.orientation.clone())
    }
    pub fn has_frame(&self) -> bool {
        self.camera.frame().is_some()
    }
    pub fn is_rhd(&self) -> bool {
        self.driver.data.rhd
    }
    pub fn close(&mut self) {
        self.publisher = None;
        self.camera.close();
    }
    fn report(&self, result: Result<(), crate::Error>) {
        if let Err(error) = result {
            self.context.actions.push(Action::Failure(error));
        }
    }
}
impl Widget for Preview {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, _: &Frame<'_>) {
        let result = self.start();
        self.report(result);
    }
    fn hide(&mut self, _: &Frame<'_>) {
        self.report(self.context.params.put_bool("IsDriverViewEnabled", false));
        self.context.device.borrow_mut().set_override_timeout(
            None,
            (self.context.now_monotonic)(),
            self.context.ui.borrow().ignition,
        );
    }
    fn mouse_release(&mut self, _: Point, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.context.params.remove("DriverTooDistracted")?;
        Ok(())
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.driver.should_draw = true;
        self.driver.force_active = true;
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        draw.scissor(Some(rect))?;
        let result = (|| {
            self.camera.set_rect(rect);
            self.camera.paint(frame, draw)?;
            if !self.has_frame() {
                paint::label(
                    draw,
                    rect,
                    paint::Label {
                        text: &self.context.tr("camera starting"),
                        font: Font::Bold,
                        horizontal: Horizontal::Center,
                        ..paint::Label::new("", if self.setup { 64.0 } else { 54.0 })
                    },
                )?;
                if !self.setup {
                    self.publish()?;
                }
                return Ok(RenderResult::None);
            }
            if self.setup {
                self.driver.set_position(
                    if self.is_rhd() {
                        float(f64::from(rect.x) + 8.0)
                    } else {
                        float(f64::from(rect.x) + f64::from(rect.width) - 128.0)
                    },
                    float(f64::from(rect.y) + 8.0),
                );
                self.driver.render(frame, draw)?;
                self.driver.read()?;
                self.overlay(draw)?;
            } else {
                self.driver.read()?;
                self.overlay(draw)?;
                self.driver.set_position(
                    if self.is_rhd() {
                        rect.x
                    } else {
                        float(f64::from(rect.x) + f64::from(rect.width) - 200.0)
                    },
                    float(f64::from(rect.y) + (f64::from(rect.height) - 200.0) / 2.0),
                );
                self.driver.render(frame, draw)?;
                self.publish()?;
                self.awareness(draw)?;
            }
            Ok(RenderResult::None)
        })();
        draw.scissor(None)?;
        result
    }
}
