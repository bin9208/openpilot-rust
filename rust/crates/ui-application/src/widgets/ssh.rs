use crate::{
    context::{Action, Context},
    paint::{self, Text},
    params::Read,
    services::ssh::Fetcher,
};
use openpilot_ui_framework::{
    button::{Button, ButtonStyle},
    callback::Callback,
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    keyboard::{Keyboard, KeyboardOptions},
    list::ItemAction,
    text::Font,
    text_layout,
    widget::{
        DialogResult, Frame, NavigationRequest, RenderResult, Widget, WidgetHandle, WidgetState,
    },
    Error,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Loading,
    Add,
    Remove,
}
struct State {
    mode: Mode,
    username: String,
}
pub struct SshAction {
    pub state: WidgetState,
    context: Context,
    model: Rc<RefCell<State>>,
    pub fetcher: Rc<RefCell<Fetcher>>,
    keyboard: WidgetHandle,
    button: Button,
    pressed: Rc<Cell<bool>>,
}
impl SshAction {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let keyboard = WidgetHandle::new(Keyboard::new(
            KeyboardOptions {
                min_length: 1,
                ..Default::default()
            },
            |path, size| paint::texture(canvas, path, size),
        )?);
        let model = Rc::new(RefCell::new(State {
            mode: Mode::Add,
            username: String::new(),
        }));
        let fetcher = Rc::new(RefCell::new(Fetcher::new(
            context.params.raw.clone(),
            context.translations.clone(),
        )));
        let (callback_model, callback_fetcher, callback_context) =
            (model.clone(), fetcher.clone(), context.clone());
        let weak_keyboard = keyboard.downgrade();
        keyboard.get_mut::<Keyboard>()?.callback = Some(Callback::new(move |result| {
            if result != DialogResult::Confirm {
                return;
            }
            let result = (|| -> Result<(), crate::Error> {
                let Some(keyboard) = weak_keyboard.upgrade() else {
                    return Ok(());
                };
                let username = keyboard.get::<Keyboard>()?.text();
                let username = username.trim_matches(|ch: char| {
                    ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch)
                });
                if username.is_empty() {
                    return Ok(());
                }
                callback_model.borrow_mut().mode = Mode::Loading;
                let model = callback_model.clone();
                let context = callback_context.clone();
                let response = Callback::new(move |error: Option<String>| {
                    let mut model = model.borrow_mut();
                    if let Some(error) = error {
                        model.mode = Mode::Add;
                        model.username.clear();
                        context.actions.push(Action::Alert(error));
                    } else {
                        model.mode = Mode::Remove;
                        model.username =
                            context.params.string("GithubUsername").unwrap_or_default();
                    }
                });
                callback_fetcher
                    .borrow_mut()
                    .fetch(username.into(), response)?;
                Ok(())
            })();
            if let Err(error) = result {
                callback_context.actions.push(Action::Failure(error));
            }
        }));
        let mut button = Button::new("");
        button.set_style(ButtonStyle::ListAction);
        button.radius = 50.0;
        button.label.size = 35.0;
        button.label.padding = 0.0;
        let pressed = Rc::new(Cell::new(false));
        let flag = pressed.clone();
        button.state.click = Some(Box::new(move || flag.set(true)));
        let mut result = Self {
            state: WidgetState::default(),
            context,
            model,
            fetcher,
            keyboard,
            button,
            pressed,
        };
        result.refresh()?;
        Ok(result)
    }
    fn refresh(&mut self) -> Result<(), Error> {
        let mut model = self.model.borrow_mut();
        model.username = self.context.params.string("GithubUsername")?;
        model.mode = if self.context.params.string("GithubSshKeys")?.is_empty() {
            Mode::Add
        } else {
            Mode::Remove
        };
        Ok(())
    }
    fn click(&mut self, frame: &Frame<'_>) -> Result<(), Error> {
        let mode = self.model.borrow().mode;
        match mode {
            Mode::Add => {
                let mut keyboard = self.keyboard.get_mut::<Keyboard>()?;
                keyboard.reset(None);
                keyboard.set_title(&self.context.tr("Enter your GitHub username"), "");
                drop(keyboard);
                frame
                    .navigation
                    .push(NavigationRequest::Push(self.keyboard.clone()));
            }
            Mode::Remove => {
                self.fetcher
                    .borrow()
                    .clear()
                    .map_err(|error| Error::Io(std::io::Error::other(error)))?;
                self.refresh()?;
            }
            Mode::Loading => {}
        }
        Ok(())
    }
}
impl ItemAction for SshAction {
    fn width_hint(&self, _: &dyn Draw) -> f64 {
        500.0
    }
}
impl Widget for SshAction {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.fetcher
            .borrow_mut()
            .update()
            .map_err(|error| Error::Io(std::io::Error::other(error)))
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let model = self.model.borrow();
        if !model.username.is_empty() {
            let size = text_layout::measure(draw, Font::Normal, &model.username, 48.0, 0.0);
            paint::text(
                draw,
                Point {
                    x: text_layout::float(
                        f64::from(rect.x) + f64::from(rect.width)
                            - 250.0
                            - f64::from(size.x)
                            - 30.0,
                    ),
                    y: text_layout::float(
                        f64::from(rect.y) + (f64::from(rect.height) - f64::from(size.y)) / 2.0,
                    ),
                },
                Text {
                    value: &model.username,
                    font: Font::Normal,
                    size: 48.0,
                    spacing: 1.0,
                    color: paint::color(170, 170, 170, 255),
                },
            )?;
        }
        self.button.set_rect(Rect {
            x: rect.x + rect.width - 250.0,
            y: rect.y + (rect.height - 100.0) / 2.0,
            width: 250.0,
            height: 100.0,
        });
        self.button.label.text = self
            .context
            .tr(match model.mode {
                Mode::Add => "ADD",
                Mode::Remove => "REMOVE",
                Mode::Loading => "LOADING",
            })
            .into();
        self.button.state.enabled = (model.mode != Mode::Loading).into();
        self.button.state.interaction_gate = self.state.interaction_gate
            && self.state.touch_valid.as_ref().is_none_or(|valid| valid());
        drop(model);
        self.button.render(frame, draw)?;
        if self.pressed.replace(false) {
            self.click(frame)?;
        }
        Ok(RenderResult::Bool(false))
    }
}
