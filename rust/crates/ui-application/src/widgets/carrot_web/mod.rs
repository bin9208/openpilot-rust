pub mod address;
use crate::{
    context::Context,
    paint::{self, Label},
    qr::{texture::Texture, Correction},
};
use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    draw::{Draw, BLACK},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{self, float, Horizontal, Vertical},
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetState},
    Error,
};
pub struct CarrotWeb {
    pub state: WidgetState,
    pub address: address::Watcher,
    pub opened_at: f64,
    pub updated_time: Option<String>,
    context: Context,
    qr: Texture,
}
impl CarrotWeb {
    pub fn new(context: Context) -> Self {
        Self {
            state: WidgetState::default(),
            address: address::Watcher::default(),
            opened_at: 0.0,
            updated_time: None,
            context,
            qr: Texture::new(Correction::Medium),
        }
    }
    fn refresh_time(&mut self) {
        self.updated_time = self
            .address
            .value
            .url
            .as_ref()
            .map(|_| (self.context.now_wall)().format("%H:%M:%S").to_string());
    }
    pub fn seconds_until_close(&self, now: f64) -> i32 {
        (30.0 - (now - self.opened_at))
            .ceil()
            .max(0.0)
            .to_i32()
            .unwrap_or(0)
    }
}
impl Widget for CarrotWeb {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.opened_at = frame.now;
        self.address
            .refresh(self.context.memory.as_ref(), frame.now, true);
        self.refresh_time();
    }
    fn update(&mut self, frame: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        if self
            .address
            .refresh(self.context.memory.as_ref(), frame.now, false)
        {
            self.refresh_time();
        }
        if frame.now - self.opened_at >= 30.0 {
            frame.navigation.push(NavigationRequest::Pop(None));
        }
        Ok(())
    }
    fn mouse_release(
        &mut self,
        _: Point,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        if frame.now - self.opened_at >= 0.35 {
            frame.navigation.push(NavigationRequest::Pop(None));
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        draw.rounded(rect, 0.0, BLACK)?;
        self.qr.set_data(self.address.value.url.as_deref(), draw)?;
        let scale = (f64::from(rect.width) / 536.0).min(f64::from(rect.height) / 240.0);
        let x = f64::from(rect.x) + (f64::from(rect.width) - 536.0 * scale) / 2.0;
        let y = f64::from(rect.y) + (f64::from(rect.height) - 240.0 * scale) / 2.0;
        let qr = Rect {
            x: float(x + 8.0 * scale),
            y: float(y + 8.0 * scale),
            width: float(224.0 * scale),
            height: float(224.0 * scale),
        };
        if !self.qr.draw(draw, qr)? {
            draw.rounded_segments(qr, 0.04, 8, paint::color(38, 38, 38, 255), false)?;
            paint::label(
                draw,
                qr,
                Label {
                    color: paint::color(175, 175, 175, 255),
                    ..Label::new(&self.context.tr("Offline"), (32.0 * scale).trunc())
                },
            )?;
        }
        let text_x = x + 254.0 * scale;
        let width = 274.0 * scale;
        paint::label(
            draw,
            Rect {
                x: float(text_x),
                y: float(y + 42.0 * scale),
                width: float(width),
                height: float(52.0 * scale),
            },
            Label {
                font: Font::Bold,
                color: openpilot_ui_framework::draw::WHITE,
                horizontal: Horizontal::Left,
                ..Label::new("Carrot Web", (38.0 * scale).trunc())
            },
        )?;
        let address = self
            .address
            .value
            .url
            .clone()
            .unwrap_or_else(|| self.context.tr("Offline"));
        let address = address.strip_prefix("http://").unwrap_or(&address);
        let preferred = (26.0 * scale).trunc();
        let measured =
            f64::from(text_layout::measure(draw, Font::Normal, address, preferred, 0.0).x);
        let mut size = if preferred <= 1.0 || measured <= width {
            preferred.max(1.0)
        } else if measured <= 0.0 || width <= 0.0 {
            1.0
        } else {
            (preferred * width / measured).floor().max(1.0)
        };
        while size > 1.0
            && f64::from(text_layout::measure(draw, Font::Normal, address, size, 0.0).x) > width
        {
            size -= 1.0;
        }
        paint::label(
            draw,
            Rect {
                x: float(text_x),
                y: float(y + 104.0 * scale),
                width: float(width),
                height: float(36.0 * scale),
            },
            Label {
                color: paint::color(205, 205, 205, 255),
                horizontal: Horizontal::Left,
                vertical: Vertical::Top,
                elide: false,
                ..Label::new(address, size)
            },
        )?;
        if let Some(time) = &self.updated_time {
            let rect = Rect {
                x: float(text_x),
                y: float(y + 190.0 * scale),
                width: float(width),
                height: float(28.0 * scale),
            };
            paint::label(
                draw,
                rect,
                Label {
                    color: paint::color(135, 135, 135, 255),
                    horizontal: Horizontal::Left,
                    ..Label::new(time, (20.0 * scale).trunc())
                },
            )?;
            paint::label(
                draw,
                rect,
                Label {
                    color: paint::color(135, 135, 135, 255),
                    horizontal: Horizontal::Right,
                    ..Label::new(
                        &format!("{}s", self.seconds_until_close(frame.now)),
                        (20.0 * scale).trunc(),
                    )
                },
            )?;
        }
        Ok(RenderResult::None)
    }
}
