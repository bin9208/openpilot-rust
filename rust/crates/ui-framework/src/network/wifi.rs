use super::{Context, Model, Phase};
use crate::{
    assets::Texture,
    button::{Button, ButtonStyle},
    callback::Callback,
    dialog::ConfirmDialog,
    draw::Draw,
    keyboard::{Keyboard, KeyboardOptions},
    text_layout::Horizontal,
    widget::{Frame, NavigationRequest, Widget, WidgetHandle, WidgetState},
    Error,
};
use openpilot_wifi::{Command, Event, Network};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};
pub struct WifiManagerUi {
    pub state: WidgetState,
    pub model: Rc<RefCell<Model>>,
    pub keyboard: WidgetHandle,
    pub ip_address: String,
    pub button_width: f32,
    pub scroll: openpilot_startup_ui::scroll::Scroll,
    pub(super) networks: Vec<Network>,
    pub(super) buttons: HashMap<String, Button>,
    pub(super) forget: HashMap<String, Button>,
    pub(super) icons: [Texture; 7],
    events: Rc<RefCell<VecDeque<Event>>>,
}
impl WifiManagerUi {
    pub fn new(
        context: Context,
        mut texture: impl FnMut(&str, (i32, i32)) -> Result<Texture, Error>,
    ) -> Result<Self, Error> {
        let mut keyboard = Keyboard::new(
            KeyboardOptions {
                max_length: 64,
                min_length: 8,
                password_toggle: true,
                ..Default::default()
            },
            &mut texture,
        )?;
        keyboard.set_cancel_text(&context.text("Cancel"));
        let icons = [
            texture("icons/wifi_strength_low.png", (50, 50))?,
            texture("icons/wifi_strength_medium.png", (50, 50))?,
            texture("icons/wifi_strength_high.png", (50, 50))?,
            texture("icons/wifi_strength_full.png", (50, 50))?,
            texture("icons/checkmark.png", (50, 50))?,
            texture("icons/circled_slash.png", (50, 50))?,
            texture("icons/lock_closed.png", (50, 50))?,
        ];
        let events = context.session.subscribe();
        let ip_address = context.session.snapshot().ipv4_address;
        Ok(Self {
            state: WidgetState::default(),
            model: Rc::new(RefCell::new(Model::new(context))),
            keyboard: WidgetHandle::new(keyboard),
            ip_address,
            button_width: 200.0,
            scroll: Default::default(),
            networks: Vec::new(),
            buttons: HashMap::new(),
            forget: HashMap::new(),
            icons,
            events,
        })
    }
    pub fn networks(&self) -> &[Network] {
        &self.networks
    }
    fn networks_updated(&mut self, networks: Vec<Network>) {
        self.networks = networks;
        for network in &self.networks {
            let mut button = Button::new(openpilot_wifi::normalize_ssid(&network.ssid));
            button.label.size = 55.0;
            button.label.horizontal = Horizontal::Left;
            button.set_style(ButtonStyle::TransparentWhiteText);
            let model = self.model.clone();
            let selected = network.clone();
            button.state.click = Some(Box::new(move || model.borrow_mut().choose(&selected)));
            self.buttons.insert(network.ssid.clone(), button);
            let mut button = Button::new(self.model.borrow().context.text("Forget"));
            button.label.size = 45.0;
            button.set_style(ButtonStyle::ForgetWifi);
            let model = self.model.clone();
            let selected = network.clone();
            button.state.click = Some(Box::new(move || {
                model.borrow_mut().request_forget(&selected)
            }));
            self.forget.insert(network.ssid.clone(), button);
        }
    }
    pub(super) fn process(&mut self) -> Result<(), Error> {
        self.model.borrow().context.session.process()?;
        self.ip_address = self.model.borrow().context.session.snapshot().ipv4_address;
        loop {
            let event = self.events.borrow_mut().pop_front();
            let Some(event) = event else { break };
            if let Event::NetworksUpdated(networks) = event {
                self.networks_updated(networks);
            } else {
                self.model.borrow_mut().event(&event, &self.networks);
            }
        }
        if let Some(error) = self.model.borrow_mut().errors.pop() {
            return Err(error);
        }
        Ok(())
    }
    pub(super) fn prompt(&mut self, frame: &Frame<'_>) -> Result<bool, Error> {
        let model = self.model.borrow();
        let Some(network) = model.network.clone() else {
            return Ok(false);
        };
        match model.phase {
            Phase::NeedsAuth => {
                let title = model.context.text(if model.password_retry {
                    "Wrong password"
                } else {
                    "Enter password"
                });
                let subtitle = model
                    .context
                    .text("for \"{}\"")
                    .replace("{}", &openpilot_wifi::normalize_ssid(&network.ssid));
                drop(model);
                let mut keyboard = self.keyboard.get_mut::<Keyboard>()?;
                keyboard.set_title(&title, &subtitle);
                keyboard.reset(Some(8));
                let weak = self.keyboard.downgrade();
                let state = self.model.clone();
                keyboard.callback = Some(Callback::new(move |result| {
                    let Some(handle) = weak.upgrade() else {
                        return;
                    };
                    let password = match handle.get_mut::<Keyboard>() {
                        Ok(mut keyboard) => {
                            let text = keyboard.text();
                            if result == crate::widget::DialogResult::Confirm {
                                keyboard.clear();
                            }
                            text
                        }
                        Err(error) => {
                            state.borrow_mut().errors.push(error);
                            return;
                        }
                    };
                    state.borrow_mut().password(&network, result, &password);
                }));
                frame
                    .navigation
                    .push(NavigationRequest::Push(self.keyboard.clone()));
                Ok(true)
            }
            Phase::ShowForgetConfirm => {
                let text = model
                    .context
                    .text("Forget Wi-Fi Network \"{}\"?")
                    .replace("{}", &openpilot_wifi::normalize_ssid(&network.ssid));
                let mut dialog = ConfirmDialog::new(
                    &text,
                    &model.context.text("Forget"),
                    &model.context.text("Cancel"),
                )?;
                drop(model);
                let state = self.model.clone();
                dialog.callback = Some(Callback::new(move |result| {
                    state.borrow_mut().forgot_result(&network, result)
                }));
                frame
                    .navigation
                    .push(NavigationRequest::Push(WidgetHandle::new(dialog)));
                Ok(true)
            }
            Phase::Idle | Phase::Connecting | Phase::Forgetting => Ok(false),
        }
    }
}
impl Widget for WifiManagerUi {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.process()
    }
    fn paint(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<crate::widget::RenderResult, Error> {
        self.render_networks(frame, draw)
    }
    fn show(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.show(frame);
        }
        self.model.borrow_mut().send(Command::SetActive(true));
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.hide(frame);
        }
        self.model.borrow_mut().send(Command::SetActive(false));
    }
}
