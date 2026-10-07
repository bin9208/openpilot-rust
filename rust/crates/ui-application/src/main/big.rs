use super::*;
use crate::layouts::{
    home::Home,
    sidebar::{Sidebar, WIDTH},
};
use crate::settings::layout::Settings;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Mode {
    Home,
    Settings,
    Onroad,
}
#[derive(Default)]
struct Requests {
    mode_for_state: bool,
    toggle_sidebar: bool,
    settings_panel: Option<Panel>,
}
pub(super) struct Big {
    pub mode: Mode,
    previous_onroad: bool,
    home: Home,
    pub settings: Settings,
    pub road: Road,
    sidebar: Sidebar,
    sidebar_rect: Rect,
    content_rect: Rect,
    requests: Rc<RefCell<Requests>>,
}
impl Big {
    pub fn new(
        context: Context,
        canvas: &mut Canvas,
        resources: Resources,
        recording: Rc<Cell<bool>>,
        camera: &str,
    ) -> Result<Self, Error> {
        let requests = Rc::new(RefCell::new(Requests::default()));
        let mut sidebar = Sidebar::new(context.clone(), canvas)?;
        let request = requests.clone();
        sidebar.on_settings = Some(Callback::new(move |()| {
            request.borrow_mut().settings_panel = Some(Panel::Device)
        }));
        let request = requests.clone();
        sidebar.open_settings = Some(Callback::new(move |()| {
            request.borrow_mut().settings_panel = Some(Panel::Toggles)
        }));
        let c = context.clone();
        sidebar.on_carrot_web = Some(Callback::new(move |()| c.open(Page::CarrotWeb)));
        let mut home = Home::new(context.clone(), canvas)?;
        let request = requests.clone();
        home.set_settings_callback(Callback::new(move |()| {
            request.borrow_mut().settings_panel = Some(Panel::Toggles)
        }));
        let mut settings = Settings::new(context.clone(), canvas, resources)?;
        let request = requests.clone();
        settings.on_close = Some(Callback::new(move |()| {
            request.borrow_mut().mode_for_state = true
        }));
        let request = requests.clone();
        context.listen(
            crate::context::Event::InteractiveTimeout,
            Callback::new(move |()| request.borrow_mut().mode_for_state = true),
        );
        let mut road = Road::with_camera(context, canvas, recording, camera)?;
        let request = requests.clone();
        road.state_mut().click = Some(Box::new(move || request.borrow_mut().toggle_sidebar = true));
        Ok(Self {
            mode: Mode::Home,
            previous_onroad: false,
            home,
            settings,
            road,
            sidebar,
            sidebar_rect: Rect::default(),
            content_rect: Rect::default(),
            requests,
        })
    }
    fn layout_widget(&mut self) -> &mut dyn Widget {
        match self.mode {
            Mode::Home => &mut self.home,
            Mode::Settings => &mut self.settings,
            Mode::Onroad => &mut self.road,
        }
    }
    fn set_mode(&mut self, mode: Mode, frame: &Frame<'_>) {
        if mode == self.mode {
            return;
        }
        self.layout_widget().hide(frame);
        self.mode = mode;
        self.layout_widget().show(frame);
    }
    pub fn mode_for_state(&mut self, started: bool, frame: &Frame<'_>) {
        if started {
            if self.mode != Mode::Onroad {
                self.sidebar.state.visible = false.into();
            }
            self.set_mode(Mode::Onroad, frame);
        } else {
            self.set_mode(Mode::Home, frame);
            self.sidebar.state.visible = true.into();
        }
    }
    pub fn open_settings(&mut self, panel: Panel, frame: &Frame<'_>) -> Result<(), Error> {
        self.settings.set_current(panel, frame)?;
        self.set_mode(Mode::Settings, frame);
        self.sidebar.state.visible = false.into();
        Ok(())
    }
    pub fn layout(&mut self, rect: Rect) {
        self.sidebar_rect = Rect {
            x: rect.x,
            y: rect.y,
            width: WIDTH,
            height: rect.height,
        };
        let offset = if self.sidebar.state.visible.get() {
            WIDTH
        } else {
            0.0
        };
        self.content_rect = Rect {
            x: float(f64::from(rect.y) + f64::from(offset)),
            y: rect.y,
            width: rect.width - offset,
            height: rect.height,
        };
    }
    pub fn paint(
        &mut self,
        context: &Context,
        state: &WidgetState,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<RenderResult, Error> {
        let started = context.ui.borrow().started;
        if started != self.previous_onroad {
            self.previous_onroad = started;
            self.mode_for_state(started, frame);
        }
        if started {
            self.road.set_cluster_hud_connected(
                context.params.boolean("ClusterHudConnected")?,
                context.ui.borrow().slow.show_camera_with_cluster,
            );
        }
        self.process(context, frame)?;
        if self.sidebar.state.visible.get() {
            self.sidebar.set_rect(self.sidebar_rect);
            self.sidebar.state.enabled = state.enabled.get().into();
            self.sidebar.render(frame, draw)?;
            self.process(context, frame)?;
        }
        let rect = if self.sidebar.state.visible.get() {
            self.content_rect
        } else {
            state.rect
        };
        let child = self.layout_widget();
        child.set_rect(rect);
        child.state_mut().enabled = state.enabled.get().into();
        let result = child.render(frame, draw)?;
        self.process(context, frame)?;
        Ok(result)
    }
    pub fn process(&mut self, context: &Context, frame: &Frame<'_>) -> Result<(), Error> {
        let (mode, toggle, panel) = {
            let mut requests = self.requests.borrow_mut();
            (
                std::mem::take(&mut requests.mode_for_state),
                std::mem::take(&mut requests.toggle_sidebar),
                requests.settings_panel.take(),
            )
        };
        if mode {
            self.mode_for_state(context.ui.borrow().started, frame);
        }
        if toggle {
            self.sidebar.state.visible = (!self.sidebar.state.visible.get()).into();
        }
        if let Some(panel) = panel {
            self.open_settings(panel, frame)?;
        }
        Ok(())
    }
    pub fn sidebar_visible(&self) -> bool {
        self.sidebar.state.visible.get()
    }
}
