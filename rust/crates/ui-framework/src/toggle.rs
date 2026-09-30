use crate::{
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use num_traits::ToPrimitive;
pub struct Toggle {
    pub state: WidgetState,
    pub changed: Option<Box<dyn FnMut(bool)>>,
    value: bool,
    progress: f64,
    target: f64,
    clicked: bool,
}
impl Toggle {
    pub fn new(value: bool) -> Self {
        let progress = if value { 1.0 } else { 0.0 };
        Self {
            state: WidgetState::default(),
            changed: None,
            value,
            progress,
            target: progress,
            clicked: false,
        }
    }
    pub fn value(&self) -> bool {
        self.value
    }
    pub fn set_value(&mut self, value: bool) {
        self.value = value;
        self.target = if value { 1.0 } else { 0.0 };
    }
    pub fn progress(&self) -> f64 {
        self.progress
    }
}
impl Widget for Toggle {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn set_rect(&mut self, rect: Rect) {
        self.state.rect = Rect {
            width: 160.0,
            height: 80.0,
            ..rect
        };
    }
    fn mouse_release(&mut self, _: Point, _: &Frame<'_>) {
        if !self.state.enabled.get() {
            return;
        }
        self.clicked = true;
        self.set_value(!self.value);
        if let Some(changed) = &mut self.changed {
            changed(self.value);
        }
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if (self.progress - self.target).abs() > 0.01 {
            let delta = frame.dt * 8.0;
            self.progress = (self.progress
                + if self.progress < self.target {
                    delta
                } else {
                    -delta
                })
            .clamp(0.0, 1.0);
        }
        let enabled = self.state.enabled.get();
        let on = if enabled {
            [51, 171, 76]
        } else {
            [34, 119, 34]
        };
        let mut color = [0, 0, 0, 255];
        for (index, value) in on.into_iter().enumerate() {
            color[index] = (57.0 + (f64::from(value) - 57.0) * self.progress)
                .to_u8()
                .ok_or(Error::Contract("toggle color out of range"))?;
        }
        let rect = self.state.rect;
        draw.rounded(
            Rect {
                x: rect.x + 5.0,
                y: rect.y + 10.0,
                width: 150.0,
                height: 60.0,
            },
            1.0,
            u32::from_le_bytes(color),
        )?;
        draw.circle(
            Point {
                x: float(f64::from(rect.x) + 40.0 + 80.0 * self.progress).trunc(),
                y: (rect.y + 40.0).trunc(),
            },
            40.0,
            if enabled {
                WHITE
            } else {
                u32::from_le_bytes([136, 136, 136, 255])
            },
        )?;
        Ok(RenderResult::Bool(std::mem::take(&mut self.clicked)))
    }
}
