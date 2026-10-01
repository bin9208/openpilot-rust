use super::{Layer, MiciKeyboard};
use crate::{
    draw::{Draw, ImageDraw, WHITE},
    geometry::{MouseEvent, Point, Rect},
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use num_traits::ToPrimitive;
fn interpolate(value: f64, end: f64, start_value: f64, end_value: f64) -> f64 {
    start_value + (value / end).clamp(0.0, 1.0) * (end_value - start_value)
}
fn distance(x: f64, y: f64) -> f64 {
    0.941246 * x.abs().max(y.abs()) + 0.41 * x.abs().min(y.abs())
}
impl MiciKeyboard {
    fn lay_out(&mut self, position: Point, layer: Layer, draw: &mut dyn Draw) -> Result<(), Error> {
        for (row, ids) in self.rows[layer.index()].iter().enumerate() {
            let padding = if row == 1 { 33.0 } else { 44.0 };
            let step_y = (f64::from(self.background.height) - 66.0) / 2.0;
            for (index, id) in ids.iter().copied().enumerate() {
                let count = ids
                    .len()
                    .to_f64()
                    .ok_or(Error::Contract("keyboard count overflow"))?;
                let mut x = f64::from(position.x)
                    + padding
                    + index
                        .to_f64()
                        .ok_or(Error::Contract("keyboard index overflow"))?
                        * ((f64::from(self.background.width) - 2.0 * padding) / (count - 1.0));
                let mut y = f64::from(position.y)
                    + 33.0
                    + row
                        .to_f64()
                        .ok_or(Error::Contract("keyboard row overflow"))?
                        * step_y;
                if let Some((selected, _)) = self.closest {
                    if selected == id {
                        y = (y - 120.0).max(40.0);
                        x += interpolate(
                            x - f64::from(self.state.rect.x),
                            f64::from(self.state.rect.width),
                            100.0,
                            -100.0,
                        );
                        self.keys[id].alpha.update(1.0);
                        self.keys[id].set_font_size(128.0);
                        let alpha = (self.selected_filter.x * 225.0)
                            .to_u8()
                            .ok_or(Error::Contract("selected key alpha out of range"))?;
                        draw.circle_gradient(
                            Point {
                                x: float(x + f64::from(self.keys[id].rect.width) / 2.0),
                                y: float(y + f64::from(self.keys[id].rect.height) / 2.0),
                            },
                            128.0,
                            [u32::from_le_bytes([0, 0, 0, alpha]), 0],
                        )?;
                    } else {
                        let dx = f64::from(self.keys[id].original.x)
                            - f64::from(self.keys[selected].original.x);
                        let dy = f64::from(self.keys[id].original.y)
                            - f64::from(self.keys[selected].original.y);
                        let distance = distance(dx, dy);
                        let inverse = 1.0 / if distance == 0.0 { 1.0 } else { distance };
                        let push = interpolate(distance, 250.0, 20.0, 0.0);
                        x += dx * inverse * push;
                        y += dy * inverse * push;
                        self.keys[id]
                            .alpha
                            .update(interpolate(distance, 100.0, 1.0, 0.35));
                        self.keys[id].set_font_size(interpolate(distance, 150.0, 84.0, 42.0));
                    }
                } else {
                    self.keys[id].alpha.update(1.0);
                    self.keys[id].set_font_size(42.0);
                }
                self.keys[id].position((x, y), f64::from(self.state.rect.y), true);
            }
        }
        Ok(())
    }
}
impl Widget for MiciKeyboard {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn mouse_event(
        &mut self,
        event: MouseEvent,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.handle_event(event, frame.now);
        Ok(())
    }
    fn mouse_release(
        &mut self,
        _: Point,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.release(frame.now);
        Ok(())
    }
    fn update(&mut self, frame: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.selected_filter
            .update(if self.closest.is_some() { 1.0 } else { 0.0 });
        if self.unselect_at.is_some_and(|time| frame.now > time) || !self.state.enabled.get() {
            self.closest = None;
            self.unselect_at = None;
            self.selected_at = None;
            if !self.state.enabled.get() {
                self.dragging = false;
                self.touch_started = false;
                self.release_started = false;
            }
        }
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let position = Point {
            x: rect.x + (rect.width - self.background.width) / 2.0,
            y: rect.y + rect.height - self.background.height,
        };
        let scale = self.background_scale.update(if self.closest.is_some() {
            1.030_769_230_769_230_7
        } else {
            1.0
        });
        let width = f64::from(self.background.width) * scale;
        draw.image(ImageDraw {
            id: self.background.id,
            source: Rect {
                x: 0.0,
                y: 0.0,
                width: self.background.width,
                height: self.background.height,
            },
            destination: Rect {
                x: float(f64::from(rect.x) + f64::from(rect.width) / 2.0 - width / 2.0),
                y: position.y,
                width: float(width),
                height: self.background.height,
            },
            origin: Point::default(),
            rotation: 0.0,
            tint: WHITE,
        })?;
        if !self.initialized {
            for layer in [
                Layer::Lower,
                Layer::Upper,
                Layer::Special,
                Layer::SuperSpecial,
            ] {
                self.lay_out(position, layer, draw)?;
            }
            self.initialized = true;
        }
        self.lay_out(position, self.layer, draw)?;
        for id in self.rows[self.layer.index()].iter().flatten() {
            self.keys[*id].paint(draw)?;
        }
        Ok(RenderResult::None)
    }
}
