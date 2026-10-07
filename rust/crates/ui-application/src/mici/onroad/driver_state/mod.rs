pub mod data;
mod paint;
use crate::{context::Context, paint as product};
use data::Data;
use openpilot_runtime_core::filters::FirstOrderFilter as Filter;
use openpilot_ui_framework::{
    assets::Texture,
    canvas::Canvas,
    draw::Draw,
    geometry::Rect,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};

pub struct DriverState {
    state: WidgetState,
    context: Context,
    pub data: Data,
    pub should_draw: bool,
    pub force_active: bool,
    lines: bool,
    icons: [Texture; 4],
    angles: Vec<Filter>,
    fade: Filter,
    pitch: Filter,
    yaw: Filter,
    rotation: Filter,
    center: Filter,
    looking_center: bool,
}
impl DriverState {
    pub fn new(
        context: Context,
        canvas: &mut Canvas,
        size: i32,
        lines: bool,
        inset: bool,
        fps: f64,
    ) -> Result<Self, Error> {
        use num_traits::ToPrimitive;
        let icon_size = (52.0 / 60.0 * f64::from(size)).round_ties_even();
        let icon_size = if inset {
            (icon_size - (f64::from(size) - icon_size)).round_ties_even()
        } else {
            icon_size
        };
        let icon_size = icon_size
            .to_i32()
            .ok_or(Error::Contract("driver icon size overflow"))?;
        let center_size = (36.0 / 60.0 * f64::from(size))
            .round_ties_even()
            .to_i32()
            .ok_or(Error::Contract("driver center size overflow"))?;
        let mut load = |name, size| {
            product::texture(
                canvas,
                &format!("icons_mici/onroad/driver_monitoring/{name}.png"),
                (size, size),
            )
        };
        let icons = [
            load("dm_person", icon_size)?,
            load("dm_cone", icon_size)?,
            load("dm_center", center_size)?,
            load("dm_background", size)?,
        ];
        let filter = |rc, initialized| Filter::new(0.0, rc, 1.0 / fps, initialized);
        let mut state = WidgetState::default();
        state.rect = Rect {
            width: openpilot_ui_framework::text_layout::float(f64::from(size)),
            height: openpilot_ui_framework::text_layout::float(f64::from(size)),
            ..Default::default()
        };
        Ok(Self {
            state,
            context,
            data: Data::default(),
            should_draw: false,
            force_active: false,
            lines,
            icons,
            angles: (0..72).map(|_| filter(0.1, true)).collect(),
            fade: filter(0.05, true),
            pitch: filter(0.05, false),
            yaw: filter(0.05, false),
            rotation: filter(0.1, false),
            center: filter(0.1, true),
            looking_center: false,
        })
    }
    pub fn read(&mut self) -> Result<(), Error> {
        self.data = Data::read(&self.context)?;
        Ok(())
    }
    fn active(&self) -> bool {
        self.force_active || self.data.active
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
        self.read()?;
        let [pitch, yaw, _] = self.data.orientation.as_slice() else {
            return Ok(());
        };
        let pitch = self.pitch.update(*pitch);
        let yaw = self.yaw.update(*yaw);
        if pitch.abs() < 3_f64.to_radians() && yaw.abs() < 3_f64.to_radians() {
            self.looking_center = true;
        } else if pitch.abs() > 6_f64.to_radians() || yaw.abs() > 6_f64.to_radians() {
            self.looking_center = false;
        }
        self.center.update(f64::from(self.looking_center));
        let difference = (pitch.atan2(yaw).to_degrees() - self.rotation.value() + 180.0)
            .rem_euclid(360.0)
            - 180.0;
        self.rotation.update(self.rotation.value() + difference);
        self.fade
            .update(if !(self.should_draw && self.data.drawable) {
                0.0
            } else if self.active() {
                1.0
            } else {
                0.35
            });
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.draw(draw)?;
        Ok(RenderResult::None)
    }
}
