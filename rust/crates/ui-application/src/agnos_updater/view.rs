use super::worker::State;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Retry,
    Start,
    Network,
    Progress,
    Reboot,
}
pub struct Shared {
    pub state: RefCell<State>,
    pub connected: std::cell::Cell<bool>,
    pub actions: RefCell<VecDeque<Action>>,
}
impl Shared {
    pub fn new(state: State) -> Rc<Self> {
        Rc::new(Self {
            state: RefCell::new(state),
            connected: false.into(),
            actions: RefCell::default(),
        })
    }
    pub fn action(&self, action: Action) {
        self.actions.borrow_mut().push_back(action);
    }
}
