//! Shared product data and typed UI effects, independent of any concrete layout.
pub mod actions;
use crate::{
    device::Device,
    params::store::Store,
    state::{Transition, UiState},
};
pub use actions::{Action, Actions, Confirmation, Page, Panel};
use openpilot_messaging::runtime::SubMaster;
use openpilot_ui_framework::{callback::Callback, multilang::Multilang, widget::Property};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc,
    },
};
pub struct PrimeStatus(AtomicI32);
impl Default for PrimeStatus {
    fn default() -> Self {
        Self::new(-2)
    }
}
impl PrimeStatus {
    pub fn new(value: i32) -> Self {
        Self(AtomicI32::new(value))
    }
    pub fn get(&self) -> i32 {
        self.0.load(Ordering::Relaxed)
    }
    pub fn set(&self, value: i32) {
        self.0.store(value, Ordering::Relaxed);
    }
    pub fn is_paired(&self) -> bool {
        self.get() > -1
    }
    pub fn is_prime(&self) -> bool {
        self.get() > 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Offroad,
    Engaged,
    InteractiveTimeout,
}
pub type EventCallbacks = Rc<RefCell<Vec<(Event, Callback<()>)>>>;
#[derive(Clone)]
pub struct Context {
    pub ui: Rc<RefCell<UiState>>,
    pub messages: Rc<RefCell<SubMaster>>,
    pub device: Rc<RefCell<Device>>,
    pub params: Rc<Store>,
    pub memory: Rc<Store>,
    pub translations: Rc<RefCell<Multilang>>,
    pub prime: Arc<PrimeStatus>,
    pub actions: Actions,
    pub big: bool,
    pub pc: bool,
    pub device_type: String,
    pub now_wall: Rc<dyn Fn() -> chrono::DateTime<chrono::Local>>,
    pub source_root: std::path::PathBuf,
    pub persist_root: std::path::PathBuf,
    pub callbacks: EventCallbacks,
}
impl Context {
    pub fn tr(&self, text: &str) -> String {
        self.translations.borrow().tr(text).into()
    }
    pub fn trn(&self, singular: &str, plural: &str, n: i64) -> String {
        self.translations.borrow().trn(singular, plural, n).into()
    }
    pub fn text(&self, text: &str) -> Property<String> {
        let translations = self.translations.clone();
        let text = text.to_owned();
        Property::Dynamic(Box::new(move || translations.borrow().tr(&text).into()))
    }
    pub fn event(&self, event: Event) {
        let callbacks: Vec<_> = self
            .callbacks
            .borrow()
            .iter()
            .filter(|(kind, _)| *kind == event)
            .map(|(_, callback)| callback.clone())
            .collect();
        for callback in callbacks {
            callback.call(());
        }
    }
    pub fn transition(&self, transition: Transition) {
        self.event(match transition {
            Transition::Offroad => Event::Offroad,
            Transition::Engaged => Event::Engaged,
        });
    }
    pub fn listen(&self, event: Event, callback: Callback<()>) {
        self.callbacks.borrow_mut().push((event, callback));
    }
    pub fn open(&self, page: Page) {
        self.actions.push(Action::Open(page));
    }
}
