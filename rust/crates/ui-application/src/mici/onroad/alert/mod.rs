mod icons;
mod paint;
use crate::{
    context::Context,
    onroad::alert::{policy::Selection, Alert, Input, Policy},
};
use icons::Side;
use openpilot_ui_framework::{
    animation::{Bounce, Filter},
    assets::Texture,
    canvas::Canvas,
    draw::Draw,
    text::Font,
    unified_label::UnifiedLabel,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub struct Alerts {
    state: WidgetState,
    context: Context,
    policy: Policy,
    previous: Option<Alert>,
    title: UnifiedLabel,
    subtitle: UnifiedLabel,
    y: Bounce,
    alpha: Filter,
    timer: f64,
    signal_alpha: Filter,
    last_side: Option<Side>,
    icons: [Texture; 4],
}
impl Alerts {
    pub fn new(context: Context, canvas: &mut Canvas, fps: f64) -> Result<Self, Error> {
        let mut title = UnifiedLabel::new("");
        title.size = 48.0;
        title.font = Font::Display;
        title.line_height = 0.86;
        title.letter_spacing = -0.02;
        let mut subtitle = UnifiedLabel::new("");
        subtitle.size = 16.0;
        subtitle.font = Font::Regular;
        subtitle.line_height = 0.86;
        subtitle.letter_spacing = 0.025;
        let icons = icons::load(canvas)?;
        Ok(Self {
            state: WidgetState::default(),
            context,
            policy: Policy::new(true, str::to_owned),
            previous: None,
            title,
            subtitle,
            y: Bounce::new(0.0, 0.1, fps, 2.0),
            alpha: Filter::new(0.0, 0.05, fps),
            timer: 0.0,
            signal_alpha: Filter::new(0.0, 0.3, fps),
            last_side: None,
            icons,
        })
    }
    pub fn current(&mut self) -> Result<Option<Alert>, Error> {
        let input = Input::read(&self.context)?;
        Ok(match self.policy.select(&input) {
            Selection::Current(alert) => {
                self.previous = Some(alert.clone());
                Some(alert)
            }
            Selection::Fallback(alert) => Some(alert),
            Selection::None => None,
        })
    }
    pub fn will_render(&mut self) -> Result<(Option<Alert>, bool), Error> {
        let current = self.current()?;
        let absent = current.is_none();
        Ok((current.or_else(|| self.previous.clone()), absent))
    }
}
impl Widget for Alerts {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let current = self.current()?;
        self.y
            .update(f64::from(self.state.rect.y) - if current.is_none() { 50.0 } else { 0.0 });
        self.alpha.update(f64::from(current.is_some()));
        let alert = if let Some(alert) = current {
            alert
        } else if self.alpha.x > 0.01 {
            let Some(alert) = &self.previous else {
                self.previous = None;
                return Ok(RenderResult::Bool(false));
            };
            alert.clone()
        } else {
            self.previous = None;
            return Ok(RenderResult::Bool(false));
        };
        self.background(draw, &alert)?;
        let layout = self.layout_alert(&alert)?;
        self.text(frame, draw, &alert, &layout)?;
        self.icon(draw, &layout)?;
        Ok(RenderResult::Bool(true))
    }
}
