//! Large-display eGPU status and connection check, from layouts/settings/usbgpu.py.
use crate::{
    context::{Action, Context},
    paint,
    services::egpu::{Backend, Check},
};
use openpilot_ui_framework::{
    callback::Callback,
    draw::Draw,
    list::{ButtonAction, ListItem, TextAction},
    scroller_tici::Scroller,
    widget::{Frame, Property, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::Cell, rc::Rc, sync::Arc};

pub struct Egpu {
    state: WidgetState,
    context: Context,
    backend: Arc<dyn Backend>,
    check: Check,
    requested: Rc<Cell<bool>>,
    pub scroller: Scroller,
}
impl Egpu {
    pub fn new(context: Context, backend: Arc<dyn Backend>) -> Result<Self, Error> {
        let mut scroller = Scroller {
            spacing: 0.0,
            line_separator: true,
            ..Default::default()
        };
        for title in ["eGPU Status", "USB Link", "Connection Check"] {
            let mut item = ListItem::new("")?;
            item.title = context.text(title);
            item.action = Some(Box::new(TextAction::new(
                if title == "Connection Check" {
                    "not checked"
                } else {
                    ""
                },
                paint::color(170, 170, 170, 255),
            )));
            scroller.add(Box::new(item));
        }
        let mut item = ListItem::new("")?;
        item.title = context.text("Check eGPU");
        item.description =
            context.text("Checks USB 5 Gbps, firmware, 12V/PCIe, and GPU execution.");
        let mut button = ButtonAction::new("");
        button.text = context.text("CHECK");
        let ui = context.ui.clone();
        button.state.enabled = Property::Dynamic(Box::new(move || !ui.borrow().started));
        item.action = Some(Box::new(button));
        let requested = Rc::new(Cell::new(false));
        let request = requested.clone();
        item.callback = Some(Callback::new(move |()| request.set(true)));
        scroller.add(Box::new(item));
        Ok(Self {
            state: WidgetState::default(),
            context,
            check: Check::new(backend.clone()),
            backend,
            requested,
            scroller,
        })
    }
    fn text(&mut self, index: usize, value: String) -> Result<(), Error> {
        self.scroller
            .item_mut::<ListItem>(index)
            .and_then(|item| item.action_mut::<TextAction>())
            .ok_or(Error::Contract("eGPU status row"))?
            .text = value.into();
        Ok(())
    }
}
impl Widget for Egpu {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.scroller.show(frame);
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let status = self.backend.status(&self.context.ui.borrow().slow)?;
        self.text(0, status)?;
        self.text(1, self.backend.link()?)?;
        match self.check.poll() {
            Ok(true) => self.text(
                2,
                self.check
                    .result
                    .clone()
                    .unwrap_or_else(|| "no errors".into()),
            )?,
            Ok(false) => {}
            Err(error) => self.context.actions.push(Action::Failure(error)),
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller.set_rect(self.state.rect);
        let result = self.scroller.render(frame, draw)?;
        if self.requested.replace(false)
            && !self.context.ui.borrow().started
            && !self.check.running()
        {
            self.check.start()?;
            self.text(2, "checking...".into())?;
        }
        Ok(result)
    }
}
