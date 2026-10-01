use super::*;
use crate::{
    draw::WHITE,
    geometry::{Point, Rect},
    widget::{RenderResult, Widget},
};
impl Widget for Keyboard {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let bounds = self.state.rect;
        let rect = Rect {
            x: bounds.x + 50.0,
            y: bounds.y + 50.0,
            width: bounds.width - 100.0,
            height: bounds.height - 100.0,
        };
        self.title.set_rect(Rect {
            height: 95.0,
            ..rect
        });
        self.title.render(frame, draw)?;
        self.subtitle.set_rect(Rect {
            y: rect.y + 95.0,
            height: 60.0,
            ..rect
        });
        self.subtitle.render(frame, draw)?;
        self.cancel.set_rect(Rect {
            x: rect.x + rect.width - 386.0,
            y: rect.y,
            width: 386.0,
            height: 125.0,
        });
        self.cancel.render(frame, draw)?;
        self.process_actions(frame, draw);
        let input = Rect {
            x: rect.x + 25.0,
            y: rect.y + 160.0,
            width: rect.width - 25.0,
            height: 100.0,
        };
        if self.options.password_toggle {
            self.input.password = self.options.password;
            self.input.set_rect(Rect {
                width: input.width - 100.0,
                ..input
            });
            self.input.render(frame, draw)?;
            let eye = Rect {
                x: input.x + input.width - 90.0,
                y: input.y,
                width: 80.0,
                height: input.height,
            };
            self.eye.set_rect(eye);
            self.eye.render(frame, draw)?;
            self.process_actions(frame, draw);
            let texture = if self.options.password {
                self.eye_closed
            } else {
                self.eye_open
            };
            texture.draw(
                draw,
                Point {
                    x: eye.x + (eye.width - texture.width) / 2.0,
                    y: eye.y + (eye.height - texture.height) / 2.0,
                },
                1.0,
                WHITE,
            )?;
        } else {
            self.input.set_rect(input);
            self.input.render(frame, draw)?;
        }
        draw.line(
            Point {
                x: input.x,
                y: input.y + input.height - 2.0,
            },
            Point {
                x: input.x + input.width,
                y: input.y + input.height - 2.0,
            },
            3.0,
            u32::from_le_bytes([189, 189, 189, 255]),
        )?;
        if !self
            .buttons
            .get(BACKSPACE)
            .is_some_and(|button| button.state.is_pressed())
        {
            self.backspace_pressed = false;
        }
        if self.backspace_pressed
            && frame.monotonic - self.backspace_start > 0.5
            && frame.monotonic - self.backspace_repeat > 0.07
        {
            self.input.backspace(draw, frame.monotonic);
            self.backspace_repeat = frame.monotonic;
        }
        for (key, rect) in self.layout.rectangles(rect) {
            let enabled = key != ENTER || self.input.len() >= self.options.min_length;
            if key == BACKSPACE
                && self
                    .buttons
                    .get(BACKSPACE)
                    .is_some_and(|button| button.state.is_pressed())
                && !self.backspace_pressed
            {
                self.backspace_pressed = true;
                self.backspace_start = frame.monotonic;
                self.backspace_repeat = frame.monotonic;
            }
            let key = if key == SHIFT_ON && self.caps_lock {
                CAPS
            } else {
                key
            };
            let button = self
                .buttons
                .get_mut(key)
                .ok_or(Error::Contract("keyboard key missing"))?;
            button.state.enabled = enabled.into();
            button.set_rect(rect);
            button.render(frame, draw)?;
            self.process_actions(frame, draw);
        }
        Ok(RenderResult::None)
    }
}
