use crate::{
    bus::{self, Address},
    engine::Engine,
    transition::Signal,
    Error,
};
use dbus::{arg::PropMap, message::MatchRule, nonblock::MsgMatch, Message};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tokio::sync::Notify;

#[derive(Default)]
struct Queues {
    removed: VecDeque<String>,
    added: VecDeque<String>,
    changed: VecDeque<bool>,
    states: VecDeque<Signal>,
    failure: Option<Error>,
}
#[derive(Clone, Copy)]
enum Kind {
    Removed,
    Added,
    Changed,
    State,
}
fn push<T>(queue: &mut VecDeque<T>, value: T) {
    if queue.len() == 10 {
        queue.pop_front();
    }
    queue.push_back(value);
}
fn receive(message: &Message, kind: Kind, queues: &mut Queues) -> Result<(), Error> {
    match kind {
        Kind::Removed | Kind::Added => {
            let (path,): (dbus::Path<'static>,) = message.read_all()?;
            match kind {
                Kind::Removed => push(&mut queues.removed, path.to_string()),
                Kind::Added => push(&mut queues.added, path.to_string()),
                Kind::Changed | Kind::State => unreachable!(),
            }
        }
        Kind::Changed => {
            let (interface, changed, _): (String, PropMap, Vec<String>) = message.read_all()?;
            push(
                &mut queues.changed,
                interface == bus::WIRELESS && changed.contains_key("LastScan"),
            );
        }
        Kind::State => {
            let (current, previous, reason) = message.read_all()?;
            push(
                &mut queues.states,
                Signal {
                    current,
                    previous,
                    reason,
                },
            );
        }
    }
    Ok(())
}
impl Engine {
    pub async fn monitor(&self) -> Result<(), Error> {
        let device = self
            .state()?
            .device
            .clone()
            .ok_or(Error::Property("WiFi device"))?;
        let queues = Arc::new(Mutex::new(Queues::default()));
        let wake = Arc::new(Notify::new());
        let specifications = [
            (bus::DEVICE, "StateChanged", device.as_str(), Kind::State),
            (
                bus::SETTINGS,
                "NewConnection",
                bus::SETTINGS_PATH,
                Kind::Added,
            ),
            (
                bus::SETTINGS,
                "ConnectionRemoved",
                bus::SETTINGS_PATH,
                Kind::Removed,
            ),
            (
                bus::PROPERTIES,
                "PropertiesChanged",
                device.as_str(),
                Kind::Changed,
            ),
        ];
        let mut listeners: Vec<MsgMatch> = Vec::new();
        for (interface, member, path, kind) in specifications {
            let mut rule = MatchRule::new_signal(interface, member);
            rule.path = Some(dbus::Path::new(path.to_owned()).map_err(Error::Request)?);
            let incoming = Arc::clone(&queues);
            let notify = Arc::clone(&wake);
            listeners.push(
                self.monitor
                    .connection
                    .add_match(rule)
                    .await?
                    .msg_cb(move |message| {
                        match incoming.lock() {
                            Ok(mut queues) => {
                                if let Err(error) = receive(&message, kind, &mut queues) {
                                    queues.failure = Some(error);
                                }
                            }
                            Err(_) => {
                                notify.notify_one();
                                return false;
                            }
                        }
                        notify.notify_one();
                        true
                    }),
            );
        }
        let result = async {
            loop {
                wake.notified().await;
                if let Some(error) = queues.lock().map_err(|_| Error::Poisoned)?.failure.take() {
                    return Err(error);
                }
                loop {
                    let path = queues
                        .lock()
                        .map_err(|_| Error::Poisoned)?
                        .removed
                        .pop_front();
                    let Some(path) = path else {
                        break;
                    };
                    self.state()?.remove_connection(&path);
                }
                loop {
                    let path = queues
                        .lock()
                        .map_err(|_| Error::Poisoned)?
                        .added
                        .pop_front();
                    let Some(path) = path else {
                        break;
                    };
                    self.refresh_connection(path).await?;
                }
                loop {
                    let changed = queues
                        .lock()
                        .map_err(|_| Error::Poisoned)?
                        .changed
                        .pop_front();
                    let Some(changed) = changed else {
                        break;
                    };
                    if changed {
                        self.update_networks().await?;
                    }
                }
                loop {
                    let signal = queues
                        .lock()
                        .map_err(|_| Error::Poisoned)?
                        .states
                        .pop_front();
                    let Some(signal) = signal else {
                        break;
                    };
                    self.handle_state(signal).await?;
                }
            }
        }
        .await;
        for listener in listeners {
            if let Err(error) = self.monitor.connection.remove_match(listener.token()).await {
                self.warn(format!("Wi-Fi signal cleanup: {error}"));
            }
        }
        result
    }
    async fn handle_state(&self, signal: Signal) -> Result<(), Error> {
        let pending = self.state()?.begin_transition(signal);
        let Some(pending) = pending else {
            return Ok(());
        };
        let activated = pending.activated;
        let active = self.active_connection(&self.monitor).await?;
        let path = active.as_ref().map(|(path, _)| path.as_str());
        if !self.state()?.finish_transition(pending, path) {
            return Ok(());
        }
        if path.is_none() {
            let stage = if activated {
                "ACTIVATED"
            } else {
                "PREPARE/CONFIG"
            };
            self.warn(format!(
                "Failed to get active wifi connection during {stage} state"
            ));
        }
        if activated {
            self.active_info().await?;
            if let Some(path) = path {
                match self
                    .monitor
                    .call::<_, ()>(
                        Address {
                            path,
                            interface: bus::CONNECTION,
                        },
                        "Save",
                        (),
                    )
                    .await
                {
                    Ok(()) => {}
                    Err(Error::Reply(error)) => {
                        self.warn(format!("Failed to persist connection to disk: {error}"))
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    }
}
