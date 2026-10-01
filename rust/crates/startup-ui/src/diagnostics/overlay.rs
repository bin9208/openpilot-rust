use crate::{
    geometry::{MouseEvent, Point},
    number,
    renderer::Renderer,
    Error,
};
use std::collections::VecDeque;
#[derive(Default)]
pub struct TouchHistory(VecDeque<(Point, f64)>);
impl TouchHistory {
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        events: &[MouseEvent],
        now: f64,
    ) -> Result<(), Error> {
        let scale = renderer.config.scale;
        for event in events {
            if event.pressed {
                self.0.clear();
            }
            if self.0.len() == 140 {
                self.0.pop_front();
            }
            self.0.push_back((
                Point {
                    x: event.pos.x * scale,
                    y: event.pos.y * scale,
                },
                now,
            ));
        }
        while self.0.front().is_some_and(|(_, time)| now - time > 3.0) {
            self.0.pop_front();
        }
        if let Some((position, _)) = self.0.back() {
            renderer.circle(
                Point {
                    x: position.x.trunc(),
                    y: position.y.trunc(),
                },
                15.0,
                u32::from_le_bytes([230, 41, 55, 255]),
            );
        }
        use num_traits::ToPrimitive;
        for (index, (position, _)) in self.0.iter().enumerate() {
            let percentage = index
                .to_f64()
                .ok_or(Error::Contract("touch index overflow"))?
                / self
                    .0
                    .len()
                    .to_f64()
                    .ok_or(Error::Contract("touch count overflow"))?;
            let red = (255.0 * (1.5 - percentage))
                .min(255.0)
                .to_u8()
                .ok_or(Error::Contract("touch red overflow"))?;
            let green = (255.0 * (percentage + 0.5))
                .min(255.0)
                .to_u8()
                .ok_or(Error::Contract("touch green overflow"))?;
            renderer.circle(
                Point {
                    x: position.x.trunc(),
                    y: position.y.trunc(),
                },
                5.0,
                u32::from_le_bytes([red, green, 50, 255]),
            );
        }
        Ok(())
    }
}
pub fn grid(renderer: &mut Renderer, size: i32) -> Result<(), Error> {
    if size <= 0 {
        return Ok(());
    }
    let width = number::integer(renderer.dimensions().0 * renderer.config.scale)?;
    let width = width + width % 2;
    let height = number::integer(renderer.dimensions().1 * renderer.config.scale)?;
    let height = height + height % 2;
    let color = u32::from_le_bytes([60, 60, 60, 255]);
    let step = usize::try_from(size).map_err(|_| Error::Contract("invalid grid size"))?;
    for x in (0..=width).step_by(step) {
        renderer.line(
            Point {
                x: number::float(x),
                y: 0.0,
            },
            Point {
                x: number::float(x),
                y: number::float(height),
            },
            1.0,
            color,
        );
    }
    for y in (0..=height).step_by(step) {
        renderer.line(
            Point {
                x: 0.0,
                y: number::float(y),
            },
            Point {
                x: number::float(width),
                y: number::float(y),
            },
            1.0,
            color,
        );
    }
    Ok(())
}
