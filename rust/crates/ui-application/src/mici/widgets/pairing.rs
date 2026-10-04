use crate::{context::Context, paint, qr::texture::Texture, widgets::pairing::pairing_url};
use openpilot_ui_framework::{
    assets,
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    navigation::NavWidget,
    text::Font,
    unified_label::UnifiedLabel,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub struct Pairing {
    state: WidgetState,
    pub url: Box<dyn FnMut() -> String>,
    pub last_generation: f64,
    qr: Texture,
    label: UnifiedLabel,
    icon: assets::Texture,
}
impl Pairing {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let mut label = UnifiedLabel::new("pair with comma connect");
        label.font = Font::Bold;
        label.size = 48.0;
        label.line_height = 0.8;
        Ok(Self {
            state: WidgetState::default(),
            url: Box::new(move || pairing_url(&context)),
            last_generation: f64::NEG_INFINITY,
            qr: Texture::mici(),
            label,
            icon: paint::texture(canvas, "icons_mici/settings/device/pair.png", (33, 60))?,
        })
    }
    pub fn navigation(self, context: Context, canvas: &Canvas) -> NavWidget {
        let mut nav = NavWidget::new(
            Box::new(self),
            20.0,
            f64::from(canvas.renderer.config.height()),
        );
        nav.on_update = Some(Box::new(move |nav, _| {
            if context.prime.is_paired() && !nav.motion.is_dismissing() {
                nav.dismiss(None);
            }
        }));
        nav
    }
}
impl Widget for Pairing {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if frame.monotonic - self.last_generation >= 300.0 {
            let url = (self.url)();
            self.qr.destroy();
            self.qr.set_data(Some(&url), draw)?;
            self.last_generation = frame.monotonic;
        }
        let rect = self.state.rect;
        let position = Point {
            x: (rect.x + 8.0).round_ties_even(),
            y: rect.y.round_ties_even(),
        };
        if !self.qr.draw(
            draw,
            Rect {
                x: position.x,
                y: position.y,
                width: rect.height,
                height: rect.height,
            },
        )? {
            paint::text(
                draw,
                Point {
                    x: rect.x + 20.0,
                    y: rect.y + (rect.height / 2.0).floor() - 15.0,
                },
                paint::Text {
                    value: "QR Code Error",
                    font: Font::Bold,
                    size: 30.0,
                    spacing: 0.0,
                    color: paint::color(230, 41, 55, 255),
                },
            )?;
        }
        let x = rect.x + 8.0 + rect.height + 24.0;
        self.label
            .set_max_width(draw, Some(f64::from((rect.width - x).trunc())));
        self.label.set_position(x, rect.y + 16.0);
        self.label.render(frame, draw)?;
        self.icon.draw(
            draw,
            Point {
                x,
                y: rect.y + rect.height - self.icon.height - 16.0,
            },
            1.0,
            paint::color(255, 255, 255, 89),
        )?;
        Ok(RenderResult::None)
    }
}
