use super::*;
use crate::paint::{self, Image, Text};
use openpilot_ui_framework::{
    draw::{BLACK, WHITE},
    text::Font,
    text_layout,
};

impl Settings {
    pub(super) fn draw(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        draw.rounded(
            Rect {
                width: 500.0,
                ..rect
            },
            0.0,
            BLACK,
        )?;
        self.close_rect = Rect {
            x: rect.x + 150.0,
            y: rect.y + 60.0,
            width: 200.0,
            height: 200.0,
        };
        let pressed = frame.last_event.down && self.close_rect.contains(frame.cursor);
        let shade = if pressed { 59 } else { 41 };
        draw.rounded_segments(
            self.close_rect,
            1.0,
            20,
            paint::color(shade, shade, shade, 255),
            false,
        )?;
        let icon = self.close_icon;
        let shade = if pressed { 220 } else { 255 };
        paint::image(
            draw,
            Image {
                texture: icon,
                rect: Rect {
                    x: self.close_rect.x + (200.0 - icon.width) / 2.0,
                    y: self.close_rect.y + (200.0 - icon.height) / 2.0,
                    width: icon.width,
                    height: icon.height,
                },
                tint: paint::color(shade, shade, shade, 255),
                origin: Point::default(),
                rotation: 0.0,
            },
        )?;
        let mut y = rect.y + 300.0;
        for (index, panel) in self.panels.iter_mut().enumerate() {
            panel.rect = Rect {
                x: rect.x + 50.0,
                y,
                width: 350.0,
                height: 110.0,
            };
            let name = self.context.tr(panel.name);
            let size = text_layout::measure(draw, Font::Medium, &name, 65.0, 0.0);
            paint::text(
                draw,
                Point {
                    x: panel.rect.x + panel.rect.width - size.x,
                    y: y + (110.0 - size.y) / 2.0,
                },
                Text {
                    value: &name,
                    font: Font::Medium,
                    size: 65.0,
                    spacing: 0.0,
                    color: if index == self.current {
                        WHITE
                    } else {
                        paint::color(128, 128, 128, 255)
                    },
                },
            )?;
            y += 110.0;
        }
        draw.rounded_segments(
            Rect {
                x: rect.x + 510.0,
                y: rect.y + 10.0,
                width: rect.width - 520.0,
                height: rect.height - 20.0,
            },
            0.04,
            30,
            paint::color(41, 41, 41, 255),
            false,
        )?;
        let mut panel = self.panels[self.current].widget.borrow_mut()?;
        panel.set_rect(Rect {
            x: rect.x + 550.0,
            y: rect.y + 25.0,
            width: rect.width - 600.0,
            height: rect.height - 50.0,
        });
        panel.render(frame, draw)?;
        Ok(RenderResult::None)
    }
}
