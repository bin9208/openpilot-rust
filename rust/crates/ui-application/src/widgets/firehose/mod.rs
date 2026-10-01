mod copy;
mod count;
mod render;
use crate::{
    context::Context,
    services::{firehose::Firehose as Service, polling::Poller},
    state::messages,
};
use openpilot_logmessaged::JsonValue;
use openpilot_startup_ui::scroll::Scroll;
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::Draw,
    navigation::NavWidget,
    scroll::ScrollPanel,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Mutex},
};
pub struct Firehose {
    pub state: WidgetState,
    context: Context,
    pub count: Arc<Mutex<JsonValue>>,
    legacy: Scroll,
    modern: ScrollPanel,
    content_height: f64,
    offset: Rc<Cell<f64>>,
    _worker: Poller,
}
impl Firehose {
    pub fn new(context: Context) -> Result<Self, Error> {
        let service = Service::new(context.params.raw.clone(), context.api.clone())
            .map_err(|error| Error::Io(std::io::Error::other(error)))?;
        let count = service.count.clone();
        let worker = service.start(context.poll_gate.clone())?;
        Ok(Self {
            state: WidgetState::default(),
            count,
            legacy: Scroll::default(),
            modern: ScrollPanel::new(false, true, !context.pc),
            content_height: 0.0,
            offset: Rc::default(),
            context,
            _worker: worker,
        })
    }
    pub fn navigation(self, canvas: &Canvas) -> NavWidget {
        let offset = self.offset.clone();
        let mut nav = NavWidget::new(
            Box::new(self),
            20.0,
            f64::from(canvas.renderer.config.height()),
        );
        nav.motion.back_area = 1.0;
        nav.back_enabled = Box::new(move || offset.get() >= -20.0);
        nav
    }
    fn status(&self) -> Result<(String, u32), Error> {
        let messages = self.context.messages.borrow();
        let device = messages::device_state(&messages.state)?;
        let active = !device.get_network_metered()
            && u16::from(device.get_network_type().map_err(crate::Error::from)?) != 0;
        Ok(if active {
            (
                self.context.tr("ACTIVE"),
                crate::paint::color(46, 204, 113, 255),
            )
        } else {
            (
                self.context.tr("INACTIVE: connect to an unmetered network"),
                crate::paint::color(231, 76, 60, 255),
            )
        })
    }
}
impl Widget for Firehose {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, _: &Frame<'_>) {
        self.legacy.set_offset(0.0);
        self.modern.set_offset(0.0);
        self.offset.set(0.0);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if self.context.big {
            let rect = self.state.rect;
            let offset = self.legacy.update(
                rect,
                openpilot_ui_framework::text_layout::float(self.content_height),
                frame.events,
                openpilot_ui_framework::text_layout::float(frame.wheel),
            );
            draw.scissor(Some(openpilot_ui_framework::geometry::Rect {
                x: rect.x.trunc(),
                y: rect.y.trunc(),
                width: rect.width.trunc(),
                height: rect.height.trunc(),
            }))?;
            self.content_height = self.render_big(draw, f64::from(offset))?;
            draw.scissor(None)?;
        } else {
            self.modern.enabled = self.state.enabled.get().into();
            let height = self.measure_small(draw)?;
            let offset = self
                .modern
                .update(self.state.rect, height, frame.events, frame.dt);
            self.offset.set(self.modern.offset());
            self.render_small(draw, offset)?;
        }
        Ok(RenderResult::None)
    }
}
