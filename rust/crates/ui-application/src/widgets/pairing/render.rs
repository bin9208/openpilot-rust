use super::*;
use crate::paint::Text;
use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    draw::{BLACK, WHITE},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{self, float},
};
impl Pairing {
    pub(super) fn render_content(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        let rect = self.state.rect;
        draw.clear(paint::color(224, 224, 224, 255))?;
        let content = Rect {
            x: rect.x + 70.0,
            y: rect.y + 70.0,
            width: rect.width - 140.0,
            height: rect.height - 140.0,
        };
        self.close.set_rect(Rect {
            x: content.x - 20.0,
            y: content.y - 20.0,
            width: 120.0,
            height: 120.0,
        });
        let navigation = frame.navigation.clone();
        self.close.state.click = Some(Box::new(move || {
            navigation.push(NavigationRequest::Pop(None))
        }));
        self.close.render(frame, draw)?;
        let mut y = content.y + 120.0;
        let left_width = (content.width * 0.5 - 15.0).trunc();
        let title = text_layout::wrap(
            draw,
            Font::Normal,
            &self.context.tr("Pair your device to your comma account"),
            75.0,
            0.0,
            f64::from(left_width),
        );
        paint::text(
            draw,
            Point { x: content.x, y },
            Text {
                value: &title.join("\n"),
                font: Font::Normal,
                size: 75.0,
                spacing: 0.0,
                color: BLACK,
            },
        )?;
        y += title
            .len()
            .to_f32()
            .ok_or(Error::Contract("pairing lines overflow"))?
            * 75.0
            + 60.0;
        self.instructions(
            draw,
            Rect {
                x: content.x,
                y,
                width: left_width,
                height: content.height - (y - content.y),
            },
        )?;
        let right_width = (content.width / 2.0).floor() - 20.0;
        let size = right_width.min(content.height) - 40.0;
        let qr = Rect {
            x: content.x + left_width + 40.0 + ((right_width - size) / 2.0).floor(),
            y: content.y,
            width: size,
            height: size,
        };
        if !self.qr.draw(draw, qr)? {
            draw.rounded_segments(qr, 0.1, 20, paint::color(240, 240, 240, 255), false)?;
            paint::text(
                draw,
                Point {
                    x: qr.x + 20.0,
                    y: qr.y + (qr.height / 2.0).floor() - 15.0,
                },
                Text {
                    value: &self.context.tr("QR Code Error"),
                    font: Font::Bold,
                    size: 30.0,
                    spacing: 0.0,
                    color: paint::color(230, 41, 55, 255),
                },
            )?;
        }
        Ok(())
    }
    fn instructions(&self, draw: &mut dyn Draw, rect: Rect) -> Result<(), Error> {
        let mut y = rect.y;
        for (index, value) in [
            "Go to https://connect.comma.ai on your phone",
            "Click \"add new device\" and scan the QR code on the right",
            "Bookmark connect.comma.ai to your home screen to use it like an app",
        ]
        .iter()
        .enumerate()
        {
            let lines = text_layout::wrap(
                draw,
                Font::Bold,
                &self.context.tr(value),
                47.0,
                0.0,
                f64::from((rect.width - 90.0).trunc()),
            );
            let height = lines
                .len()
                .to_f32()
                .ok_or(Error::Contract("pairing instruction lines overflow"))?
                * 47.0;
            let circle = Point {
                x: rect.x + 40.0,
                y: y + (height / 2.0).floor(),
            };
            draw.circle(
                Point {
                    x: circle.x.trunc(),
                    y: circle.y.trunc(),
                },
                25.0,
                paint::color(70, 70, 70, 255),
            )?;
            let number = (index + 1).to_string();
            let size = text_layout::measure(draw, Font::Bold, &number, 30.0, 0.0);
            paint::text(
                draw,
                Point {
                    x: float((f64::from(circle.x) - (f64::from(size.x) / 2.0).floor()).trunc()),
                    y: float((f64::from(circle.y) - (f64::from(size.y) / 2.0).floor()).trunc()),
                },
                Text {
                    value: &number,
                    font: Font::Bold,
                    size: 30.0,
                    spacing: 0.0,
                    color: WHITE,
                },
            )?;
            paint::text(
                draw,
                Point {
                    x: rect.x + 90.0,
                    y,
                },
                Text {
                    value: &lines.join("\n"),
                    font: Font::Bold,
                    size: 47.0,
                    spacing: 0.0,
                    color: BLACK,
                },
            )?;
            y += height + 50.0;
        }
        Ok(())
    }
}
