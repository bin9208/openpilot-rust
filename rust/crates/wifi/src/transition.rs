use crate::{state::State, ConnectStatus, Event, WifiState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Signal {
    pub current: u32,
    pub previous: u32,
    pub reason: u32,
}
pub struct PendingState {
    pub epoch: u64,
    pub state: WifiState,
    pub activated: bool,
}
impl State {
    pub fn begin_transition(&mut self, signal: Signal) -> Option<PendingState> {
        match signal.current {
            30 => {
                let saved = self
                    .snapshot
                    .wifi_state
                    .ssid
                    .as_deref()
                    .is_some_and(|ssid| !ssid.is_empty() && self.connection(ssid).is_some());
                if signal.reason != 60 && !(signal.reason == 38 && saved) {
                    self.set_connecting(None);
                }
            }
            40 | 50 => {
                if self.snapshot.wifi_state.ssid.is_some() {
                    self.snapshot.wifi_state.status = ConnectStatus::Connecting;
                } else {
                    return Some(PendingState {
                        epoch: self.epoch,
                        state: WifiState {
                            ssid: self.snapshot.wifi_state.ssid.clone(),
                            status: ConnectStatus::Connecting,
                        },
                        activated: false,
                    });
                }
            }
            60 | 120
                if (signal.current == 60 && signal.reason == 8 && signal.previous == 50)
                    || (signal.current == 120 && signal.reason == 7) =>
            {
                if let Some(ssid) = self
                    .snapshot
                    .wifi_state
                    .ssid
                    .clone()
                    .filter(|ssid| !ssid.is_empty())
                {
                    self.events.push(Event::NeedAuth(ssid));
                    self.set_connecting(None);
                }
            }
            100 => {
                return Some(PendingState {
                    epoch: self.epoch,
                    state: WifiState {
                        ssid: self.snapshot.wifi_state.ssid.clone(),
                        status: ConnectStatus::Connected,
                    },
                    activated: true,
                })
            }
            110 if signal.reason == 38
                && self.snapshot.wifi_state.status == ConnectStatus::Connected =>
            {
                self.set_connecting(None)
            }
            _ => {}
        }
        None
    }
    pub fn finish_transition(
        &mut self,
        mut pending: PendingState,
        connection: Option<&str>,
    ) -> bool {
        if self.epoch != pending.epoch {
            return false;
        }
        if let Some(path) = connection {
            pending.state.ssid = self.ssid(path);
        }
        self.snapshot.wifi_state = pending.state;
        if pending.activated {
            self.events.push(Event::Activated);
        }
        true
    }
    pub fn finish_initial_state(
        &mut self,
        epoch: u64,
        device_state: u32,
        connection: Option<&str>,
    ) {
        if self.epoch != epoch {
            return;
        }
        let status = match device_state {
            40..=90 if device_state != 60 => ConnectStatus::Connecting,
            100 => ConnectStatus::Connected,
            _ => ConnectStatus::Disconnected,
        };
        self.snapshot.wifi_state = WifiState {
            ssid: connection.and_then(|path| self.ssid(path)),
            status,
        };
    }
}
