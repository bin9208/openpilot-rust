use crate::{
    context::{Action, Confirmation, Context, Event},
    params::Read,
};
use openpilot_ui_framework::{callback::Callback, widget::DialogResult};
use std::{cell::Cell, rc::Rc};
#[derive(Clone, Copy)]
pub(crate) enum Key {
    Adb,
    Ssh,
    Joystick,
    Longitudinal,
    Alpha,
    Debug,
}
pub(crate) const KEYS: [Key; 6] = [
    Key::Adb,
    Key::Ssh,
    Key::Joystick,
    Key::Longitudinal,
    Key::Alpha,
    Key::Debug,
];
impl Key {
    pub fn index(self) -> usize {
        match self {
            Self::Adb => 0,
            Self::Ssh => 1,
            Self::Joystick => 2,
            Self::Longitudinal => 3,
            Self::Alpha => 4,
            Self::Debug => 5,
        }
    }
    pub fn param(self) -> &'static str {
        match self {
            Self::Adb => "AdbEnabled",
            Self::Ssh => "SshEnabled",
            Self::Joystick => "JoystickDebugMode",
            Self::Longitudinal => "LongitudinalManeuverMode",
            Self::Alpha => "AlphaLongitudinalEnabled",
            Self::Debug => "ShowDebugInfo",
        }
    }
}
pub(crate) struct Policy {
    pub context: Context,
    pub checked: [Rc<Cell<bool>>; 6],
    pub visible: [Cell<bool>; 6],
    long_enabled: Cell<bool>,
    is_release: bool,
}
impl Policy {
    pub fn new(context: Context) -> Result<Rc<Self>, crate::Error> {
        let is_release = if context.big {
            context.params.boolean("IsReleaseBranch")?
        } else {
            context.ui.borrow().is_release
        };
        let checked = std::array::from_fn(|_| Rc::new(Cell::new(false)));
        let visible = std::array::from_fn(|_| Cell::new(true));
        let policy = Rc::new(Self {
            context,
            checked,
            visible,
            long_enabled: Cell::new(true),
            is_release,
        });
        for key in KEYS {
            policy.checked[key.index()].set(policy.context.params.boolean(key.param())?);
        }
        if !policy.context.big {
            for key in [Key::Joystick, Key::Longitudinal, Key::Alpha] {
                policy.visible[key.index()].set(!is_release);
            }
        }
        let debug = policy.checked[Key::Debug.index()].get();
        if policy.context.big {
            policy.context.params.put_bool(Key::Debug.param(), debug)?;
        }
        if policy.context.big || debug {
            policy.debug(debug);
        }
        let weak = Rc::downgrade(&policy);
        let actions = policy.context.actions.clone();
        policy.context.listen(
            Event::Offroad,
            Callback::new(move |()| {
                if let Some(policy) = weak.upgrade() {
                    if let Err(error) = policy.refresh() {
                        actions.push(Action::Failure(error));
                    }
                }
            }),
        );
        Ok(policy)
    }
    pub fn enabled(&self, key: Key) -> bool {
        match key {
            Key::Adb | Key::Joystick => !self.context.ui.borrow().started,
            Key::Longitudinal => self.long_enabled.get(),
            Key::Alpha => !self.context.ui.borrow().engaged,
            Key::Ssh | Key::Debug => true,
        }
    }
    pub fn refresh(&self) -> Result<(), crate::Error> {
        self.context.refresh_params()?;
        if self.context.big {
            for key in [Key::Joystick, Key::Longitudinal, Key::Alpha] {
                self.visible[key.index()].set(!self.is_release);
            }
        }
        let ui = self.context.ui.borrow();
        if let Some(car) = ui.slow.car {
            let alpha = car.alpha_longitudinal_available && !self.is_release;
            self.visible[Key::Alpha.index()].set(alpha);
            if !alpha {
                self.context.params.remove(Key::Alpha.param())?;
            }
            let enabled = ui.slow.has_longitudinal_control && !ui.started;
            self.long_enabled.set(enabled);
            if !enabled {
                self.checked[Key::Longitudinal.index()].set(false);
                self.context
                    .params
                    .put_bool(Key::Longitudinal.param(), false)?;
            }
        } else {
            self.long_enabled.set(false);
            self.visible[Key::Alpha.index()].set(false);
        }
        drop(ui);
        for key in KEYS {
            self.checked[key.index()].set(self.context.params.boolean(key.param())?);
        }
        Ok(())
    }
    fn debug(&self, value: bool) {
        self.context.actions.push(Action::ShowTouches(value));
        self.context.actions.push(Action::ShowFps(value));
    }
    pub fn callback(policy: &Rc<Self>, key: Key) -> Callback<bool> {
        let weak = Rc::downgrade(policy);
        Callback::new(move |value| {
            if let Some(policy) = weak.upgrade() {
                if let Err(error) = policy.toggle(key, value) {
                    policy.context.actions.push(Action::Failure(error));
                }
            }
        })
    }
    fn toggle(self: &Rc<Self>, key: Key, value: bool) -> Result<(), crate::Error> {
        self.checked[key.index()].set(value);
        match key {
            Key::Adb | Key::Ssh => self.context.params.put_bool(key.param(), value)?,
            Key::Debug => {
                if self.context.big {
                    self.context.params.put_bool(key.param(), value)?;
                    self.debug(value);
                } else {
                    self.debug(value);
                    self.context.params.put_bool(key.param(), value)?;
                }
            }
            Key::Joystick | Key::Longitudinal => {
                let other = match key {
                    Key::Joystick => Key::Longitudinal,
                    Key::Longitudinal => Key::Joystick,
                    Key::Adb | Key::Ssh | Key::Alpha | Key::Debug => {
                        return Err(crate::Error::Contract(
                            "invalid developer toggle counterpart",
                        ))
                    }
                };
                self.context.params.put_bool(key.param(), value)?;
                self.context.params.put_bool(other.param(), false)?;
                self.checked[other.index()].set(false);
                if !self.context.big && matches!(key, Key::Longitudinal) {
                    self.context.params.put_bool("OnroadCycleRequested", true)?;
                }
            }
            Key::Alpha => {
                if value && self.context.big {
                    let weak = Rc::downgrade(self);
                    let callback = Callback::new(move |result| {
                        if let Some(policy) = weak.upgrade() {
                            let result = (|| -> Result<(), crate::Error> {
                                if result == DialogResult::Confirm {
                                    policy.context.params.put_bool(Key::Alpha.param(), true)?;
                                    policy
                                        .context
                                        .params
                                        .put_bool("OnroadCycleRequested", true)?;
                                    policy.refresh()?;
                                } else {
                                    policy.checked[Key::Alpha.index()].set(false);
                                }
                                Ok(())
                            })();
                            if let Err(error) = result {
                                policy.context.actions.push(Action::Failure(error));
                            }
                        }
                    });
                    self.context.actions.push(Action::Confirm(Confirmation {
                        text: format!(
                            "<h1>{}</h1><br><p>{}</p>",
                            self.context.tr("openpilot Longitudinal Control (Alpha)"),
                            self.context.tr(super::copy::ALPHA)
                        ),
                        confirm: self.context.tr("Enable"),
                        cancel: self.context.tr("Cancel"),
                        rich: true,
                        callback,
                    }));
                } else {
                    self.context.params.put_bool(key.param(), value)?;
                    self.context.params.put_bool("OnroadCycleRequested", true)?;
                    self.refresh()?;
                }
            }
        }
        Ok(())
    }
}
