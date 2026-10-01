use super::session::Context;
use crate::{widget::DialogResult, Error};
use openpilot_wifi::{Command, Event, Network, SecurityType};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum Phase {
    #[default]
    Idle,
    Connecting,
    NeedsAuth,
    ShowForgetConfirm,
    Forgetting,
}
pub struct Model {
    pub context: Context,
    pub phase: Phase,
    pub network: Option<Network>,
    pub password_retry: bool,
    pub errors: Vec<Error>,
}
impl Model {
    pub fn new(context: Context) -> Self {
        Self {
            context,
            phase: Phase::Idle,
            network: None,
            password_retry: false,
            errors: Vec::new(),
        }
    }
    pub fn send(&mut self, command: Command) {
        if let Err(error) = self.context.session.send(command) {
            self.errors.push(error);
        }
    }
    pub fn choose(&mut self, network: &Network) {
        let snapshot = self.context.session.snapshot();
        if !snapshot.saved_ssids.contains(&network.ssid)
            && network.security_type != SecurityType::Open
        {
            self.phase = Phase::NeedsAuth;
            self.network = Some(network.clone());
            self.password_retry = false;
        } else if snapshot.wifi_state.ssid.as_deref() != Some(network.ssid.as_str()) {
            self.connect(network, "");
        }
    }
    pub fn connect(&mut self, network: &Network, password: &str) {
        self.phase = Phase::Connecting;
        self.network = Some(network.clone());
        if self
            .context
            .session
            .snapshot()
            .saved_ssids
            .contains(&network.ssid)
            && password.is_empty()
        {
            self.send(Command::Activate(network.ssid.clone()));
        } else {
            self.send(Command::Connect {
                ssid: network.ssid.clone(),
                password: password.into(),
                hidden: false,
            });
        }
    }
    pub fn forget(&mut self, network: &Network) {
        self.phase = Phase::Forgetting;
        self.network = Some(network.clone());
        self.send(Command::Forget(network.ssid.clone()));
    }
    pub fn request_forget(&mut self, network: &Network) {
        self.phase = Phase::ShowForgetConfirm;
        self.network = Some(network.clone());
    }
    pub fn password(&mut self, network: &Network, result: DialogResult, password: &str) {
        match result {
            DialogResult::Confirm => {
                if password.chars().count() >= 8 {
                    self.connect(network, password);
                }
            }
            DialogResult::Cancel => self.phase = Phase::Idle,
            DialogResult::NoAction => {}
        }
    }
    pub fn forgot_result(&mut self, network: &Network, result: DialogResult) {
        match result {
            DialogResult::Confirm => self.forget(network),
            DialogResult::Cancel => self.phase = Phase::Idle,
            DialogResult::NoAction => {}
        }
    }
    pub fn event(&mut self, event: &Event, networks: &[Network]) {
        match event {
            Event::NeedAuth(ssid) => {
                if let Some(network) = networks.iter().find(|network| network.ssid == *ssid) {
                    self.phase = Phase::NeedsAuth;
                    self.network = Some(network.clone());
                    self.password_retry = true;
                }
            }
            Event::Activated | Event::Disconnected => {
                if self.phase == Phase::Connecting {
                    self.phase = Phase::Idle;
                }
            }
            Event::Forgotten(_) => {
                if self.phase == Phase::Forgetting {
                    self.phase = Phase::Idle;
                }
            }
            Event::NetworksUpdated(_) => {}
        }
    }
}
