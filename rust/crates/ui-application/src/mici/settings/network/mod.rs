//! Compact network settings and retained scan cards over the native Wi-Fi session.
mod assets;
mod card;
mod forget;
mod icon;
mod loading;
mod menu_control;
mod menu_model;
mod model;
mod scanning;
mod top_button;
pub mod wifi;
use crate::{context::Context, mici::widgets::big_button::Kind, params::binding::Binding};
use menu_control::{Control, ControlKind};
use menu_model::MenuModel;
use openpilot_ui_framework::{
    application::{Tick, TickRegistry},
    callback::Callback,
    canvas::Canvas,
    draw::Draw,
    navigation::NavWidget,
    network::WifiSession,
    scroller::Scroller,
    widget::{Frame, Property, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use openpilot_wifi::Command;
use std::rc::Rc;
pub struct Connection {
    pub session: WifiSession,
    pub ticks: TickRegistry,
}
pub struct Network {
    state: WidgetState,
    pub scroller: Scroller,
    pub wifi: WidgetHandle,
    model: Rc<MenuModel>,
    ticks: TickRegistry,
    wifi_tick: Tick,
    menu_tick: Tick,
}
impl Network {
    pub fn new(
        context: Context,
        canvas: &mut Canvas,
        connection: Connection,
    ) -> Result<Self, Error> {
        let Connection { session, ticks } = connection;
        session.send(Command::SetActive(false))?;
        let assets = Rc::new(assets::Assets::new(canvas)?);
        let wifi = wifi::Wifi::with_assets(context.clone(), session.clone(), assets.clone())?;
        let wifi_tick = wifi.tick.clone();
        let wifi = WidgetHandle::new(wifi.navigation());
        let model = MenuModel::new(context.clone(), session.clone());
        let callback_model = model.clone();
        let menu_tick = Tick::new(move || callback_model.process_callbacks());
        let mut scroller = Scroller::new(true, false, !context.pc, 20.0);
        scroller.indicator = Some(assets.get(
            "icons_mici/settings/horizontal_scroll_indicator.png",
            (96, 48),
        )?);
        scroller.add(Box::new(top_button::TopButton::new(
            session,
            wifi.clone(),
            &assets,
        )?))?;
        let mut metered = assets.button("network usage")?;
        metered.set_multiple(vec!["default".into(), "metered".into(), "unmetered".into()])?;
        let gate = model.clone();
        metered.state.enabled = Property::Dynamic(Box::new(move || gate.meter_enabled()));
        let weak = Rc::downgrade(&model);
        metered.selected = Some(Callback::new(move |value| {
            if let Some(model) = weak.upgrade() {
                model.report(model.meter(value));
            }
        }));
        scroller.add(Box::new(Control {
            button: metered,
            model: model.clone(),
            kind: ControlKind::Meter,
        }))?;
        let mut tether = assets.button("enable tethering")?;
        tether.kind = Kind::Toggle(false);
        let gate = model.clone();
        tether.state.enabled = Property::Dynamic(Box::new(move || gate.tether_enabled.get()));
        let weak = Rc::downgrade(&model);
        tether.changed = Some(Callback::new(move |value| {
            if let Some(model) = weak.upgrade() {
                model.report(model.tether(value));
            }
        }));
        scroller.add(Box::new(Control {
            button: tether,
            model: model.clone(),
            kind: ControlKind::Tether,
        }))?;
        let mut password = assets.button("tethering password")?;
        password.icon = Some(assets.get("icons_mici/settings/network/tethering.png", (64, 54))?);
        let gate = model.clone();
        password.state.enabled = Property::Dynamic(Box::new(move || gate.password_enabled.get()));
        let weak = Rc::downgrade(&model);
        password.state.click = Some(Box::new(move || {
            if let Some(model) = weak.upgrade() {
                model.password();
            }
        }));
        scroller.add(Box::new(password))?;
        for (title, key) in [
            ("enable roaming", Some("GsmRoaming")),
            ("apn settings", None),
            ("cellular metered", Some("GsmMetered")),
        ] {
            let mut button = assets.button(title)?;
            let gate = model.clone();
            button.state.visible = Property::Dynamic(Box::new(move || gate.cellular_visible()));
            if let Some(key) = key {
                button.kind = Kind::Toggle(false);
                button.binding = Some(Binding {
                    params: context.params.clone(),
                    key: key.into(),
                    asynchronous: false,
                });
                button.refresh_param()?;
            } else {
                button.value = "edit".into();
                let weak = Rc::downgrade(&model);
                button.state.click = Some(Box::new(move || {
                    if let Some(model) = weak.upgrade() {
                        model.report(model.apn());
                    }
                }));
            }
            scroller.add(Box::new(button))?;
        }
        Ok(Self {
            state: WidgetState::default(),
            scroller,
            wifi,
            model,
            ticks,
            wifi_tick,
            menu_tick,
        })
    }
    pub fn navigation(self) -> NavWidget {
        NavWidget::new(Box::new(self), 20.0, 240.0)
    }
}
impl Drop for Network {
    fn drop(&mut self) {
        self.ticks.remove(&self.wifi_tick);
        self.ticks.remove(&self.menu_tick);
    }
}
impl Widget for Network {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.scroller.show(frame);
        self.model
            .report(self.model.session.send(Command::SetActive(true)));
        self.ticks.add(self.wifi_tick.clone());
        self.ticks.add(self.menu_tick.clone());
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.scroller.hide(frame);
        self.model
            .report(self.model.session.send(Command::SetActive(false)));
        self.ticks.remove(&self.wifi_tick);
        self.ticks.remove(&self.menu_tick);
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.model
            .session
            .send(Command::SetIpv4Forward(self.model.cellular_visible()))
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller.state.enabled = self.state.enabled.get().into();
        self.scroller.set_rect(self.state.rect);
        self.scroller.render(frame, draw)
    }
}
