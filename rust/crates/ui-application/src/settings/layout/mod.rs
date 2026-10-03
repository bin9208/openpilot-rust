mod paint;
use super::{
    developer::Developer, device::Device, egpu::Egpu, resources::Resources, software::Software,
    toggles::Toggles,
};
use crate::{
    context::{Action, Context, Panel},
    paint::texture,
    widgets::firehose::Firehose,
};
use openpilot_ui_framework::{
    assets::Texture,
    callback::Callback,
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    network::{Context as NetworkContext, NetworkUi},
    widget::{Frame, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use openpilot_wifi::Command;
use std::rc::Rc;

pub struct PanelInfo {
    pub kind: Panel,
    pub name: &'static str,
    pub widget: WidgetHandle,
    rect: Rect,
}
pub struct Settings {
    state: WidgetState,
    context: Context,
    pub panels: [PanelInfo; 7],
    current: usize,
    close_icon: Texture,
    close_rect: Rect,
    pub on_close: Option<Callback<()>>,
}
impl Settings {
    pub fn new(context: Context, canvas: &mut Canvas, resources: Resources) -> Result<Self, Error> {
        resources.network.session.send(Command::SetActive(false))?;
        let translations = context.translations.clone();
        let mut network = NetworkUi::new(
            NetworkContext {
                session: resources.network.session,
                translate: Rc::new(move |text| translations.tr(text)),
            },
            resources.network.params,
            |path, size| texture(canvas, path, size),
        )?;
        let prime = context.prime.clone();
        network.advanced.show_cell_settings =
            openpilot_ui_framework::widget::Property::Dynamic(Box::new(move || {
                matches!(prime.get(), 0 | 2)
            }));
        let definitions = [
            (Panel::Device, "Device", Device::create(context.clone())?),
            (Panel::Network, "Network", WidgetHandle::new(network)),
            (
                Panel::Toggles,
                "Toggles",
                Toggles::create(context.clone(), canvas)?,
            ),
            (
                Panel::Software,
                "Software",
                Software::create(context.clone())?,
            ),
            (
                Panel::Firehose,
                "Firehose",
                WidgetHandle::new(Firehose::new(context.clone())?),
            ),
            (
                Panel::Developer,
                "Developer",
                WidgetHandle::new(Developer::new(context.clone(), canvas)?),
            ),
            (
                Panel::Egpu,
                "eGPU",
                WidgetHandle::new(Egpu::new(context.clone(), resources.egpu)?),
            ),
        ];
        let panels = definitions.map(|(kind, name, widget)| PanelInfo {
            kind,
            name,
            widget,
            rect: Rect::default(),
        });
        Ok(Self {
            state: WidgetState::default(),
            context,
            panels,
            current: 0,
            close_icon: texture(canvas, "icons/close2.png", (70, 70))?,
            close_rect: Rect::default(),
            on_close: None,
        })
    }
    pub fn current(&self) -> Panel {
        self.panels[self.current].kind
    }
    pub fn set_current(&mut self, panel: Panel, frame: &Frame<'_>) -> Result<(), Error> {
        let index = match panel {
            Panel::Device => 0,
            Panel::Network => 1,
            Panel::Toggles => 2,
            Panel::Software => 3,
            Panel::Firehose => 4,
            Panel::Developer => 5,
            Panel::Egpu => 6,
        };
        if index != self.current {
            self.panels[self.current].widget.borrow_mut()?.hide(frame);
            self.current = index;
            self.panels[self.current].widget.borrow_mut()?.show(frame);
        }
        Ok(())
    }
    fn report(&self, result: Result<(), Error>) {
        if let Err(error) = result {
            self.context.actions.push(Action::Failure(error.into()));
        }
    }
}
impl Widget for Settings {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.report(
            self.panels[self.current]
                .widget
                .borrow_mut()
                .map(|mut widget| widget.show(frame)),
        );
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.report(
            self.panels[self.current]
                .widget
                .borrow_mut()
                .map(|mut widget| widget.hide(frame)),
        );
    }
    fn mouse_release(
        &mut self,
        position: Point,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        if self.close_rect.contains(position) {
            if let Some(callback) = &self.on_close {
                callback.call(());
            }
            return Ok(());
        }
        if let Some(panel) = self
            .panels
            .iter()
            .find(|panel| panel.rect.contains(position))
        {
            self.set_current(panel.kind, frame)?;
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.draw(frame, draw)
    }
}
