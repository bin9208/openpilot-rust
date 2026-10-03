use crate::state::EventType;
use openpilot_cereal::car_capnp::car_control::h_u_d_control::{AudibleAlert, VisualAlert};
use openpilot_cereal::log_capnp::selfdrive_state::{AlertSize, AlertStatus};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Priority {
    #[default]
    Lowest,
    Lower,
    Low,
    Mid,
    High,
    Highest,
}

pub(crate) mod enum_wire {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<T: Copy, S: Serializer>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        u16: From<T>,
    {
        serializer.serialize_u16(u16::from(*value))
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: TryFrom<u16>,
        T::Error: std::fmt::Display,
        D: Deserializer<'de>,
    {
        T::try_from(u16::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Alert {
    pub alert_text_1: String,
    pub alert_text_2: Option<String>,
    #[serde(with = "enum_wire")]
    pub alert_status: AlertStatus,
    #[serde(with = "enum_wire")]
    pub alert_size: AlertSize,
    pub priority: Priority,
    #[serde(with = "enum_wire")]
    pub visual_alert: VisualAlert,
    #[serde(with = "enum_wire")]
    pub audible_alert: AudibleAlert,
    pub duration: i64,
    pub creation_delay: f64,
    pub alert_type: String,
    pub event_type: Option<EventType>,
}

#[derive(Debug, thiserror::Error)]
#[error("null alertText2 rejected at cereal encoding")]
pub struct MissingAlertText;

impl Alert {
    pub fn wire_text_2(&self) -> Result<&str, MissingAlertText> {
        self.alert_text_2.as_deref().ok_or(MissingAlertText)
    }
}

impl Default for Alert {
    fn default() -> Self {
        Self {
            alert_text_1: String::new(),
            alert_text_2: Some(String::new()),
            alert_status: AlertStatus::Normal,
            alert_size: AlertSize::None,
            priority: Priority::Lowest,
            visual_alert: VisualAlert::None,
            audible_alert: AudibleAlert::None,
            duration: 0,
            creation_delay: 0.0,
            alert_type: String::new(),
            event_type: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AlertEntry {
    pub alert: Alert,
    pub start_frame: i64,
    pub end_frame: i64,
    pub added_frame: i64,
}

#[derive(Default)]
pub struct AlertManager {
    entries: Vec<AlertEntry>,
    current: Alert,
}

impl AlertManager {
    pub fn current(&self) -> &Alert {
        &self.current
    }
    pub fn entries(&self) -> &[AlertEntry] {
        &self.entries
    }

    pub fn add_many(&mut self, frame: i64, alerts: impl IntoIterator<Item = Alert>) {
        for alert in alerts {
            if let Some(entry) = self
                .entries
                .iter_mut()
                .find(|entry| entry.alert.alert_type == alert.alert_type)
            {
                if !(frame <= entry.end_frame && frame == entry.added_frame + 1) {
                    entry.start_frame = frame;
                }
                entry.end_frame = (frame + 1).max(entry.start_frame + alert.duration);
                entry.added_frame = frame;
                entry.alert = alert;
            } else {
                self.entries.push(AlertEntry {
                    end_frame: (frame + 1).max(frame + alert.duration),
                    alert,
                    start_frame: frame,
                    added_frame: frame,
                });
            }
        }
    }

    pub fn process_alerts(&mut self, frame: i64, clear_event_types: &[EventType]) -> &Alert {
        let mut selected: Option<&AlertEntry> = None;
        for entry in &mut self.entries {
            if entry
                .alert
                .event_type
                .is_some_and(|event| clear_event_types.contains(&event))
            {
                entry.end_frame = -1;
            }
            if frame <= entry.end_frame
                && selected.is_none_or(|current| {
                    (entry.alert.priority, entry.start_frame)
                        > (current.alert.priority, current.start_frame)
                })
            {
                selected = Some(entry);
            }
        }
        self.current = selected.map_or_else(Alert::default, |entry| entry.alert.clone());
        &self.current
    }
}
