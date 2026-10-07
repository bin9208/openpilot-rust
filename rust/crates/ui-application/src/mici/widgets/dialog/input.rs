use super::*;
use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    animation::Filter, geometry::Point, mici_keyboard::MiciKeyboard, text::Font,
    unified_label::UnifiedLabel,
};
mod render;
pub struct InputOptions {
    pub hint: String,
    pub text: String,
    pub minimum_length: usize,
    pub callback: Option<Rc<dyn Fn(String)>>,
    pub auto_return: String,
}
pub struct InputDialog {
    state: WidgetState,
    hint: UnifiedLabel,
    keyboard: MiciKeyboard,
    minimum_length: usize,
    callback: Option<Rc<dyn Fn(String)>>,
    backspace_held: Option<f64>,
    backspace: Texture,
    enter: Texture,
    enter_disabled: Texture,
    backspace_alpha: Filter,
    enter_alpha: Filter,
    left: Rect,
    right: Rect,
    dismissing: bool,
    confirmed: Option<String>,
}
impl InputDialog {
    pub fn text(&self) -> &str {
        &self.keyboard.text
    }
    pub fn candidate(&self) -> &str {
        self.keyboard.candidate()
    }
    pub fn create(canvas: &mut Canvas, options: InputOptions) -> Result<NavWidget, Error> {
        let mut hint = UnifiedLabel::new(options.hint);
        hint.size = 35.0;
        hint.font = Font::Medium;
        hint.color = paint::color(255, 255, 255, 89);
        let mut keyboard = MiciKeyboard::new(20.0, |path, size| {
            canvas.texture(
                path,
                openpilot_startup_ui::renderer::TextureOptions {
                    width: Some(size.0),
                    height: Some(size.1),
                    keep_aspect: !path.ends_with("keyboard_background.png"),
                    ..Default::default()
                },
            )
        })?;
        keyboard.text = options.text;
        keyboard.auto_return = options.auto_return;
        let input = Self {
            state: WidgetState::default(),
            hint,
            keyboard,
            minimum_length: options.minimum_length,
            callback: options.callback,
            backspace_held: None,
            backspace: paint::texture(
                canvas,
                "icons_mici/settings/keyboard/backspace.png",
                (42, 36),
            )?,
            enter: paint::texture(canvas, "icons_mici/settings/keyboard/enter.png", (76, 62))?,
            enter_disabled: paint::texture(
                canvas,
                "icons_mici/settings/keyboard/enter_disabled.png",
                (76, 62),
            )?,
            backspace_alpha: Filter::new(0.0, 0.05, 20.0),
            enter_alpha: Filter::new(0.0, 0.05, 20.0),
            left: Rect::default(),
            right: Rect::default(),
            dismissing: false,
            confirmed: None,
        };
        let mut nav = navigation(Box::new(input));
        nav.motion.back_area = 0.2;
        nav.on_update = Some(Box::new(|nav, frame| {
            if let Some(input) =
                (nav.content.as_mut() as &mut dyn std::any::Any).downcast_mut::<Self>()
            {
                input.dismissing = nav.motion.is_dismissing();
                input.update_hold(frame);
            }
        }));
        nav.after_content = Some(Box::new(|nav, _| {
            let confirm = (nav.content.as_mut() as &mut dyn std::any::Any)
                .downcast_mut::<Self>()
                .and_then(|input| {
                    input
                        .confirmed
                        .take()
                        .map(|text| (text, input.callback.clone()))
                });
            if let Some((text, callback)) = confirm {
                nav.dismiss(
                    callback.map(|callback| Box::new(move || callback(text)) as Box<dyn FnOnce()>),
                );
            }
        }));
        Ok(nav)
    }
    fn update_hold(&mut self, frame: &Frame<'_>) {
        if self.dismissing {
            self.backspace_held = None;
            return;
        }
        if frame.last_event.down
            && self.right.contains(frame.last_event.pos)
            && self.backspace_alpha.x > 1.0
        {
            let since = *self.backspace_held.get_or_insert(frame.now);
            let interval = (frame.target_fps / 25.0)
                .round_ties_even()
                .to_u64()
                .unwrap_or(1)
                .max(1);
            if frame.now - since > 0.5 && frame.index.is_multiple_of(interval) {
                self.keyboard.backspace();
            }
        } else {
            self.backspace_held = None;
        }
    }
}
impl Widget for InputDialog {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn mouse_press(
        &mut self,
        position: Point,
        _: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        if self.dismissing {
            return Ok(());
        }
        if self.right.contains(position) && self.backspace_alpha.x > 254.0 {
            self.keyboard.backspace();
        } else if self.left.contains(position) && self.enter_alpha.x > 254.0 {
            self.confirmed = Some(self.keyboard.text.clone());
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.draw_input(frame, draw)?;
        self.keyboard.state.enabled = (self.state.enabled.get() && !self.dismissing).into();
        self.keyboard.set_rect(self.state.rect);
        self.keyboard.render(frame, draw)
    }
}
