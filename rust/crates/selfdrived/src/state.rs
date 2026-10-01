use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EventType {
    Enable,
    PreEnable,
    OverrideLateral,
    OverrideLongitudinal,
    NoEntry,
    Warning,
    UserDisable,
    SoftDisable,
    ImmediateDisable,
    Permanent,
}

impl EventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enable => "enable",
            Self::PreEnable => "preEnable",
            Self::OverrideLateral => "overrideLateral",
            Self::OverrideLongitudinal => "overrideLongitudinal",
            Self::NoEntry => "noEntry",
            Self::Warning => "warning",
            Self::UserDisable => "userDisable",
            Self::SoftDisable => "softDisable",
            Self::ImmediateDisable => "immediateDisable",
            Self::Permanent => "permanent",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    #[default]
    Disabled,
    PreEnabled,
    Enabled,
    SoftDisabling,
    Overriding,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Flags {
    pub enabled: bool,
    pub active: bool,
}

impl State {
    pub const fn flags(self) -> Flags {
        match self {
            Self::Disabled => Flags {
                enabled: false,
                active: false,
            },
            Self::PreEnabled => Flags {
                enabled: true,
                active: false,
            },
            Self::Enabled | Self::SoftDisabling | Self::Overriding => Flags {
                enabled: true,
                active: true,
            },
        }
    }
}

#[derive(Serialize)]
pub struct StateMachine {
    pub state: State,
    pub soft_disable_timer: u32,
    pub current_alert_types: Vec<EventType>,
}

impl Default for StateMachine {
    fn default() -> Self {
        let mut current_alert_types = Vec::with_capacity(4);
        current_alert_types.push(EventType::Permanent);
        Self {
            state: State::Disabled,
            soft_disable_timer: 0,
            current_alert_types,
        }
    }
}

impl StateMachine {
    pub fn update(&mut self, events: &[EventType]) -> Flags {
        use EventType as E;
        self.soft_disable_timer = self.soft_disable_timer.saturating_sub(1);
        self.current_alert_types.clear();
        self.current_alert_types.push(E::Permanent);
        let has = |event| events.contains(&event);
        let overriding = has(E::OverrideLateral) || has(E::OverrideLongitudinal);
        if self.state.flags().enabled && has(E::UserDisable) {
            self.state = State::Disabled;
            self.current_alert_types.push(E::UserDisable);
        } else if self.state.flags().enabled && has(E::ImmediateDisable) {
            self.state = State::Disabled;
            self.current_alert_types.push(E::ImmediateDisable);
        } else {
            match self.state {
                State::Disabled => {
                    if has(E::Enable) {
                        if has(E::NoEntry) {
                            self.current_alert_types.push(E::NoEntry);
                        } else {
                            self.state = if has(E::PreEnable) {
                                State::PreEnabled
                            } else if overriding {
                                State::Overriding
                            } else {
                                State::Enabled
                            };
                            self.current_alert_types.push(E::Enable);
                        }
                    }
                }
                State::Enabled => {
                    if has(E::SoftDisable) {
                        self.soft_disable();
                    } else if overriding {
                        self.state = State::Overriding;
                        self.overrides();
                    }
                }
                State::SoftDisabling => {
                    if !has(E::SoftDisable) {
                        self.state = State::Enabled;
                    } else if self.soft_disable_timer > 0 {
                        self.current_alert_types.push(E::SoftDisable);
                    } else {
                        self.state = State::Disabled;
                    }
                }
                State::PreEnabled => {
                    if has(E::PreEnable) {
                        self.current_alert_types.push(E::PreEnable);
                    } else {
                        self.state = State::Enabled;
                    }
                }
                State::Overriding => {
                    if has(E::SoftDisable) {
                        self.soft_disable();
                    } else if overriding {
                        self.overrides();
                    } else {
                        self.state = State::Enabled;
                    }
                }
            }
        }
        let flags = self.state.flags();
        if flags.active {
            self.current_alert_types.push(E::Warning);
        }
        flags
    }

    fn soft_disable(&mut self) {
        self.state = State::SoftDisabling;
        self.soft_disable_timer = 300;
        self.current_alert_types.push(EventType::SoftDisable);
    }

    fn overrides(&mut self) {
        self.current_alert_types
            .extend([EventType::OverrideLateral, EventType::OverrideLongitudinal]);
    }
}
