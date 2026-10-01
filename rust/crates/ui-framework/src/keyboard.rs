//! Touch keyboard state and input semantics, ported from system/ui/widgets/keyboard.py.
pub mod layout;
mod render;
use crate::{
    assets::Texture,
    button::{Button, ButtonStyle},
    callback::Callback,
    draw::Draw,
    inputbox::InputBox,
    label::Label,
    text::Font,
    text_layout::Horizontal,
    widget::{DialogResult, Frame, NavigationRequest, WidgetState},
    Error,
};
use layout::{Layout, BACKSPACE, CAPS, ENTER, SHIFT_OFF, SHIFT_ON};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};
#[derive(Clone, Copy, Debug)]
pub struct KeyboardOptions {
    pub max_length: usize,
    pub min_length: usize,
    pub password: bool,
    pub password_toggle: bool,
}
impl Default for KeyboardOptions {
    fn default() -> Self {
        Self {
            max_length: 255,
            min_length: 0,
            password: false,
            password_toggle: false,
        }
    }
}
pub struct Keyboard {
    pub state: WidgetState,
    pub input: InputBox,
    pub title: Label,
    pub subtitle: Label,
    pub options: KeyboardOptions,
    pub callback: Option<Callback<DialogResult>>,
    pub layout: Layout,
    pub caps_lock: bool,
    last_shift: f64,
    backspace_pressed: bool,
    backspace_start: f64,
    backspace_repeat: f64,
    cancel: Button,
    eye: Button,
    eye_open: Texture,
    eye_closed: Texture,
    buttons: HashMap<&'static str, Button>,
    actions: Rc<RefCell<VecDeque<&'static str>>>,
}
impl Keyboard {
    pub fn new(
        options: KeyboardOptions,
        mut texture: impl FnMut(&str, (i32, i32)) -> Result<Texture, Error>,
    ) -> Result<Self, Error> {
        let actions = Rc::new(RefCell::new(VecDeque::new()));
        let make = |key: &'static str, style: ButtonStyle| {
            let mut button = Button::new(key);
            button.set_style(style);
            let queue = actions.clone();
            button.state.click = Some(Box::new(move || queue.borrow_mut().push_back(key)));
            button
        };
        let mut cancel = make("Cancel", ButtonStyle::Normal);
        cancel.label.text = "Cancel".to_owned().into();
        let mut eye = make("eye", ButtonStyle::Transparent);
        eye.label.text = String::new().into();
        let eye_open = texture("icons/eye_open.png", (81, 54))?;
        let eye_closed = texture("icons/eye_closed.png", (81, 54))?;
        let mut icons = HashMap::new();
        for (key, path) in [
            (BACKSPACE, "backspace"),
            (SHIFT_OFF, "shift"),
            (SHIFT_ON, "shift-fill"),
            (CAPS, "capslock-fill"),
            (ENTER, "arrow-right"),
        ] {
            icons.insert(key, texture(&format!("icons/{path}.png"), (80, 80))?);
        }
        let mut buttons = HashMap::new();
        for layout in [
            Layout::Lowercase,
            Layout::Uppercase,
            Layout::Numbers,
            Layout::Specials,
        ] {
            for key in layout.rows().into_iter().flatten().copied().chain([CAPS]) {
                let mut button = make(
                    key,
                    if key == ENTER {
                        ButtonStyle::Primary
                    } else {
                        ButtonStyle::Keyboard
                    },
                );
                button.state.multi_touch = true;
                if let Some(icon) = icons.get(key) {
                    button.label.text = String::new().into();
                    button.label.icon = Some(*icon);
                } else {
                    button.label.size = 85.0;
                }
                buttons.insert(key, button);
            }
        }
        let mut title = Label::new("");
        title.size = 90.0;
        title.font = Font::Bold;
        title.horizontal = Horizontal::Left;
        title.padding = 20.0;
        let mut subtitle = Label::new("");
        subtitle.size = 55.0;
        subtitle.horizontal = Horizontal::Left;
        subtitle.padding = 20.0;
        Ok(Self {
            state: WidgetState::default(),
            input: InputBox::new(options.max_length, false),
            title,
            subtitle,
            options,
            callback: None,
            layout: Layout::Lowercase,
            caps_lock: false,
            last_shift: 0.0,
            backspace_pressed: false,
            backspace_start: 0.0,
            backspace_repeat: 0.0,
            cancel,
            eye,
            eye_open,
            eye_closed,
            buttons,
            actions,
        })
    }
    pub fn text(&self) -> String {
        self.input.text()
    }
    pub fn set_text(&mut self, text: &str) {
        self.input.replace_text(text);
    }
    pub fn clear(&mut self) {
        self.layout = Layout::Lowercase;
        self.caps_lock = false;
        self.input.clear();
        self.backspace_pressed = false;
    }
    pub fn reset(&mut self, minimum: Option<usize>) {
        if let Some(value) = minimum {
            self.options.min_length = value;
        }
        self.last_shift = 0.0;
        self.backspace_pressed = false;
        self.backspace_start = 0.0;
        self.backspace_repeat = 0.0;
        self.clear();
    }
    pub fn set_title(&mut self, title: &str, subtitle: &str) {
        self.title.text = title.to_owned().into();
        self.subtitle.text = subtitle.to_owned().into();
    }
    pub fn set_cancel_text(&mut self, text: &str) {
        self.cancel.label.text = text.to_owned().into();
    }
    pub fn key(&mut self, key: &str, draw: &dyn Draw, now: f64) {
        match key {
            CAPS | "ABC" => {
                self.caps_lock = false;
                self.layout = Layout::Lowercase;
            }
            SHIFT_OFF => {
                self.last_shift = now;
                self.layout = Layout::Uppercase;
            }
            SHIFT_ON => {
                if now - self.last_shift < 0.5 {
                    self.caps_lock = true;
                } else {
                    self.layout = Layout::Lowercase;
                }
            }
            "123" => self.layout = Layout::Numbers,
            "#+=" => self.layout = Layout::Specials,
            BACKSPACE => {
                self.input.backspace(draw, now);
            }
            _ => {
                self.input.add(key, draw, now);
                if !self.caps_lock && self.layout == Layout::Uppercase {
                    self.layout = Layout::Lowercase;
                }
            }
        }
    }
    fn finish(&self, result: DialogResult, frame: &Frame<'_>) {
        let callback = self.callback.clone();
        frame
            .navigation
            .push(NavigationRequest::Pop(Some(Box::new(move || {
                if let Some(callback) = callback {
                    callback.call(result);
                }
            }))));
    }
    fn process_actions(&mut self, frame: &Frame<'_>, draw: &dyn Draw) {
        loop {
            let action = self.actions.borrow_mut().pop_front();
            let Some(action) = action else { break };
            match action {
                "Cancel" => {
                    self.clear();
                    self.finish(DialogResult::Cancel, frame);
                }
                "eye" => self.options.password = !self.options.password,
                ENTER => self.finish(DialogResult::Confirm, frame),
                key => self.key(key, draw, frame.monotonic),
            }
        }
    }
}
