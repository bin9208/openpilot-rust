mod big;
mod compact;
mod recording;
mod shared;
use crate::{
    context::{Action, Context, Page, Panel},
    onroad::augmented::Road,
    params::Read,
    settings::resources::Resources,
    state::messages,
};
pub use big::Mode;
use openpilot_ui_framework::{
    callback::Callback,
    canvas::Canvas,
    draw::Draw,
    geometry::Rect,
    text_layout::float,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
enum Layout {
    Big(Box<big::Big>),
    Compact(Box<compact::Compact>),
}
pub struct Main {
    state: WidgetState,
    context: Context,
    layout: Layout,
    recording: recording::Recording,
    layout_dirty: bool,
}
#[derive(serde::Serialize)]
pub struct Snapshot {
    pub mode: Option<Mode>,
    pub settings_panel: Option<String>,
    pub sidebar: bool,
    pub scroll: Option<f64>,
    pub in_plot_mode: bool,
    pub recording: bool,
}
impl Main {
    pub fn new(
        context: Context,
        canvas: &mut Canvas,
        resources: Resources,
        recording: Rc<Cell<bool>>,
        camera: &str,
    ) -> Result<Self, Error> {
        let (width, height) = canvas.renderer.dimensions();
        let rect = Rect {
            width,
            height,
            ..Default::default()
        };
        let layout = if context.big {
            Layout::Big(Box::new(big::Big::new(
                context.clone(),
                canvas,
                resources,
                recording.clone(),
                camera,
            )?))
        } else {
            Layout::Compact(Box::new(compact::Compact::new(
                context.clone(),
                canvas,
                resources,
                recording.clone(),
                camera,
                rect,
            )?))
        };
        Ok(Self {
            state: WidgetState::default(),
            recording: recording::Recording::new(context.clone(), recording),
            context,
            layout,
            layout_dirty: true,
        })
    }
    pub fn open(&mut self, page: Page, frame: &Frame<'_>) -> Result<bool, Error> {
        match (&mut self.layout, page) {
            (Layout::Big(big), Page::Home) => {
                big.mode_for_state(self.context.ui.borrow().started, frame);
                Ok(true)
            }
            (Layout::Big(big), Page::Settings(panel)) => {
                big.open_settings(panel, frame)?;
                Ok(true)
            }
            (Layout::Compact(compact), Page::Settings(_)) => {
                frame
                    .navigation
                    .push(NavigationRequest::Push(compact.settings.clone()));
                Ok(true)
            }
            (Layout::Compact(compact), Page::Home) => {
                compact.in_plot_mode = false;
                compact.scroller.scroll_to(
                    f64::from(
                        compact
                            .scroller
                            .item(1)
                            .ok_or(Error::Contract("root home missing"))?
                            .state()
                            .rect
                            .x,
                    )
                    .trunc(),
                    true,
                    false,
                    false,
                )?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub fn tick(
        &mut self,
        root: &WidgetHandle,
        onboarding: &WidgetHandle,
        stack: &openpilot_ui_framework::stack::NavigationStack,
        frame: &Frame<'_>,
    ) -> Result<(), Error> {
        match &mut self.layout {
            Layout::Compact(compact) => compact.tick(&self.context, root, onboarding, stack, frame),
            Layout::Big(big) => big.process(&self.context, frame),
        }
    }
    pub fn finish_render(
        &mut self,
        diagnostics: &mut openpilot_startup_ui::diagnostics::Diagnostics,
        renderer: &mut openpilot_startup_ui::renderer::Renderer,
    ) -> Result<(), Error> {
        self.recording.update(diagnostics, renderer)
    }
    pub fn events(
        &mut self,
        root: &WidgetHandle,
        onboarding: &WidgetHandle,
        stack: &openpilot_ui_framework::stack::NavigationStack,
        frame: &Frame<'_>,
    ) -> Result<(), Error> {
        match &mut self.layout {
            Layout::Big(big) => big.process(&self.context, frame),
            Layout::Compact(compact) => {
                compact.timeout(&self.context, root, onboarding, stack, frame)
            }
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        let (mode, sidebar, scroll, in_plot_mode) = match &self.layout {
            Layout::Big(big) => (Some(big.mode), big.sidebar_visible(), None, false),
            Layout::Compact(compact) => (
                None,
                false,
                Some(compact.scroller.panel.offset()),
                compact.in_plot_mode,
            ),
        };
        let settings_panel = match &self.layout {
            Layout::Big(big) if big.mode == Mode::Settings => {
                Some(format!("{:?}", big.settings.current()))
            }
            _ => None,
        };
        Snapshot {
            mode,
            settings_panel,
            sidebar,
            scroll,
            in_plot_mode,
            recording: self.recording.active(),
        }
    }
}
impl Widget for Main {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn layout_changed(&mut self) {
        match &mut self.layout {
            Layout::Big(big) => big.layout(self.state.rect),
            Layout::Compact(_) => self.layout_dirty = true,
        }
    }
    fn layout(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        if std::mem::take(&mut self.layout_dirty) {
            match &mut self.layout {
                Layout::Big(_) => {}
                Layout::Compact(compact) => compact.layout(self.state.rect)?,
            }
        }
        Ok(())
    }
    fn show(&mut self, frame: &Frame<'_>) {
        if let Layout::Compact(compact) = &mut self.layout {
            compact.scroller.show(frame);
        }
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        if let Layout::Compact(compact) = &mut self.layout {
            compact.scroller.hide(frame);
        }
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let result = match &mut self.layout {
            Layout::Big(big) => big.paint(&self.context, &self.state, frame, draw)?,
            Layout::Compact(compact) => compact.paint(&self.state, frame, draw)?,
        };
        self.recording.rendered = true;
        Ok(result)
    }
}
