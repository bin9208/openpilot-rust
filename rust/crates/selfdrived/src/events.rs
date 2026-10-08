use crate::alerts::{enum_wire, Alert};
use crate::state::EventType;
use openpilot_cereal::log_capnp::onroad_event::EventName;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum Callback {
    AudioFeedbackAlert,
    BelowEngageSpeedAlert,
    BelowSteerSpeedAlert,
    CalibrationIncompleteAlert,
    CalibrationInvalidAlert,
    CameraMalfunctionAlert,
    CarParserResult,
    CommIssueAlert,
    InvalidLkasSettingAlert,
    JoystickAlert,
    LongitudinalManeuverAlert,
    LowMemoryAlert,
    ModeldLaggingAlert,
    OutOfSpaceAlert,
    OverheatAlert,
    ParamsdInvalidAlert,
    PersonalityChangedAlert,
    PosenetInvalidAlert,
    ProcessNotRunningAlert,
    SoftDisableAlert { text: String },
    StartupMasterAlert,
    TorqueNnLoadAlert,
    UserSoftDisableAlert { text: String },
    WrongCarModeAlert,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Definition {
    Static { alert: Alert },
    Callback { callback: Callback },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Category {
    pub category: EventType,
    pub definition: Definition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EventDefinition {
    #[serde(with = "enum_wire")]
    pub event: EventName,
    pub name: String,
    pub definitions: Vec<Category>,
}

#[derive(Deserialize)]
struct CatalogData {
    tici: Vec<EventDefinition>,
    mici: Vec<EventDefinition>,
}

pub struct Catalog {
    definitions: BTreeMap<u16, EventDefinition>,
}

impl Catalog {
    pub fn load(mici: bool) -> Result<Self, serde_json::Error> {
        let data: CatalogData = serde_json::from_str(include_str!("../data/alerts.json"))?;
        let definitions = if mici { data.mici } else { data.tici };
        Ok(Self {
            definitions: definitions
                .into_iter()
                .map(|entry| (entry.event.into(), entry))
                .collect(),
        })
    }

    pub fn get(&self, event: EventName) -> Option<&EventDefinition> {
        self.definitions.get(&event.into())
    }

    pub fn definitions(&self) -> impl Iterator<Item = &EventDefinition> {
        self.definitions.values()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateAlertError<E> {
    #[error("event {0:?} has no alert definition")]
    UndefinedEvent(EventName),
    #[error("alert callback failed: {0}")]
    Callback(E),
}

pub struct Events {
    catalog: Catalog,
    events: Vec<EventName>,
    static_events: Vec<EventName>,
    counters: BTreeMap<u16, u64>,
}

impl Events {
    pub fn new(catalog: Catalog) -> Self {
        Self {
            counters: catalog.definitions.keys().map(|key| (*key, 0)).collect(),
            catalog,
            events: Vec::new(),
            static_events: Vec::new(),
        }
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn names(&self) -> &[EventName] {
        &self.events
    }
    pub fn static_names(&self) -> &[EventName] {
        &self.static_events
    }

    pub fn counters(&self) -> &BTreeMap<u16, u64> {
        &self.counters
    }

    pub fn add(&mut self, event: EventName, static_event: bool) {
        let insert = |events: &mut Vec<EventName>| {
            let index = events.partition_point(|existing| u16::from(*existing) <= u16::from(event));
            events.insert(index, event);
        };
        if static_event {
            insert(&mut self.static_events);
        }
        insert(&mut self.events);
    }

    pub fn clear(&mut self) {
        for (event, counter) in &mut self.counters {
            *counter = if self.events.iter().any(|item| u16::from(*item) == *event) {
                *counter + 1
            } else {
                0
            };
        }
        self.events.clone_from(&self.static_events);
    }

    pub fn contains(&self, category: EventType) -> bool {
        self.events.iter().any(|event| {
            self.categories(*event)
                .any(|item| item.category == category)
        })
    }

    pub fn categories(&self, event: EventName) -> impl Iterator<Item = &Category> {
        self.catalog
            .get(event)
            .into_iter()
            .flat_map(|entry| entry.definitions.iter())
    }

    pub fn create_alerts<E>(
        &self,
        categories: &[EventType],
        mut callback: impl FnMut(&Callback) -> Result<Alert, E>,
        mut translate: impl FnMut(&str) -> String,
    ) -> Result<Vec<Alert>, CreateAlertError<E>> {
        let mut alerts = Vec::new();
        for event in &self.events {
            let definition = self
                .catalog
                .get(*event)
                .ok_or(CreateAlertError::UndefinedEvent(*event))?;
            for category in categories {
                let Some(entry) = definition
                    .definitions
                    .iter()
                    .find(|entry| entry.category == *category)
                else {
                    continue;
                };
                let mut alert = match &entry.definition {
                    Definition::Static { alert } => alert.clone(),
                    Definition::Callback {
                        callback: definition,
                    } => callback(definition).map_err(CreateAlertError::Callback)?,
                };
                alert.alert_text_1 = translate(&alert.alert_text_1);
                alert.alert_text_2 = alert.alert_text_2.as_deref().map(&mut translate);
                if 0.01 * (self.counters[&u16::from(*event)] + 1) as f64 >= alert.creation_delay {
                    alert.alert_type = format!("{}/{}", definition.name, category.as_str());
                    alert.event_type = Some(*category);
                    alerts.push(alert);
                }
            }
        }
        Ok(alerts)
    }
}
