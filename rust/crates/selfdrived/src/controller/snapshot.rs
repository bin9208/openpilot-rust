use super::Controller;
use crate::{
    alerts::{Alert, AlertEntry},
    cutin::Candidate,
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct Snapshot<'a> {
    pub policy: &'a Controller,
    pub events: Vec<u16>,
    pub static_events: Vec<u16>,
    pub counters: &'a BTreeMap<u16, u64>,
    pub alerts: &'a [AlertEntry],
    pub current_alert: &'a Alert,
    pub previous: &'a [u8],
    pub events_previous: Vec<u16>,
    pub startup_event: Option<u16>,
    pub cutins: &'a [Candidate],
}
impl Controller {
    pub fn snapshot(&self) -> Snapshot<'_> {
        Snapshot {
            policy: self,
            events: self.events.names().iter().copied().map(u16::from).collect(),
            static_events: self
                .events
                .static_names()
                .iter()
                .copied()
                .map(u16::from)
                .collect(),
            counters: self.events.counters(),
            alerts: self.alerts.entries(),
            current_alert: self.alerts.current(),
            previous: &self.previous.bytes,
            events_previous: self
                .events_previous
                .iter()
                .copied()
                .map(u16::from)
                .collect(),
            startup_event: self.startup_event.map(u16::from),
            cutins: self.cutin_tracker.previous(),
        }
    }
}
