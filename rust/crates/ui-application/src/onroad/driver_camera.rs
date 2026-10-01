use super::{
    camera::{CameraView, Config, Transform},
    driver_state::DriverState,
};
use crate::{
    context::{Action, Context, Event},
    paint::{self, color},
    state::messages,
};
use num_traits::ToPrimitive;
use openpilot_msgq::VisionStream;
use openpilot_ui_framework::{
    callback::Callback,
    canvas::Canvas,
    draw::{Draw, RoundedOutline},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{float, Horizontal},
    widget::{Frame, NavigationQueue, NavigationRequest, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
struct NavigationState {
    queue: Option<NavigationQueue>,
    pending: bool,
}
#[derive(Default, Clone)]
pub(crate) struct Navigation(Rc<RefCell<NavigationState>>);
impl Navigation {
    pub(crate) fn pop(&self) {
        let mut state = self.0.borrow_mut();
        if let Some(queue) = &state.queue {
            queue.push(NavigationRequest::Pop(None));
        } else {
            state.pending = true;
        }
    }
    pub(crate) fn bind(&self, frame: &Frame<'_>) {
        let mut state = self.0.borrow_mut();
        state.queue = Some(frame.navigation.clone());
        if std::mem::take(&mut state.pending) {
            frame.navigation.push(NavigationRequest::Pop(None));
        }
    }
}
pub struct Dialog {
    state: WidgetState,
    context: Context,
    camera: CameraView,
    driver: DriverState,
    navigation: Navigation,
    callback: Callback<()>,
}
impl Dialog {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        Self::with_camera(context, canvas, "camerad")
    }
    pub fn with_camera(context: Context, canvas: &mut Canvas, name: &str) -> Result<Self, Error> {
        let mut camera = CameraView::new(
            context.clone(),
            Config {
                name: name.into(),
                stream: VisionStream::Driver,
                compact: false,
            },
            canvas,
        )?;
        camera.transform = Transform::DriverLarge;
        let driver = DriverState::new(context.clone(), canvas)?;
        let navigation = Navigation::default();
        let expired = navigation.clone();
        let callback = Callback::new(move |()| expired.pop());
        context.params.put_bool("IsDriverViewEnabled", true)?;
        context.listen(Event::InteractiveTimeout, callback.clone());
        Ok(Self {
            state: WidgetState::default(),
            context,
            camera,
            driver,
            navigation,
            callback,
        })
    }
    pub fn is_rhd(&self) -> bool {
        self.driver.is_rhd()
    }
    pub fn has_frame(&self) -> bool {
        self.camera.frame().is_some()
    }
    pub fn close(&mut self) {
        self.camera.close();
    }
    fn face(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        let messages = self.context.messages.borrow();
        let state = messages::driver_state(&messages.state)?;
        let data = if state.get_wheel_on_right_prob() > 0.5 {
            state.get_right_driver_data()
        } else {
            state.get_left_driver_data()
        }
        .map_err(crate::Error::from)?;
        if data.get_face_prob() <= 0.7 {
            return Ok(());
        }
        let position = data.get_face_position().map_err(crate::Error::from)?;
        let deviation = data
            .get_face_orientation_std()
            .map_err(crate::Error::from)?;
        if position.len() != 2 || deviation.len() < 2 {
            return Err(Error::Contract("driver face values missing"));
        }
        let (x, y) = (f64::from(position.get(0)), f64::from(position.get(1)));
        let std = f64::from(deviation.get(0).max(deviation.get(1)));
        let alpha = if std > 0.15 {
            (0.7 - (std - 0.15) * 3.5).max(0.0)
        } else {
            0.7
        };
        let x = (1080.0 - 1714.0 * x).trunc();
        let face_x = f64::from(position.get(0));
        let y =
            (-135.0 + (504.0 + face_x.abs() * 112.0) + (1205.0 - face_x.abs() * 724.0) * y).trunc();
        draw.rounded_outline(
            Rect {
                x: float(x - 110.0),
                y: float(y - 110.0),
                width: 220.0,
                height: 220.0,
            },
            RoundedOutline {
                roundness: float(35.0 / 220.0 / 2.0),
                segments: 10,
                thickness: 10.0,
                color: color(
                    255,
                    255,
                    255,
                    (alpha * 255.0)
                        .to_u8()
                        .ok_or(Error::Contract("invalid face opacity"))?,
                ),
            },
        )
    }
}
impl Drop for Dialog {
    fn drop(&mut self) {
        self.context
            .callbacks
            .borrow_mut()
            .retain(|(_, callback)| !callback.same(&self.callback));
    }
}
impl Widget for Dialog {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.navigation.bind(frame);
    }
    fn hide(&mut self, _: &Frame<'_>) {
        if let Err(error) = self.context.params.put_bool("IsDriverViewEnabled", false) {
            self.context.actions.push(Action::Failure(error));
        }
        self.close();
    }
    fn update(&mut self, frame: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.navigation.bind(frame);
        Ok(())
    }
    fn mouse_release(
        &mut self,
        _: Point,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        frame.navigation.push(NavigationRequest::Pop(None));
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
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
                    ..paint::Label::new("", 100.0)
                },
            )?;
            return Ok(RenderResult::None);
        }
        self.face(draw)?;
        self.driver.set_rect(rect);
        self.driver.render(frame, draw)?;
        Ok(RenderResult::None)
    }
}
