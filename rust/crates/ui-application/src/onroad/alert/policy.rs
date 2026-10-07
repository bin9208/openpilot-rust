use crate::{context::Context, state::messages, Error};
use serde::{Deserialize, Serialize};
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alert {
    pub text1: String,
    pub text2: String,
    pub size: u16,
    pub status: u16,
    #[serde(default)]
    pub visual_alert: u16,
    #[serde(default)]
    pub alert_type: String,
}
#[derive(Serialize, Deserialize)]
pub struct Input {
    pub current: Alert,
    pub enabled: bool,
    pub updated: bool,
    pub receive_frame: i64,
    pub receive_time: f64,
    pub started_frame: i64,
    pub started_time: f64,
    pub now: f64,
    pub tici: bool,
}
impl Input {
    pub fn read(context: &Context) -> Result<Self, Error> {
        let messages = context.messages.borrow();
        let state = &messages.state;
        let topic = state.topic("selfdriveState")?;
        let ss = messages::selfdrive_state(state)?;
        let ui = context.ui.borrow();
        let text = |value: capnp::text::Reader<'_>| -> Result<String, Error> {
            Ok(value
                .to_str()
                .map_err(|_| Error::Contract("invalid alert text"))?
                .to_owned())
        };
        Ok(Self {
            current: Alert {
                text1: text(ss.get_alert_text1()?)?,
                text2: text(ss.get_alert_text2()?)?,
                size: ss.get_alert_size()?.into(),
                status: ss.get_alert_status()?.into(),
                visual_alert: ss.get_alert_hud_visual()?.into(),
                alert_type: text(ss.get_alert_type()?)?,
            },
            enabled: ss.get_enabled(),
            updated: topic.updated,
            receive_frame: topic.receive_frame,
            receive_time: topic.receive_time,
            started_frame: ui.started_frame,
            started_time: ui.started_time,
            now: (context.now_monotonic)(),
            tici: !context.pc,
        })
    }
}
pub struct Policy {
    pub compact: bool,
    startup: Alert,
    timeout: Alert,
    reboot: Alert,
}
pub enum Selection {
    None,
    Current(Alert),
    Fallback(Alert),
}
impl Policy {
    pub fn new(compact: bool, tr: impl Fn(&str) -> String) -> Self {
        let translate = |s: &str| if compact { s.to_owned() } else { tr(s) };
        Self {
            compact,
            startup: Alert {
                text1: translate("openpilot Unavailable"),
                text2: translate("Waiting to start"),
                size: 2,
                ..Default::default()
            },
            timeout: Alert {
                text1: translate("TAKE CONTROL IMMEDIATELY"),
                text2: translate("System Unresponsive"),
                size: 3,
                status: 2,
                ..Default::default()
            },
            reboot: Alert {
                text1: translate("System Unresponsive"),
                text2: translate("Reboot Device"),
                size: if compact { 3 } else { 2 },
                status: if compact { 2 } else { 0 },
                ..Default::default()
            },
        }
    }
    pub fn get(&self, input: &Input) -> Option<Alert> {
        match self.select(input) {
            Selection::None => None,
            Selection::Current(alert) | Selection::Fallback(alert) => Some(alert),
        }
    }
    pub fn select(&self, input: &Input) -> Selection {
        if !input.updated {
            let waiting = input.receive_frame < input.started_frame;
            if waiting && input.now - input.started_time > 5.0 {
                return Selection::Fallback(self.startup.clone());
            }
            if input.tici && !waiting {
                let missing = input.now - input.receive_time;
                if missing > 5.0 {
                    return Selection::Fallback(if input.enabled && missing - 5.0 < 10.0 {
                        self.timeout.clone()
                    } else {
                        self.reboot.clone()
                    });
                }
            }
        }
        if input.current.size == 0 || (!self.compact && input.receive_frame < input.started_frame) {
            Selection::None
        } else {
            Selection::Current(input.current.clone())
        }
    }
}
