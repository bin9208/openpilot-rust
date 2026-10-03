use openpilot_ui_framework::{
    animation::Bounce,
    assets::Texture,
    callback::Callback,
    draw::{Draw, ImageDraw, WHITE},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{float, Vertical},
    unified_label::UnifiedLabel,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
mod interaction;
mod paint;
#[derive(Clone, Debug)]
pub enum Kind {
    Button,
    Toggle(bool),
    Multiple { checked: bool, options: Vec<String> },
    Grey,
}
pub struct BigButton {
    pub binding: Option<crate::params::binding::Binding>,
    pub state: WidgetState,
    pub text: String,
    pub font_size: Option<f64>,
    pub value: String,
    pub icon: Option<Texture>,
    pub scroll: bool,
    pub kind: Kind,
    pub changed: Option<Callback<bool>>,
    pub selected: Option<Callback<String>>,
    pub scale: Bounce,
    pub shake_start: Option<f64>,
    pub grow_until: Option<f64>,
    pub rotate_since: Option<f64>,
    pub(crate) label: UnifiedLabel,
    pub(crate) sub_label: UnifiedLabel,
    backgrounds: [Texture; 3],
    pills: [Texture; 2],
    position_base: Option<Point>,
}
impl BigButton {
    pub fn new(
        text: &str,
        mut load: impl FnMut(&str, (i32, i32)) -> Result<Texture, Error>,
    ) -> Result<Self, Error> {
        let backgrounds = [
            load("icons_mici/buttons/button_rectangle.png", (402, 180))?,
            load(
                "icons_mici/buttons/button_rectangle_pressed.png",
                (402, 180),
            )?,
            load(
                "icons_mici/buttons/button_rectangle_disabled.png",
                (402, 180),
            )?,
        ];
        let pills = [
            load("icons_mici/buttons/toggle_pill_disabled.png", (84, 66))?,
            load("icons_mici/buttons/toggle_pill_enabled.png", (84, 66))?,
        ];
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 402.0,
            height: 180.0,
        };
        state.click_delay = Some(0.075);
        let mut label = UnifiedLabel::new(text);
        label.font = Font::Bold;
        label.vertical = Vertical::Bottom;
        label.line_height = 0.9;
        let mut sub_label = UnifiedLabel::new("");
        sub_label.font = Font::Regular;
        sub_label.size = 36.0;
        sub_label.vertical = Vertical::Bottom;
        sub_label.color = u32::from_le_bytes([170, 170, 170, 255]);
        Ok(Self {
            binding: None,
            state,
            text: text.into(),
            font_size: None,
            value: String::new(),
            icon: None,
            scroll: false,
            kind: Kind::Button,
            changed: None,
            selected: None,
            scale: Bounce::new(1.0, 0.1, 20.0, 2.0),
            shake_start: None,
            grow_until: None,
            rotate_since: None,
            label,
            sub_label,
            backgrounds,
            pills,
            position_base: None,
        })
    }
    pub fn refresh_param(&mut self) -> Result<(), Error> {
        if let Some(binding) = &self.binding {
            match &mut self.kind {
                Kind::Toggle(checked) => *checked = binding.boolean()?,
                Kind::Multiple { options, .. } => {
                    let raw = i64::from(binding.integer()?);
                    let length = i64::try_from(options.len())
                        .map_err(|_| Error::Contract("UI option count overflow"))?;
                    let index = usize::try_from(if raw < 0 { length + raw } else { raw })
                        .map_err(|_| Error::Contract("UI option index out of range"))?;
                    self.value = options
                        .get(index)
                        .ok_or(Error::Contract("UI option index out of range"))?
                        .clone();
                }
                Kind::Button | Kind::Grey => {}
            }
        }
        Ok(())
    }
    pub fn set_multiple(&mut self, options: Vec<String>) -> Result<(), Error> {
        self.value = options
            .first()
            .ok_or(Error::Contract("multi-toggle requires options"))?
            .clone();
        self.kind = Kind::Multiple {
            checked: false,
            options,
        };
        Ok(())
    }
    pub fn set_grey(&mut self) {
        self.kind = Kind::Grey;
        self.state.rect.width = 476.0;
        self.state.touch_valid = Some(Box::new(|| false));
        self.label.line_height = 1.0;
        self.sub_label.line_height = 0.95;
        self.sub_label.color = u32::from_le_bytes([255, 255, 255, 229]);
    }
    pub fn set_rotate(&mut self, rotate: bool, now: f64) {
        if rotate && self.rotate_since.is_some() {
            return;
        }
        self.rotate_since = rotate.then_some(now);
    }
    pub fn checked(&self) -> Option<bool> {
        match &self.kind {
            Kind::Toggle(value) | Kind::Multiple { checked: value, .. } => Some(*value),
            Kind::Button | Kind::Grey => None,
        }
    }
    pub fn set_checked(&mut self, value: bool) {
        match &mut self.kind {
            Kind::Toggle(checked) | Kind::Multiple { checked, .. } => *checked = value,
            Kind::Button | Kind::Grey => {}
        }
    }
    pub fn width_hint(&self) -> f64 {
        let padding = if matches!(self.kind, Kind::Grey) {
            30.0
        } else {
            40.0
        };
        let icon = match &self.kind {
            Kind::Multiple { .. } => f64::from(self.pills[1].width),
            Kind::Grey => 0.0,
            Kind::Button | Kind::Toggle(_) => {
                if self.scroll && !self.value.is_empty() {
                    self.icon.map_or(0.0, |icon| f64::from(icon.width))
                } else {
                    0.0
                }
            }
        };
        (f64::from(self.state.rect.width) - padding * 2.0 - icon).trunc()
    }
    fn shake(&self, now: f64) -> f64 {
        match self.shake_start {
            Some(start) if now - start <= 0.5 => {
                let t = now - start;
                (1.0 - t / 0.5) * 24.0 * (t * 32.0).sin()
            }
            _ => 0.0,
        }
    }
}
