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
    Regulatory,
}
pub struct Confirmation {
    pub text: String,
    pub confirm: String,
    pub cancel: String,
    pub rich: bool,
    pub callback: Callback<DialogResult>,
}
#[derive(Clone, Copy, Debug)]
pub enum UpdaterAction {
    Check,
    Download,
    Reboot,
}
pub struct Selection {
    pub title: String,
    pub options: Vec<String>,
    pub selected: String,
    pub callback: Callback<Option<String>>,
}
pub enum Action {
    Select(Selection),
    Open(Page),
    PairingCheck,
    Failure(crate::Error),
    Confirm(Confirmation),
    Alert(String),
    MiciAlert { title: String, description: String },
    MiciConfirm(crate::mici::widgets::dialog::Confirmation),
    MiciInput(crate::mici::widgets::dialog::InputOptions),
    Updater(UpdaterAction),
    Recording(bool),
    ToggleRecording,
    Bookmark,
    SetOffroadBrightness(Option<i32>),
    SetInteractiveTimeout(Option<i32>),
    RefreshParams,
    ShowTouches(bool),
    ShowFps(bool),
    SetLanguage(String),
    SelectLanguage(Callback<()>),
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
