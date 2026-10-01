//! Compact developer controls share source parameter policy, not layout behavior.
mod controls;
mod ssh;
use crate::{
    context::{Action, Context},
    paint,
    services::ssh::Fetcher,
    settings::developer::policy::{Key, Policy},
};
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::Draw,
    navigation::NavWidget,
    scroller::Scroller,
    widget::{Frame, Property, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::RefCell, rc::Rc};
pub struct Developer {
    state: WidgetState,
    pub scroller: Scroller,
    policy: Rc<Policy>,
    pub fetcher: Rc<RefCell<Fetcher>>,
}
impl Developer {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        use crate::mici::widgets::{big_button::BigButton, circle_button::CircleButton};
        use openpilot_ui_framework::geometry::Point;
        let policy = Policy::new(context.clone())?;
        let fetcher = Rc::new(RefCell::new(Fetcher::new(
            context.params.raw.clone(),
            context.translations.clone(),
        )));
        let mut scroller = Scroller::new(true, false, !context.pc, 20.0);
        scroller.indicator = Some(paint::texture(
            canvas,
            "icons_mici/settings/horizontal_scroll_indicator.png",
            (96, 48),
        )?);
        for key in [
            Key::Adb,
            Key::Ssh,
            Key::Joystick,
            Key::Longitudinal,
            Key::Alpha,
            Key::Debug,
        ] {
            let callback = Policy::callback(&policy, key);
            let body = if matches!(key, Key::Adb | Key::Ssh) {
                let icon = paint::texture(
                    canvas,
                    if matches!(key, Key::Adb) {
                        "icons_mici/adb_short.png"
                    } else {
                        "icons_mici/ssh_short.png"
                    },
                    (82, 82),
                )?;
                let mut button =
                    CircleButton::new(icon, |path, size| paint::texture(canvas, path, size))?;
                button.enable_toggle(|path, size| paint::texture(canvas, path, size))?;
                button.offset = Point { x: 0.0, y: 12.0 };
                button.changed = Some(callback);
                controls::Body::Circle(Box::new(button))
            } else {
                let text = match key {
                    Key::Joystick => "joystick debug mode",
                    Key::Longitudinal => "longitudinal maneuver mode",
                    Key::Alpha => "alpha longitudinal",
                    Key::Debug => "ui debug mode",
                    Key::Adb | Key::Ssh => {
                        return Err(Error::Contract("unexpected developer big button"))
                    }
                };
                let mut button =
                    BigButton::new(text, |path, size| paint::texture(canvas, path, size))?;
                button.changed = Some(callback);
                controls::Body::Big(Box::new(button))
            };
            let mut button = controls::Control {
                body,
                policy: policy.clone(),
                key,
            };
            let model = policy.clone();
            button.state_mut().enabled = Property::Dynamic(Box::new(move || model.enabled(key)));
            let model = policy.clone();
            button.state_mut().visible =
                Property::Dynamic(Box::new(move || model.visible[key.index()].get()));
            scroller.add(Box::new(button))?;
            if matches!(key, Key::Ssh) {
                scroller.add(Box::new(ssh::Ssh::new(&policy, &fetcher, canvas)?))?;
            }
        }
        Ok(Self {
            state: WidgetState::default(),
            scroller,
            policy,
            fetcher,
        })
    }
    pub fn navigation(self) -> NavWidget {
        NavWidget::new(Box::new(self), 20.0, 240.0)
    }
}
impl Widget for Developer {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.scroller.show(frame);
        if let Err(error) = self.policy.refresh() {
            self.policy.context.actions.push(Action::Failure(error));
        }
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.scroller.hide(frame);
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.fetcher
            .borrow_mut()
            .update()
            .map_err(|error| Error::Io(std::io::Error::other(error)))
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller.state.enabled = self.state.enabled.get().into();
        self.scroller.set_rect(self.state.rect);
        self.scroller.render(frame, draw)
    }
}
