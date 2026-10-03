use crate::{
    engine::Remote, learning::Learning, Action, Address, CommandWriter, Error, Gates, Intent,
    Seconds, Token,
};
use indexmap::IndexMap;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Serialize)]
pub(crate) struct Record {
    id: String,
    time: Seconds,
    address: Address,
    button: Token,
    action: Action,
    emitted: bool,
    reason: Reason,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum Reason {
    Test,
    Inactive,
    Sent,
}

#[derive(Default)]
pub(crate) struct Activity {
    pub last: Option<Record>,
    pub by_address: IndexMap<Address, Record>,
    pub recent: VecDeque<Record>,
    last_fire: HashMap<(Address, Token), Seconds>,
}

pub(crate) struct Emission<'a> {
    pub path: &'a str,
    pub tokens: &'a [Token],
    pub at: Seconds,
    pub gates: Gates,
    pub learning: &'a Learning,
    pub writer: &'a mut CommandWriter,
}

impl Activity {
    pub fn emit(&mut self, remote: &mut Remote, event: Emission<'_>) -> Result<(), Error> {
        let testing = event.learning.matches(&remote.address);
        let held = remote.decoder.active_longs();
        if event.gates.hold_blocked && !testing {
            remote.decoder.cancel_holds();
        }
        for token in event.tokens {
            let action = remote
                .device
                .mapping
                .0
                .get(token)
                .copied()
                .unwrap_or(Action::None);
            let key = (remote.address.clone(), token.clone());
            let last = self.last_fire.get(&key).map_or(0.0, |time| time.0);
            let mut emitted = false;
            let mut reason = if testing {
                Reason::Test
            } else {
                Reason::Inactive
            };
            if !testing
                && remote.device.enabled
                && event.gates.started
                && event.gates.car_ok
                && !(held.contains(token) && event.gates.hold_blocked)
                && event.at.0 - last >= 0.18
                && action != Action::None
            {
                event.writer.send(Intent {
                    address: remote.address.clone(),
                    action,
                    at: event.at,
                    hold: held
                        .contains(token)
                        .then(|| format!("{}:{}", event.path, token.as_str())),
                    repeat: remote.decoder.repeated().contains(token),
                })?;
                self.last_fire.insert(key, event.at);
                emitted = true;
                reason = Reason::Sent;
            }
            let record = Record {
                id: uuid::Uuid::new_v4().simple().to_string(),
                time: event.at,
                address: remote.address.clone(),
                button: token.clone(),
                action,
                emitted,
                reason,
            };
            self.last = Some(record.clone());
            self.by_address
                .insert(remote.address.clone(), record.clone());
            if self.recent.len() == 128 {
                self.recent.pop_front();
            }
            self.recent.push_back(record);
        }
        Ok(())
    }
}
