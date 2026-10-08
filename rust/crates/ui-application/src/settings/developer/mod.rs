//! Developer settings policy and large-display presentation from the original layouts.
mod controls;
mod copy;
pub(crate) mod policy;
use crate::{
    context::{Action, Context},
    widgets::ssh::SshAction,
};
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::Draw,
    list::{ListItem, ToggleAction},
    scroller_tici::Scroller,
    widget::{Frame, Property, RenderResult, Widget, WidgetState},
    Error,
};
use policy::{Key, Policy};
use std::rc::Rc;
pub struct Developer {
    state: WidgetState,
    pub scroller: Scroller,
    policy: Rc<Policy>,
}
impl Developer {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let policy = Policy::new(context.clone())?;
        let mut scroller = Scroller {
            spacing: 0.0,
            line_separator: true,
            ..Default::default()
        };
        for (key, title, description) in [
            (Key::Adb, "Enable ADB", copy::ADB),
            (Key::Ssh, "Enable SSH", ""),
            (Key::Joystick, "Joystick Debug Mode", ""),
            (Key::Longitudinal, "Longitudinal Maneuver Mode", ""),
            (
                Key::Alpha,
                "openpilot Longitudinal Control (Alpha)",
                copy::ALPHA,
            ),
            (Key::Debug, "UI Debug Mode", ""),
        ] {
            let mut item = ListItem::new("")?;
            item.title = context.text(title);
            item.description = context.text(description);
            let model = policy.clone();
            item.state.visible =
                Property::Dynamic(Box::new(move || model.visible[key.index()].get()));
            let mut action = ToggleAction::new(policy.checked[key.index()].get());
            let model = policy.clone();
            action.state.enabled = Property::Dynamic(Box::new(move || model.enabled(key)));
            let callback = Policy::callback(&policy, key);
            action.toggle.changed = Some(Box::new(move |value| callback.call(value)));
            item.action = Some(Box::new(controls::Control {
                action,
                checked: policy.checked[key.index()].clone(),
            }));
            scroller.add(Box::new(item));
            if matches!(key, Key::Ssh) {
                let mut item = ListItem::new("")?;
                item.title = context.text("SSH Keys");
                item.description = context.text(copy::SSH);
                item.action = Some(Box::new(SshAction::new(context.clone(), canvas)?));
                scroller.add(Box::new(item));
            }
        }
        Ok(Self {
            state: WidgetState::default(),
            scroller,
            policy,
        })
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
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller.set_rect(self.state.rect);
        self.scroller.render(frame, draw)
    }
}
