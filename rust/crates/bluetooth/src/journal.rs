use crate::{
    files::{atomic_json, Error},
    Action, Address, Seconds,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Cruise,
    Lane,
}

impl Channel {
    pub const ALL: [Self; 2] = [Self::Cruise, Self::Lane];

    pub const fn filename(self) -> &'static str {
        match self {
            Self::Cruise => "cruise.json",
            Self::Lane => "lane.json",
        }
    }
}

#[derive(Serialize)]
struct Command {
    id: String,
    time: Seconds,
    action: Action,
    address: Address,
    hold: Option<String>,
    repeat: bool,
}

pub struct Intent {
    pub address: Address,
    pub action: Action,
    pub at: Seconds,
    pub hold: Option<String>,
    pub repeat: bool,
}

#[derive(Default, Serialize)]
struct Journal {
    events: Vec<Command>,
}

pub struct CommandWriter {
    root: PathBuf,
    cruise: Journal,
    lane: Journal,
    session: String,
    sequence: u64,
}

impl CommandWriter {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            cruise: Journal::default(),
            lane: Journal::default(),
            session: uuid::Uuid::new_v4().simple().to_string(),
            sequence: 0,
        }
    }

    fn events(&mut self, channel: Channel) -> &mut Vec<Command> {
        match channel {
            Channel::Cruise => &mut self.cruise.events,
            Channel::Lane => &mut self.lane.events,
        }
    }

    pub fn publish(&self, channel: Channel) -> Result<(), Error> {
        let journal = match channel {
            Channel::Cruise => &self.cruise,
            Channel::Lane => &self.lane,
        };
        atomic_json(&self.root.join(channel.filename()), journal)
    }

    pub fn send(&mut self, intent: Intent) -> Result<(), Error> {
        let channel = match intent.action {
            Action::LaneLeft | Action::LaneRight => Channel::Lane,
            Action::None
            | Action::AccelCruise
            | Action::DecelCruise
            | Action::GapAdjustCruise
            | Action::LfaButton
            | Action::Cancel
            | Action::AccelCruiseLong
            | Action::DecelCruiseLong
            | Action::GapAdjustCruiseLong
            | Action::LfaButtonLong
            | Action::CancelLong
            | Action::PaddleDecel
            | Action::CarrotCruise => Channel::Cruise,
        };
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Sequence)?;
        let id = format!("{}:{}", self.session, self.sequence);
        let events = self.events(channel);
        events.retain(|event| {
            (0.0..=0.4).contains(&(intent.at.0 - event.time.0))
                && intent
                    .hold
                    .as_ref()
                    .is_none_or(|hold| hold.is_empty() || event.hold.as_ref() != Some(hold))
        });
        events.push(Command {
            id,
            time: intent.at,
            action: intent.action,
            address: intent.address,
            hold: intent.hold,
            repeat: intent.repeat,
        });
        let discarded = events.len().saturating_sub(64);
        drop(events.drain(..discarded));
        self.publish(channel)
    }

    pub fn prune(
        &mut self,
        addresses: &HashSet<Address>,
        now: Seconds,
        active_holds: Option<&HashSet<String>>,
    ) -> Result<(), Error> {
        for channel in Channel::ALL {
            let events = self.events(channel);
            let previous = events.len();
            events.retain(|event| {
                addresses.contains(&event.address)
                    && (0.0..=0.4).contains(&(now.0 - event.time.0))
                    && event.hold.as_ref().is_none_or(|hold| {
                        hold.is_empty() || active_holds.is_none_or(|active| active.contains(hold))
                    })
            });
            if previous != events.len() {
                self.publish(channel)?;
            }
        }
        Ok(())
    }
}
