use super::{product_widgets::Product, Scene};
use openpilot_ui_application::{
    context::{Action, Context, Event, Page, Panel},
    root_layout::Main,
    settings::resources::{Network, Resources},
};
use openpilot_ui_framework::{
    canvas::Canvas,
    label::Label,
    stack::NavigationStack,
    widget::{Frame, WidgetHandle},
    Error,
};
use serde::Deserialize;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[derive(Deserialize)]
pub struct Options {
    #[serde(default)]
    steps: Vec<Step>,
    #[serde(skip)]
    control: RefCell<Option<Control>>,
}
#[derive(Deserialize)]
struct Step {
    frame: u32,
    page: Option<String>,
    #[serde(default)]
    timeout: bool,
}
struct Control {
    stack: NavigationStack,
    onboarding: WidgetHandle,
    diagnostics: openpilot_startup_ui::diagnostics::Diagnostics,
}
impl Options {
    pub fn create(
        &self,
        context: &Context,
        canvas: &mut Canvas,
        scene: &Scene,
    ) -> Result<Product, Box<dyn std::error::Error>> {
        let (session, network) = super::product_network::Fixture::new(scene)?;
        let egpu = super::product_egpu::Fixture::new(scene)?;
        let resources = Resources {
            network: Network {
                session,
                params: Rc::new(context.params.raw.as_ref().clone()),
            },
            ticks: network.ticks.clone(),
            egpu,
        };
        let widget = WidgetHandle::new(Main::new(
            context.clone(),
            canvas,
            resources,
            Rc::new(Cell::new(false)),
            "rustvision",
        )?);
        let mut stack = NavigationStack::default();
        stack.render_depth = if context.big { 1 } else { 2 };
        *self.control.borrow_mut() = Some(Control {
            stack,
            onboarding: WidgetHandle::new(Label::new("completed onboarding")),
            diagnostics: openpilot_startup_ui::diagnostics::Diagnostics::new(
                &mut canvas.renderer,
                Default::default(),
                20,
            )?,
        });
        Ok(Product {
            widget,
            dialogs: Rc::default(),
            network: None,
            egpu: None,
        })
    }
    pub fn render(
        &self,
        context: &Context,
        widget: &WidgetHandle,
        frame: &Frame<'_>,
        canvas: &mut Canvas,
    ) -> Result<(), Error> {
        let mut control = self.control.borrow_mut();
        let control = control
            .as_mut()
            .ok_or(Error::Contract("root fixture not initialized"))?;
        if control.stack.is_empty() {
            control.stack.push(widget.clone(), frame)?;
        }
        if let Some(step) = self
            .steps
            .iter()
            .find(|step| u64::from(step.frame) == frame.index)
        {
            if let Some(page) = &step.page {
                let panel = match page.as_str() {
                    "home" => None,
                    "device" => Some(Panel::Device),
                    "toggles" => Some(Panel::Toggles),
                    "network" => Some(Panel::Network),
                    _ => return Err(Error::Contract("root fixture page")),
                };
                widget
                    .get_mut::<Main>()?
                    .open(panel.map_or(Page::Home, Page::Settings), frame)?;
            }
            if step.timeout {
                context.event(Event::InteractiveTimeout);
            }
        }
        widget
            .get_mut::<Main>()?
            .tick(widget, &control.onboarding, &control.stack, frame)?;
        control.stack.process(frame)?;
        let rect = widget.borrow()?.state().rect;
        control.stack.render(frame, rect, canvas)?;
        widget
            .get_mut::<Main>()?
            .finish_render(&mut control.diagnostics, &mut canvas.renderer)?;
        while let Some(action) = context.actions.pop() {
            match action {
                Action::Open(page) => {
                    if !widget.get_mut::<Main>()?.open(page, frame)? {
                        return Err(Error::Contract("unexpected root fixture page"));
                    }
                }
                Action::RefreshParams => context.refresh_params()?,
                Action::ShowTouches(value) => control.diagnostics.options.show_touches = value,
                Action::ShowFps(value) => control.diagnostics.options.show_fps = value,
                Action::Failure(error) => return Err(error.into()),
                _ => return Err(Error::Contract("unexpected root fixture action")),
            }
        }
        widget
            .get_mut::<Main>()?
            .events(widget, &control.onboarding, &control.stack, frame)?;
        control.stack.process(frame)?;
        Ok(())
    }
    pub fn snapshot(&self, widget: &WidgetHandle) -> Result<serde_json::Value, Error> {
        let control = self.control.borrow();
        let control = control
            .as_ref()
            .ok_or(Error::Contract("root fixture not initialized"))?;
        let mut value = serde_json::to_value(widget.get::<Main>()?.snapshot())
            .map_err(|_| Error::Contract("root fixture serialization"))?;
        value["stack"] = serde_json::json!(control.stack.len());
        Ok(value)
    }
}
