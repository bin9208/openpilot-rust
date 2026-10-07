use crate::{
    context::{Action, Context},
    mici::widgets::dialog::InputOptions,
};
use openpilot_ui_framework::{network::WifiSession, Error};
use openpilot_wifi::{Command, Event, Network, SecurityType};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
pub(super) struct CardState {
    pub network: Network,
    pub missing: bool,
    pub forgetting: bool,
    pub wrong_password: bool,
    pub shake_start: Option<f64>,
}
pub(super) struct Movement {
    pub ssid: String,
    pub scroll: bool,
}
pub(super) struct Model {
    pub context: Context,
    pub session: WifiSession,
    pub networks: RefCell<Vec<Network>>,
    pub cards: RefCell<Vec<Rc<RefCell<CardState>>>>,
    pub order: RefCell<Vec<String>>,
    pub moves: RefCell<VecDeque<Movement>>,
    pub pending_move: Cell<bool>,
    pub re_sort: Cell<bool>,
    pub revision: Cell<u64>,
    events: Rc<RefCell<VecDeque<Event>>>,
}
impl Model {
    pub fn new(context: Context, session: WifiSession) -> Rc<Self> {
        let events = session.subscribe();
        Rc::new(Self {
            context,
            session,
            events,
            networks: RefCell::new(Vec::new()),
            cards: RefCell::new(Vec::new()),
            order: RefCell::new(Vec::new()),
            moves: RefCell::new(VecDeque::new()),
            pending_move: Cell::new(false),
            re_sort: Cell::new(false),
            revision: Cell::new(0),
        })
    }
    pub fn process_callbacks(&self) {
        loop {
            let event = self.events.borrow_mut().pop_front();
            let Some(event) = event else { break };
            self.event(event);
        }
    }
    pub fn show(&self) {
        self.networks_updated(self.session.snapshot().networks);
        let networks = self.networks.borrow();
        let mut cards = self.cards.borrow_mut();
        *cards = networks
            .iter()
            .filter_map(|n| {
                cards
                    .iter()
                    .find(|c| c.borrow().network.ssid == n.ssid)
                    .cloned()
            })
            .collect();
        self.re_sort.set(true);
    }
    fn networks_updated(&self, networks: Vec<Network>) {
        self.revision.set(self.revision.get().wrapping_add(1));
        let mut unique: Vec<Network> = Vec::new();
        for network in networks {
            if let Some(existing) = unique.iter_mut().find(|item| item.ssid == network.ssid) {
                *existing = network;
            } else {
                unique.push(network);
            }
        }
        let snapshot = self.session.snapshot();
        let mut cards = self.cards.borrow_mut();
        for network in &unique {
            if let Some(card) = cards
                .iter()
                .find(|card| card.borrow().network.ssid == network.ssid)
            {
                let mut card = card.borrow_mut();
                card.network = network.clone();
                card.missing = false;
                if snapshot.connected_ssid.as_deref() == Some(network.ssid.as_str())
                    || snapshot.connecting_to_ssid.as_deref() == Some(network.ssid.as_str())
                {
                    card.wrong_password = false;
                }
            } else {
                cards.push(Rc::new(RefCell::new(CardState {
                    network: network.clone(),
                    missing: false,
                    forgetting: false,
                    wrong_password: false,
                    shake_start: None,
                })));
            }
        }
        for card in cards.iter() {
            let mut card = card.borrow_mut();
            if !unique.iter().any(|n| n.ssid == card.network.ssid) {
                card.missing = true;
            }
        }
        *self.networks.borrow_mut() = unique;
    }
    pub fn event(&self, event: Event) {
        match event {
            Event::NetworksUpdated(networks) => self.networks_updated(networks),
            Event::NeedAuth(ssid) => {
                if let Some(card) = self.card(&ssid) {
                    let mut card = card.borrow_mut();
                    card.wrong_password = true;
                    card.shake_start = Some((self.context.now_monotonic)());
                }
            }
            Event::Forgotten(ssid) => {
                if let Some(card) = self.card(&ssid) {
                    card.borrow_mut().forgetting = false;
                }
            }
            Event::Activated | Event::Disconnected => {}
        }
    }
    pub fn card(&self, ssid: &str) -> Option<Rc<RefCell<CardState>>> {
        self.cards
            .borrow()
            .iter()
            .find(|card| card.borrow().network.ssid == ssid)
            .cloned()
    }
    pub fn report(&self, result: Result<(), Error>) {
        if let Err(error) = result {
            self.context.actions.push(Action::Failure(error.into()));
        }
    }
    pub fn request_move(&self, ssid: &str, scroll: bool) {
        if self
            .order
            .borrow()
            .iter()
            .position(|value| value == ssid)
            .is_some_and(|index| index > 0)
        {
            self.pending_move.set(true);
            self.moves.borrow_mut().push_back(Movement {
                ssid: ssid.into(),
                scroll,
            });
        }
    }
    pub fn connect(self: &Rc<Self>, ssid: &str) -> Result<(), Error> {
        let network = self
            .networks
            .borrow()
            .iter()
            .find(|n| n.ssid == ssid)
            .cloned();
        let Some(network) = network else {
            eprintln!("Trying to connect to unknown network: {ssid}");
            return Ok(());
        };
        if self.session.snapshot().saved_ssids.contains(&network.ssid) {
            self.session.send(Command::Activate(network.ssid.clone()))?;
        } else if network.security_type == SecurityType::Open {
            self.session.send(Command::Connect {
                ssid: network.ssid.clone(),
                password: String::new(),
                hidden: false,
            })?;
        } else {
            let weak = Rc::downgrade(self);
            let ssid = network.ssid.clone();
            self.context.actions.push(Action::MiciInput(InputOptions {
                hint: "enter password...".into(),
                text: String::new(),
                minimum_length: 8,
                auto_return: String::new(),
                callback: Some(Rc::new(move |password| {
                    if let Some(model) = weak.upgrade() {
                        model.report(model.connect_with_password(&ssid, password));
                    }
                })),
            }));
            return Ok(());
        }
        self.request_move(&network.ssid, true);
        Ok(())
    }
    fn connect_with_password(&self, ssid: &str, password: String) -> Result<(), Error> {
        self.session.send(Command::Connect {
            ssid: ssid.into(),
            password,
            hidden: false,
        })?;
        self.request_move(ssid, true);
        Ok(())
    }
    pub fn forget(&self, ssid: &str) -> Result<(), Error> {
        if let Some(card) = self.card(ssid) {
            let mut card = card.borrow_mut();
            if card.forgetting {
                return Ok(());
            }
            card.forgetting = true;
        }
        self.session.send(Command::Forget(ssid.into()))
    }
}
