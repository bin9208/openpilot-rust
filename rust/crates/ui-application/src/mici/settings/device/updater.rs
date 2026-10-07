use crate::{
    context::{actions::UpdaterAction, Action, Context},
    mici::widgets::big_button::BigButton,
    paint,
    params::{typed, Read},
};
use openpilot_ui_framework::{
    assets::Texture,
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Waiting,
    Responding,
}
pub(super) struct Updater {
    button: BigButton,
    context: Context,
    icons: [Texture; 3],
    waiting: Option<f64>,
    hide: Option<f64>,
    state: State,
}
impl Updater {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let icons = [
            paint::texture(canvas, "icons_mici/settings/device/update.png", (64, 75))?,
            paint::texture(canvas, "icons_mici/settings/device/reboot.png", (64, 70))?,
            paint::texture(
                canvas,
                "icons_mici/settings/device/up_to_date.png",
                (64, 64),
            )?,
        ];
        let mut button = BigButton::new("update openpilot", |path, size| {
            paint::texture(canvas, path, size)
        })?;
        button.icon = Some(icons[0]);
        Ok(Self {
            button,
            context,
            icons,
            waiting: None,
            hide: None,
            state: State::Idle,
        })
    }
    pub fn offroad(&mut self) {
        if !self.context.ui.borrow().started {
            self.button.state.enabled = true.into();
        }
    }
    fn value(&mut self, value: &str) {
        self.button.value = value.into();
        self.button.text = if value.is_empty() {
            "update openpilot"
        } else {
            ""
        }
        .into();
    }
    fn refresh(&mut self, now: f64) -> Result<(), Error> {
        if self.context.ui.borrow().started {
            self.button.state.enabled = false.into();
            return Ok(());
        }
        let updater = self.context.params.string("UpdaterState")?;
        let failed =
            typed::integer_value(self.context.params.as_ref(), "UpdateFailedCount", false)?
                .is_some_and(|value| value != "0" && !value.starts_with('-'));
        if self.context.params.boolean("UpdateAvailable")? {
            self.button.set_rotate(false, now);
            self.button.state.enabled = true.into();
            if self.button.value != "update now" {
                self.value("update now");
                self.button.icon = Some(self.icons[1]);
            }
        } else {
            match self.state {
                State::Waiting => {
                    self.button.set_rotate(true, now);
                    if updater != "idle" {
                        self.state = State::Responding;
                    }
                    let since = *self.waiting.get_or_insert(now);
                    if now - since > 10.0 {
                        self.button.set_rotate(false, now);
                        self.value("updater failed\nto respond");
                        self.state = State::Idle;
                        self.hide = Some(now);
                    }
                }
                State::Responding => {
                    if updater == "idle" {
                        self.button.set_rotate(false, now);
                        self.state = State::Idle;
                        self.hide = Some(now);
                    } else if self.button.value != updater {
                        self.value(&updater);
                    }
                }
                State::Idle => {
                    self.button.set_rotate(false, now);
                    if failed {
                        if self.button.value != "failed to update" {
                            self.value("failed to update");
                        }
                    } else if self.context.params.boolean("UpdaterFetchAvailable")? {
                        self.button.state.enabled = true.into();
                        if self.button.value != "download update" {
                            self.value("download update");
                        }
                    } else if let Some(since) = self.hide {
                        self.button.state.enabled = true.into();
                        if self.button.value == "checking..." {
                            self.value("up to date");
                            self.button.icon = Some(self.icons[2]);
                        }
                        if now - since > 3.0 {
                            self.hide = None;
                            self.value("");
                            self.button.icon = Some(self.icons[0]);
                        }
                    } else if !self.button.value.is_empty() {
                        self.value("");
                    }
                }
            }
        }
        if self.state != State::Waiting {
            self.waiting = None;
        }
        Ok(())
    }
}
impl Widget for Updater {
    fn state(&self) -> &WidgetState {
        &self.button.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.button.state
    }
    fn set_position(&mut self, x: f32, y: f32) {
        self.button.set_position(x, y);
    }
    fn set_rect(&mut self, rect: Rect) {
        self.button.set_rect(rect);
    }
    fn update(&mut self, frame: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.refresh(frame.now)
    }
    fn mouse_release(
        &mut self,
        position: Point,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.button.mouse_release(position, frame, draw)?;
        if !self.context.system_time_valid()? {
            self.context.actions.push(Action::MiciAlert {
                title: String::new(),
                description: self.context.tr("Please connect to Wi-Fi to update."),
            });
            return Ok(());
        }
        self.button.state.enabled = false.into();
        self.state = State::Waiting;
        self.button.icon = Some(self.icons[0]);
        let action = match self.button.value.as_str() {
            "download update" => UpdaterAction::Download,
            "update now" => UpdaterAction::Reboot,
            _ => UpdaterAction::Check,
        };
        self.context.actions.push(Action::Updater(action));
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.button.paint(frame, draw)
    }
}
