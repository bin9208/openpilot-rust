//! AlertItem geometry, original textures and UnifiedLabel composition (MIT source).
use super::AlertData;
use crate::paint;
use openpilot_ui_framework::{
    assets::Texture,
    canvas::Canvas,
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{float, Vertical},
    unified_label::UnifiedLabel,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
const WIDTH: f32 = 520.0;
const HEIGHTS: [f32; 3] = [212.0, 240.0, 324.0];
const PADDING: f32 = 28.0;
const ICON: f32 = 64.0;
const TITLE_WIDTH: f32 = WIDTH - PADDING * 2.0 - ICON - 12.0;
const BODY_WIDTH: f32 = WIDTH - PADDING * 2.0;
const SPACING: f64 = 24.0;

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub enum AlertSize {
    Small,
    Medium,
    Big,
}
impl AlertSize {
    fn index(self) -> usize {
        match self {
            Self::Small => 0,
            Self::Medium => 1,
            Self::Big => 2,
        }
    }
}
pub struct AlertItem {
    pub state: WidgetState,
    pub alert_data: AlertData,
    pub alert_size: AlertSize,
    backgrounds: [[Texture; 2]; 3],
    icons: [Texture; 3],
    title: UnifiedLabel,
    body: UnifiedLabel,
    title_text: String,
    body_text: String,
    pending_layout: bool,
}
impl AlertItem {
    pub fn new(data: AlertData, canvas: &mut Canvas) -> Result<Self, Error> {
        let mut title = UnifiedLabel::new("");
        title.size = 32.0;
        title.font = Font::SemiBold;
        title.line_height = 0.95;
        let mut body = UnifiedLabel::new("");
        body.size = 28.0;
        body.font = Font::Regular;
        body.vertical = Vertical::Bottom;
        body.line_height = 0.95;
        let mut backgrounds = Vec::new();
        for (name, height) in [("small", 212), ("medium", 240), ("big", 324)] {
            backgrounds.push([
                paint::texture(
                    canvas,
                    &format!("icons_mici/offroad_alerts/{name}_alert.png"),
                    (520, height),
                )?,
                paint::texture(
                    canvas,
                    &format!("icons_mici/offroad_alerts/{name}_alert_pressed.png"),
                    (520, height),
                )?,
            ]);
        }
        let mut item = Self {
            state: WidgetState::default(),
            alert_data: data.clone(),
            alert_size: AlertSize::Small,
            backgrounds: backgrounds
                .try_into()
                .map_err(|_| Error::Contract("alert background count"))?,
            icons: [
                paint::texture(
                    canvas,
                    "icons_mici/offroad_alerts/orange_warning.png",
                    (64, 64),
                )?,
                paint::texture(
                    canvas,
                    "icons_mici/offroad_alerts/red_warning.png",
                    (64, 64),
                )?,
                paint::texture(
                    canvas,
                    "icons_mici/offroad_alerts/green_wheel.png",
                    (64, 64),
                )?,
            ],
            title,
            body,
            title_text: String::new(),
            body_text: String::new(),
            pending_layout: true,
        };
        item.update_alert_data(data);
        item.prepare_layout(canvas);
        Ok(item)
    }
    pub fn update_alert_data(&mut self, data: AlertData) {
        self.state.visible = (data.visible && !data.text.is_empty()).into();
        if self.state.visible.get() {
            (self.title_text, self.body_text) = split_text(&data.text);
            self.title.text = self.title_text.clone().into();
            self.body.text = self.body_text.clone().into();
            self.pending_layout = true;
        }
        self.alert_data = data;
    }
    pub(super) fn prepare_layout(&mut self, draw: &dyn Draw) {
        if !self.pending_layout || !self.state.visible.get() {
            return;
        }
        let title_height = if self.title_text.is_empty() {
            0.0
        } else {
            self.title.content_height(draw, f64::from(TITLE_WIDTH))
        };
        let body_height = if self.body_text.is_empty() {
            0.0
        } else {
            self.body.content_height(draw, f64::from(BODY_WIDTH))
        };
        let spacing = if self.title_text.is_empty() || self.body_text.is_empty() {
            0.0
        } else {
            SPACING
        };
        let height = title_height + spacing + body_height + f64::from(PADDING * 2.0);
        self.alert_size = if height > 240.0 {
            AlertSize::Big
        } else if height > 212.0 {
            AlertSize::Medium
        } else {
            AlertSize::Small
        };
        self.set_rect(Rect {
            x: 0.0,
            y: 0.0,
            width: WIDTH,
            height: HEIGHTS[self.alert_size.index()],
        });
        self.pending_layout = false;
    }
    pub fn split(&self) -> (&str, &str) {
        (&self.title_text, &self.body_text)
    }
}
impl Widget for AlertItem {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<(), Error> {
        self.prepare_layout(draw);
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        self.backgrounds[self.alert_size.index()][usize::from(self.state.is_pressed())].draw(
            draw,
            Point {
                x: rect.x,
                y: rect.y,
            },
            1.0,
            WHITE,
        )?;
        let x = rect.x + PADDING;
        let mut y = f64::from(rect.y) + f64::from(PADDING);
        if !self.title_text.is_empty() {
            let height = self.title.content_height(draw, f64::from(TITLE_WIDTH));
            self.title.set_rect(Rect {
                x,
                y: float(y),
                width: TITLE_WIDTH,
                height: float(height),
            });
            self.title.render(frame, draw)?;
            y += f64::from(float(height)) + SPACING;
        }
        if !self.body_text.is_empty() {
            self.body.set_rect(Rect {
                x,
                y: float(y),
                width: BODY_WIDTH,
                height: float(f64::from(rect.height) - y + f64::from(rect.y) - f64::from(PADDING)),
            });
            self.body.render(frame, draw)?;
        }
        let icon = if self.alert_data.severity == -1 {
            2
        } else {
            usize::from(self.alert_data.severity > 0)
        };
        self.icons[icon].draw(
            draw,
            Point {
                x: rect.x + WIDTH - PADDING - ICON,
                y: rect.y + PADDING,
            },
            1.0,
            WHITE,
        )?;
        Ok(RenderResult::None)
    }
}
pub fn split_text(text: &str) -> (String, String) {
    for (index, character) in text.char_indices() {
        if matches!(character, '.' | '!' | '?') {
            let suffix = &text[index + character.len_utf8()..];
            if suffix.is_empty()
                || suffix
                    .chars()
                    .next()
                    .is_some_and(openpilot_ui_framework::text::whitespace)
            {
                return (
                    openpilot_ui_framework::text::trim(&text[..index]).into(),
                    openpilot_ui_framework::text::trim(suffix).into(),
                );
            }
        }
    }
    (String::new(), text.into())
}
