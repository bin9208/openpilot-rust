//! Large-display device settings from layouts/settings/device.py.
mod actions;
mod build;
mod calibration;
use crate::context::{Action, Context, Event, Page};
use openpilot_ui_framework::{
    callback::Callback,
    draw::Draw,
    list::{DualButtonAction, ListItem},
    scroller_tici::Scroller,
    widget::{Frame, RenderResult, WeakWidgetHandle, Widget, WidgetHandle, WidgetState},
    Error,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
#[derive(Clone, Copy)]
enum Change {
    Reset,
    Reboot,
    Shutdown,
    Language,
}
pub struct Device {
    state: WidgetState,
    context: Context,
    pub scroller: Scroller,
    changes: Rc<RefCell<VecDeque<Change>>>,
    handle: Option<WeakWidgetHandle>,
    calibration: Rc<RefCell<Option<String>>>,
}
impl Device {
    pub fn create(context: Context) -> Result<WidgetHandle, Error> {
        let widget = WidgetHandle::new(Self::new(context.clone())?);
        widget.get_mut::<Self>()?.handle = Some(widget.downgrade());
        let weak = widget.downgrade();
        let actions = context.actions.clone();
        context.listen(
            Event::Offroad,
            Callback::new(move |()| {
                if let Some(widget) = weak.upgrade() {
                    let result = (|| -> Result<(), crate::Error> {
                        widget.get_mut::<Self>()?.offroad()?;
                        Ok(())
                    })();
                    if let Err(error) = result {
                        actions.push(Action::Failure(error));
                    }
                }
            }),
        );
        Ok(widget)
    }
    fn offroad(&mut self) -> Result<(), Error> {
        let offroad = !self.context.ui.borrow().started;
        let item = self
            .scroller
            .item_mut::<ListItem>(8)
            .ok_or(Error::Contract("device power item"))?;
        item.action_mut::<DualButtonAction>()
            .ok_or(Error::Contract("device power action"))?
            .right
            .state
            .visible = offroad.into();
        calibration::write_position(&self.context)?;
        Ok(())
    }
    fn description(&mut self) -> Result<(), Error> {
        *self.calibration.borrow_mut() = Some(calibration::description(&self.context)?);
        Ok(())
    }
}
impl Widget for Device {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.scroller.show(frame);
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.scroller.hide(frame);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller.set_rect(self.state.rect);
        let result = self.scroller.render(frame, draw)?;
        loop {
            let next = self.changes.borrow_mut().pop_front();
            let Some(change) = next else { break };
            self.change(change)?;
        }
        Ok(result)
    }
}
