use crate::Error;
use openpilot_wifi::{Command, Event, Snapshot, WifiManager};
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::{Rc, Weak},
};
pub trait WifiBackend {
    fn snapshot(&self) -> Result<Snapshot, Error>;
    fn drain_events(&self) -> Result<Vec<Event>, Error>;
    fn send(&self, command: Command) -> Result<(), Error>;
}
impl WifiBackend for WifiManager {
    fn snapshot(&self) -> Result<Snapshot, Error> {
        WifiManager::snapshot(self).map_err(|error| Error::Io(std::io::Error::other(error)))
    }
    fn drain_events(&self) -> Result<Vec<Event>, Error> {
        WifiManager::drain_events(self).map_err(|error| Error::Io(std::io::Error::other(error)))
    }
    fn send(&self, command: Command) -> Result<(), Error> {
        WifiManager::send(self, command).map_err(|error| Error::Io(std::io::Error::other(error)))
    }
}
type Events = Rc<RefCell<VecDeque<Event>>>;
struct Session {
    backend: Box<dyn WifiBackend>,
    snapshot: Snapshot,
    listeners: Vec<Weak<RefCell<VecDeque<Event>>>>,
}
#[derive(Clone)]
pub struct WifiSession(Rc<RefCell<Session>>);
impl WifiSession {
    pub fn new(backend: impl WifiBackend + 'static) -> Result<Self, Error> {
        let snapshot = backend.snapshot()?;
        Ok(Self(Rc::new(RefCell::new(Session {
            backend: Box::new(backend),
            snapshot,
            listeners: Vec::new(),
        }))))
    }
    pub fn subscribe(&self) -> Events {
        let events = Rc::new(RefCell::new(VecDeque::new()));
        self.0.borrow_mut().listeners.push(Rc::downgrade(&events));
        events
    }
    pub fn snapshot(&self) -> Snapshot {
        self.0.borrow().snapshot.clone()
    }
    pub fn send(&self, command: Command) -> Result<(), Error> {
        self.0.borrow().backend.send(command)
    }
    pub fn process(&self) -> Result<(), Error> {
        let mut session = self.0.borrow_mut();
        session.snapshot = session.backend.snapshot()?;
        for event in session.backend.drain_events()? {
            session.listeners.retain(|listener| {
                if let Some(queue) = listener.upgrade() {
                    queue.borrow_mut().push_back(event.clone());
                    true
                } else {
                    false
                }
            });
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct Context {
    pub session: WifiSession,
    pub translate: Rc<dyn Fn(&str) -> String>,
}
impl Context {
    pub fn text(&self, text: &str) -> String {
        (self.translate)(text)
    }
}
