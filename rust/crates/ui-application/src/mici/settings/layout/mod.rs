mod button;
use super::{
    developer::Developer,
    device::{pair::Pair, Device},
    egpu::Egpu,
    network::{Connection, Network},
    toggles::Toggles,
};
use crate::{
    context::{Action, Context},
    paint,
    params::Read,
    settings::resources::Resources,
    widgets::firehose::Firehose,
};
use button::{Button, Icon};
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::Draw,
    navigation::NavWidget,
    scroller::Scroller,
    widget::{Frame, Property, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};

pub struct Settings {
    state: WidgetState,
    pub scroller: Scroller,
}
impl Settings {
    pub fn new(context: Context, canvas: &mut Canvas, resources: Resources) -> Result<Self, Error> {
        let toggles = Toggles::create(context.clone(), canvas)?;
        let network = WidgetHandle::new(
            Network::new(
                context.clone(),
                canvas,
                Connection {
                    session: resources.network.session,
                    ticks: resources.ticks,
                },
            )?
            .navigation(),
        );
        let device = Device::create(context.clone(), canvas)?;
        let developer = WidgetHandle::new(Developer::new(context.clone(), canvas)?.navigation());
        let firehose = WidgetHandle::new(Firehose::new(context.clone())?.navigation(canvas));
        let egpu =
            WidgetHandle::new(Egpu::new(context.clone(), canvas, resources.egpu)?.navigation());
        let mut scroller = Scroller::new(true, false, !context.pc, 20.0);
        scroller.indicator = Some(paint::texture(
            canvas,
            "icons_mici/settings/horizontal_scroll_indicator.png",
            (96, 48),
        )?);
        for (title, icon, size, target) in [
            ("toggles", "icons_mici/settings.png", (64, 64), toggles),
            (
                "network",
                "icons_mici/settings/network/wifi_strength_full.png",
                (76, 56),
                network,
            ),
            (
                "device",
                "icons_mici/settings/device_icon.png",
                (72, 58),
                device,
            ),
            (
                "firehose",
                "icons_mici/settings/firehose.png",
                (52, 62),
                firehose,
            ),
            (
                "eGPU",
                "icons_mici/settings/network/wifi_strength_full.png",
                (76, 56),
                egpu,
            ),
            (
                "developer",
                "icons_mici/settings/developer_icon.png",
                (64, 60),
                developer,
            ),
        ] {
            if title == "firehose" {
                scroller.add(Box::new(Pair::new(context.clone(), canvas)?))?;
            }
            let mut button = Button::new(canvas, title, Icon { path: icon, size }, target)?;
            if title == "eGPU" {
                let ctx = context.clone();
                button.button.state.visible = Property::Dynamic(Box::new(move || {
                    let ui = ctx.ui.borrow();
                    if ui.slow.usbgpu_present || ui.slow.usbgpu_compiled {
                        return true;
                    }
                    match ctx.params.string("GitBranch") {
                        Ok(branch) => branch == "carrot-egpu",
                        Err(error) => {
                            ctx.actions.push(Action::Failure(error));
                            false
                        }
                    }
                }));
            }
            scroller.add(Box::new(button))?;
        }
        Ok(Self {
            state: WidgetState::default(),
            scroller,
        })
    }
    pub fn navigation(self) -> NavWidget {
        NavWidget::new(Box::new(self), 20.0, 240.0)
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
