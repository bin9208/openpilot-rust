pub mod geometry;
mod points;
use crate::{
    context::Context,
    mici::onroad::driver_state::data::Data,
    paint::{self, color},
};
use geometry::Geometry;
use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    assets::Texture,
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub struct DriverState {
    state: WidgetState,
    context: Context,
    data: Data,
    pub geometry: Geometry,
    icon: Texture,
}
impl DriverState {
    pub fn is_rhd(&self) -> bool {
        self.data.rhd
    }
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        Ok(Self {
            state: WidgetState::default(),
            context,
            data: Data::default(),
            geometry: Geometry::default(),
            icon: paint::texture(canvas, "icons/driver_face.png", (144, 144))?,
        })
    }
}
impl Widget for DriverState {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let messages = self.context.messages.borrow();
        self.data.drawable = crate::state::messages::selfdrive_state(&messages.state)?
            .get_alert_size()
            .map_err(crate::Error::from)?
            == openpilot_cereal::log_capnp::selfdrive_state::AlertSize::None
            && messages
                .state
                .topic("driverStateV2")
                .map_err(crate::Error::from)?
                .receive_frame
                > self.context.ui.borrow().started_frame;
        if !self.data.drawable {
            return Ok(());
        }
        drop(messages);
        self.data = Data::read(&self.context)?;
        if self.data.drawable {
            let orientation = self
                .data
                .orientation
                .as_slice()
                .try_into()
                .map_err(|_| Error::Contract("driver orientation must have three values"))?;
            self.geometry.update(
                orientation,
                self.data.active,
                self.data.rhd,
                self.state.rect,
            );
        }
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if !self.data.drawable {
            return Ok(RenderResult::None);
        }
        let alpha = (255.0 * if self.data.active { 0.65 } else { 0.2 })
            .to_u8()
            .ok_or(Error::Contract("invalid driver opacity"))?;
        let [x, y] = self.geometry.center;
        draw.circle(
            Point {
                x: float(x.trunc()),
                y: float(y.trunc()),
            },
            96.0,
            color(0, 0, 0, 70),
        )?;
        paint::image(
            draw,
            paint::Image {
                texture: self.icon,
                rect: Rect {
                    x: float(x - 72.0),
                    y: float(y - 72.0),
                    width: 144.0,
                    height: 144.0,
                },
                origin: Point::default(),
                rotation: 0.0,
                tint: color(255, 255, 255, alpha),
            },
        )?;
        draw.spline(&self.geometry.lines, 5.2, color(255, 255, 255, alpha))?;
        let alpha = (0.4 * 255.0 * (1.0 - self.geometry.fade))
            .to_u8()
            .ok_or(Error::Contract("invalid arc opacity"))?;
        let tint = if self.context.ui.borrow().engaged {
            color(26, 242, 66, alpha)
        } else {
            color(139, 139, 139, alpha)
        };
        for arc in [&self.geometry.horizontal, &self.geometry.vertical]
            .into_iter()
            .flatten()
        {
            draw.spline(&arc.points, float(arc.thickness), tint)?;
        }
        Ok(RenderResult::None)
    }
}
