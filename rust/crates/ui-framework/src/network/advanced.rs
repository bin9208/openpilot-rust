use super::Context;
use crate::{
    assets::Texture,
    callback::Callback,
    draw::Draw,
    keyboard::{Keyboard, KeyboardOptions},
    list::{ButtonAction, ListItem, MultipleButtonAction, TextAction, ToggleAction},
    scroller_tici::Scroller,
    widget::{Frame, Property, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use openpilot_wifi::{Command, Event, MeteredType};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
#[derive(Clone, Copy)]
pub(super) enum Action {
    Tether,
    Password,
    Roaming,
    CellMetered,
    Apn,
    WifiMetered(usize),
    Hidden,
}
pub struct AdvancedNetworkSettings {
    pub state: WidgetState,
    pub show_cell_settings: Property<bool>,
    pub keyboard: WidgetHandle,
    pub scroller: Scroller,
    pub(super) context: Context,
    pub(super) params: Rc<openpilot_params::Params>,
    pub(super) tether_enabled: Rc<Cell<bool>>,
    pub(super) password_enabled: Rc<Cell<bool>>,
    pub(super) metered_enabled: Rc<Cell<bool>>,
    pub(super) errors: Rc<RefCell<Vec<Error>>>,
    events: Rc<RefCell<VecDeque<Event>>>,
    actions: Rc<RefCell<VecDeque<Action>>>,
}
impl AdvancedNetworkSettings {
    pub fn item(&mut self, index: usize) -> Result<&mut ListItem, Error> {
        self.scroller
            .item_mut::<ListItem>(index)
            .ok_or(Error::Contract("network settings item missing"))
    }
    fn network_updated(&mut self) -> Result<(), Error> {
        let snapshot = self.context.session.snapshot();
        self.tether_enabled.set(true);
        self.item(0)?
            .action_mut::<ToggleAction>()
            .ok_or(Error::Contract("tether action missing"))?
            .toggle
            .set_value(snapshot.tethering_active);
        self.password_enabled.set(true);
        let enabled = !snapshot.tethering_active && !snapshot.ipv4_address.is_empty();
        self.metered_enabled.set(enabled);
        self.item(6)?
            .action_mut::<MultipleButtonAction>()
            .ok_or(Error::Contract("metered action missing"))?
            .selected = if enabled {
            match snapshot.current_network_metered {
                MeteredType::Unknown => 0,
                MeteredType::Yes => 1,
                MeteredType::No => 2,
            }
        } else {
            0
        };
        Ok(())
    }
    pub(super) fn toggle_value(&mut self, index: usize) -> Result<bool, Error> {
        Ok(self
            .item(index)?
            .action_mut::<ToggleAction>()
            .ok_or(Error::Contract("network toggle missing"))?
            .toggle
            .value())
    }
}
impl Widget for AdvancedNetworkSettings {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.context.session.process()?;
        loop {
            let event = self.events.borrow_mut().pop_front();
            match event {
                Some(Event::NetworksUpdated(_)) => self.network_updated()?,
                Some(
                    Event::NeedAuth(_)
                    | Event::Activated
                    | Event::Forgotten(_)
                    | Event::Disconnected,
                ) => {}
                None => break,
            }
        }
        let visible = self.show_cell_settings.get();
        self.context
            .session
            .send(Command::SetIpv4Forward(visible))?;
        for index in [3, 4, 5] {
            self.item(index)?.state.visible = visible.into();
        }
        if let Some(error) = self.errors.borrow_mut().pop() {
            return Err(error);
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller.set_rect(self.state.rect);
        self.scroller.render(frame, draw)?;
        loop {
            let action = self.actions.borrow_mut().pop_front();
            let Some(action) = action else { break };
            self.perform(action, frame)?;
        }
        Ok(RenderResult::None)
    }
}

mod build;
