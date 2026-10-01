//! Compact device settings from mici/layouts/settings/device.py.
mod build;
mod info;
pub(crate) mod pair;
mod updater;
use crate::{
    context::{Action, Context, Event, Page},
    mici::widgets::{big_button::BigButton, circle_button::CircleButton, dialog::Confirmation},
    paint,
};
use openpilot_ui_framework::{
    assets::Texture,
    callback::Callback,
    canvas::Canvas,
    draw::Draw,
    navigation::NavWidget,
    scroller::Scroller,
    widget::{Frame, Property, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use std::rc::Rc;
pub struct Device {
    state: WidgetState,
    pub scroller: Scroller,
}
impl Device {
    pub fn create(context: Context, canvas: &mut Canvas) -> Result<WidgetHandle, Error> {
        let content = Self::new(context.clone(), canvas)?;
        let widget = WidgetHandle::new(NavWidget::new(Box::new(content), 20.0, 240.0));
        let weak = widget.downgrade();
        let actions = context.actions.clone();
        context.listen(
            Event::Offroad,
            Callback::new(move |()| {
                let result = (|| -> Result<(), crate::Error> {
                    let Some(widget) = weak.upgrade() else {
                        return Ok(());
                    };
                    let mut nav = widget.get_mut::<NavWidget>()?;
                    let content = (nav.content.as_mut() as &mut dyn std::any::Any)
                        .downcast_mut::<Self>()
                        .ok_or(Error::Contract("Mici device content"))?;
                    let button = content
                        .scroller
                        .item_mut(1)
                        .and_then(|item| {
                            (item as &mut dyn std::any::Any).downcast_mut::<updater::Updater>()
                        })
                        .ok_or(Error::Contract("Mici updater button"))?;
                    button.offroad();
                    Ok(())
                })();
                if let Err(error) = result {
                    actions.push(Action::Failure(error));
                }
            }),
        );
        Ok(widget)
    }
}
#[derive(Clone, Copy)]
enum Operation {
    Reset,
    Uninstall,
    Reboot,
    Shutdown,
}
fn engaged(context: Context, operation: Operation, icon: Texture) {
    let title = match operation {
        Operation::Reset => "reset",
        Operation::Uninstall => "uninstall",
        Operation::Reboot => "reboot",
        Operation::Shutdown => "power off",
    };
    if context.ui.borrow().engaged {
        context.actions.push(Action::MiciAlert {
            title: String::new(),
            description: format!("Disengage to {title}"),
        });
        return;
    }
    let actions = context.actions.clone();
    let callback = Rc::new(move || {
        if context.ui.borrow().engaged {
            return;
        }
        let result = (|| -> Result<(), crate::Error> {
            let params = &context.params;
            match operation {
                Operation::Reset => {
                    for key in [
                        "CalibrationParams",
                        "LiveTorqueParameters",
                        "LiveParameters",
                        "LiveParametersV2",
                        "LiveDelay",
                    ] {
                        params.remove(key)?;
                    }
                    params.put_bool("OnroadCycleRequested", true)?;
                }
                Operation::Uninstall => params.put_bool("DoUninstall", true)?,
                Operation::Reboot => params.put_bool("DoReboot", true)?,
                Operation::Shutdown => params.put_bool("DoShutdown", true)?,
            }
            Ok(())
        })();
        if let Err(error) = result {
            context.actions.push(Action::Failure(error));
        }
    });
    actions.push(Action::MiciConfirm(Confirmation {
        title: format!("slide to\n{title}"),
        icon,
        callback,
        exit_on_confirm: matches!(operation, Operation::Reset),
        red: matches!(operation, Operation::Shutdown),
    }));
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
        self.scroller.state.enabled = self.state.enabled.get().into();
        self.scroller.set_rect(self.state.rect);
        self.scroller.render(frame, draw)
    }
}
