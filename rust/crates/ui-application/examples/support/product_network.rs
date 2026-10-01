use super::{product_input::Step, Scene};
use openpilot_ui_application::{
    context::Context,
    mici::settings::network::{self, wifi::Wifi},
};
use openpilot_ui_framework::{
    application::TickRegistry,
    canvas::Canvas,
    navigation::NavWidget,
    network::{WifiBackend, WifiSession},
    widget::WidgetHandle,
    Error,
};
use openpilot_wifi::{Command, ConnectStatus, Event, Snapshot, WifiState};
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
struct Backend {
    snapshot: Snapshot,
    events: Vec<Event>,
    commands: Vec<Command>,
}
#[derive(Clone)]
struct Fake(Rc<RefCell<Backend>>);
impl WifiBackend for Fake {
    fn snapshot(&self) -> Result<Snapshot, Error> {
        Ok(self.0.borrow().snapshot.clone())
    }
    fn drain_events(&self) -> Result<Vec<Event>, Error> {
        Ok(std::mem::take(&mut self.0.borrow_mut().events))
    }
    fn send(&self, command: Command) -> Result<(), Error> {
        let mut backend = self.0.borrow_mut();
        if let Command::Connect { ssid, .. } | Command::Activate(ssid) = &command {
            backend.snapshot.wifi_state = WifiState {
                ssid: Some(ssid.clone()),
                status: ConnectStatus::Connecting,
            };
            backend.snapshot.connecting_to_ssid = Some(ssid.clone());
            backend.snapshot.connected_ssid = None;
        }
        backend.commands.push(command);
        Ok(())
    }
}
pub struct Fixture {
    backend: Rc<RefCell<Backend>>,
    pub ticks: TickRegistry,
}
impl Fixture {
    pub fn create(
        context: &Context,
        canvas: &mut Canvas,
        scene: &Scene,
    ) -> Result<(WidgetHandle, Self), Box<dyn std::error::Error>> {
        let backend = Rc::new(RefCell::new(Backend {
            snapshot: scene.wifi.clone().ok_or("Wi-Fi fixture missing")?,
            ..Default::default()
        }));
        let session = WifiSession::new(Fake(backend.clone()))?;
        let ticks = TickRegistry::default();
        let widget = if scene.kind == "network-mici" {
            WidgetHandle::new(
                network::Network::new(
                    context.clone(),
                    canvas,
                    network::Connection {
                        session,
                        ticks: ticks.clone(),
                    },
                )?
                .navigation(),
            )
        } else {
            let widget = Wifi::new(context.clone(), session, canvas)?;
            ticks.add(widget.tick.clone());
            WidgetHandle::new(widget.navigation())
        };
        Ok((widget, Self { backend, ticks }))
    }
    pub fn before(&self, step: Option<&Step>) {
        if let Some(step) = step {
            let mut backend = self.backend.borrow_mut();
            if let Some(snapshot) = &step.wifi {
                backend.snapshot = snapshot.clone();
            }
            backend.events.extend(step.wifi_events.clone());
        }
    }
    pub fn snapshot(&self, widget: &WidgetHandle) -> Result<Value, Error> {
        let nav = widget.get::<NavWidget>()?;
        let wifi = (nav.content.as_ref() as &dyn std::any::Any).downcast_ref::<Wifi>();
        Ok(
            json!({"commands":self.backend.borrow().commands,"forgetting":wifi.map(Wifi::any_network_forgetting)}),
        )
    }
}
