use openpilot_ui_framework::{callback::Callback, widget::DialogResult};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Device,
    Network,
    Toggles,
    Software,
    Firehose,
    Developer,
    Egpu,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Settings(Panel),
    CarrotWeb,
    Pairing,
    DriverCamera,
    Terms,
    Training,
    Language,
}
pub struct Confirmation {
    pub text: String,
    pub confirm: String,
    pub cancel: String,
    pub rich: bool,
    pub callback: Callback<DialogResult>,
}
pub enum Action {
    Open(Page),
    PairingCheck,
    Failure(crate::Error),
    Confirm(Confirmation),
    Alert(String),
    Recording(bool),
    ToggleRecording,
    Bookmark,
    SetOffroadBrightness(Option<i32>),
    SetInteractiveTimeout(Option<i32>),
    RefreshParams,
    SetLanguage(String),
    Exit,
}
#[derive(Clone, Default)]
pub struct Actions(Rc<RefCell<VecDeque<Action>>>);
impl Actions {
    pub fn push(&self, action: Action) {
        self.0.borrow_mut().push_back(action);
    }
    pub fn pop(&self) -> Option<Action> {
        self.0.borrow_mut().pop_front()
    }
}
