use crate::{
    context::{Action, Context},
    mici::widgets::dialog::InputOptions,
    params::Read,
};
use openpilot_ui_framework::{network::WifiSession, Error};
use openpilot_wifi::{Command, Event, MeteredType};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
pub(super) struct MenuModel {
    pub context: Context,
    pub session: WifiSession,
    pub tether_enabled: Cell<bool>,
    pub password_enabled: Cell<bool>,
    pub tether_checked: Cell<bool>,
    meter_ready: Cell<bool>,
    tether_when_updated: Cell<bool>,
    pub meter_value: RefCell<String>,
    events: Rc<RefCell<VecDeque<Event>>>,
}
impl MenuModel {
    pub fn new(context: Context, session: WifiSession) -> Rc<Self> {
        let events = session.subscribe();
        Rc::new(Self {
            context,
            session,
            events,
            tether_enabled: Cell::new(true),
            password_enabled: Cell::new(true),
            tether_checked: Cell::new(false),
            meter_ready: Cell::new(false),
            tether_when_updated: Cell::new(false),
            meter_value: RefCell::new("default".into()),
        })
    }
    pub fn process_callbacks(&self) {
        loop {
            let event = self.events.borrow_mut().pop_front();
            let Some(event) = event else { break };
            if matches!(event, Event::NetworksUpdated(_)) {
                let snapshot = self.session.snapshot();
                self.tether_enabled.set(true);
                self.password_enabled.set(true);
                self.meter_ready.set(true);
                self.tether_when_updated.set(snapshot.tethering_active);
                self.tether_checked.set(snapshot.tethering_active);
                *self.meter_value.borrow_mut() = match snapshot.current_network_metered {
                    MeteredType::Unknown => "default",
                    MeteredType::Yes => "metered",
                    MeteredType::No => "unmetered",
                }
                .into();
            }
        }
    }
    pub fn meter_enabled(&self) -> bool {
        self.meter_ready.get()
            && !self.tether_when_updated.get()
            && !self.session.snapshot().ipv4_address.is_empty()
    }
    pub fn cellular_visible(&self) -> bool {
        matches!(self.context.prime.get(), 0 | 2)
    }
    pub fn report(&self, result: Result<(), Error>) {
        if let Err(error) = result {
            self.context.actions.push(Action::Failure(error.into()));
        }
    }
    pub fn tether(&self, value: bool) -> Result<(), Error> {
        self.tether_checked.set(value);
        self.tether_enabled.set(false);
        self.password_enabled.set(false);
        self.meter_ready.set(false);
        self.session.send(Command::SetTetheringActive(value))
    }
    pub fn meter(&self, value: String) -> Result<(), Error> {
        let metered = match value.as_str() {
            "metered" => MeteredType::Yes,
            "unmetered" => MeteredType::No,
            _ => MeteredType::Unknown,
        };
        *self.meter_value.borrow_mut() = value;
        self.meter_ready.set(false);
        self.session
            .send(Command::SetCurrentNetworkMetered(metered))
    }
    pub fn password(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.context.actions.push(Action::MiciInput(InputOptions {
            hint: "enter password...".into(),
            text: self.session.snapshot().tethering_password,
            minimum_length: 8,
            auto_return: String::new(),
            callback: Some(Rc::new(move |password: String| {
                if let Some(model) = weak.upgrade() {
                    if !password.is_empty() {
                        model.tether_enabled.set(false);
                        model.password_enabled.set(false);
                        model.report(model.session.send(Command::SetTetheringPassword(password)));
                    }
                }
            })),
        }));
    }
    pub fn apn(self: &Rc<Self>) -> Result<(), Error> {
        let current = self.context.params.string("GsmApn")?;
        let weak = Rc::downgrade(self);
        self.context.actions.push(Action::MiciInput(InputOptions {
            hint: "enter APN...".into(),
            text: current,
            minimum_length: 0,
            auto_return: String::new(),
            callback: Some(Rc::new(move |apn: String| {
                if let Some(model) = weak.upgrade() {
                    let apn = apn.trim_matches(|ch: char| {
                        ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch)
                    });
                    let result = if apn.is_empty() {
                        model.context.params.remove("GsmApn")
                    } else {
                        model.context.params.put("GsmApn", apn.as_bytes())
                    };
                    if let Err(error) = result {
                        model.context.actions.push(Action::Failure(error));
                    }
                }
            })),
        }));
        Ok(())
    }
}
