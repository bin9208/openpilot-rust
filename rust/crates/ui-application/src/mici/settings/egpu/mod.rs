mod check;
mod info;
use crate::{
    context::{Action, Context},
    mici::widgets::{big_button::BigButton, dialog::Confirmation},
    paint,
    services::egpu::Backend,
};
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::Draw,
    navigation::NavWidget,
    scroller::Scroller,
    widget::{Frame, Property, RenderResult, Widget, WidgetState},
    Error,
};
use std::{rc::Rc, sync::Arc};

pub struct Egpu {
    state: WidgetState,
    pub scroller: Scroller,
}
impl Egpu {
    pub fn new(
        context: Context,
        canvas: &mut Canvas,
        backend: Arc<dyn Backend>,
    ) -> Result<Self, Error> {
        let mut scroller = Scroller::new(true, false, !context.pc, 20.0);
        scroller.indicator = Some(paint::texture(
            canvas,
            "icons_mici/settings/horizontal_scroll_indicator.png",
            (96, 48),
        )?);
        scroller.add(Box::new(info::Info::new(context.clone(), backend.clone())))?;
        scroller.add(Box::new(check::CheckButton::new(
            context.clone(),
            canvas,
            backend.clone(),
        )?))?;
        let icon = paint::texture(canvas, "icons_mici/settings/device/reboot.png", (64, 70))?;
        let mut compile = BigButton::new("compile model", |path, size| {
            paint::texture(canvas, path, size)
        })?;
        compile.icon = Some(icon);
        compile.value = "keep ignition on".into();
        let ui = context.ui.clone();
        compile.state.visible =
            Property::Dynamic(Box::new(move || !ui.borrow().slow.usbgpu_compiled));
        let ui = context.ui.clone();
        compile.state.enabled = Property::Dynamic(Box::new(move || !ui.borrow().started));
        compile.state.click = Some(Box::new(move || {
            let context = context.clone();
            let backend = backend.clone();
            let actions = context.actions.clone();
            actions.push(Action::MiciConfirm(Confirmation {
                title: "slide to reboot and compile".into(),
                icon,
                red: false,
                exit_on_confirm: false,
                callback: Rc::new(move || {
                    let result = backend
                        .remove_compiled_manifest()
                        .and_then(|()| context.params.put_bool("DoReboot", true));
                    if let Err(error) = result {
                        context.actions.push(Action::Failure(error));
                    }
                }),
            }));
        }));
        scroller.add(Box::new(compile))?;
        Ok(Self {
            state: WidgetState::default(),
            scroller,
        })
    }
    pub fn navigation(self) -> NavWidget {
        NavWidget::new(Box::new(self), 20.0, 240.0)
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
    fn hide(&mut self, frame: &Frame<'_>) {
        self.scroller.hide(frame);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller.state.enabled = self.state.enabled.get().into();
        self.scroller.set_rect(self.state.rect);
        self.scroller.render(frame, draw)
    }
}
