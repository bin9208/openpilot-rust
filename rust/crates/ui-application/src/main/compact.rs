use super::*;
use crate::mici::{
    layouts::{home::Home, offroad_alerts::OffroadAlerts},
    onroad::debug_plot::DebugPlot,
    settings::layout::Settings,
};
use openpilot_ui_framework::{scroller::Scroller, widget::Property};
pub(super) struct Compact {
    pub scroller: Scroller,
    pub settings: WidgetHandle,
    alerts: WidgetHandle,
    pub road: WidgetHandle,
    children: [WidgetHandle; 4],
    previous_onroad: bool,
    previous_standstill: bool,
    onroad_delay: Option<f64>,
    setup: bool,
    show_plot_mode: i32,
    pub in_plot_mode: bool,
    scroll_home: Rc<Cell<bool>>,
    timeout: Rc<Cell<bool>>,
}
impl Compact {
    pub fn new(
        context: Context,
        canvas: &mut Canvas,
        resources: Resources,
        recording: Rc<Cell<bool>>,
        camera: &str,
        rect: Rect,
    ) -> Result<Self, Error> {
        let mut home = Home::new(context.clone(), canvas)?;
        let c = context.clone();
        home.on_settings = Some(Callback::new(move |()| {
            c.open(Page::Settings(Panel::Device))
        }));
        let c = context.clone();
        home.on_carrot_web = Some(Callback::new(move |()| c.open(Page::CarrotWeb)));
        let alerts = WidgetHandle::new(OffroadAlerts::new(context.clone(), canvas)?);
        let settings =
            WidgetHandle::new(Settings::new(context.clone(), canvas, resources)?.navigation());
        let mut road = Road::with_camera(context.clone(), canvas, recording, camera)?;
        let scroll_home = Rc::new(Cell::new(false));
        let request = scroll_home.clone();
        road.state_mut().click = Some(Box::new(move || request.set(true)));
        let road = WidgetHandle::new(road);
        let children = [
            alerts.clone(),
            WidgetHandle::new(home),
            road.clone(),
            WidgetHandle::new(DebugPlot::new(context.clone(), rect)),
        ];
        let mut scroller = Scroller::new(true, true, !context.pc, 20.0);
        scroller.spacing = 0.0;
        scroller.padding = 0.0;
        scroller.edge_shadows = false;
        scroller.reset_on_show = false;
        for child in &children {
            child.borrow_mut()?.set_rect(rect);
            scroller.add(Box::new(shared::Shared::new(child.clone())?))?;
        }
        settings.borrow_mut()?.set_rect(rect);
        let weak = road.downgrade();
        scroller.scrolling_enabled = Property::Dynamic(Box::new(move || {
            weak.upgrade()
                .and_then(|road| road.get::<Road>().ok().map(|road| !road.is_swiping_left()))
                .unwrap_or(true)
        }));
        let timeout = Rc::new(Cell::new(false));
        let request = timeout.clone();
        context.listen(
            crate::context::Event::InteractiveTimeout,
            Callback::new(move |()| request.set(true)),
        );
        Ok(Self {
            scroller,
            settings,
            alerts,
            road,
            children,
            previous_onroad: false,
            previous_standstill: false,
            onroad_delay: None,
            setup: false,
            show_plot_mode: 0,
            in_plot_mode: false,
            scroll_home,
            timeout,
        })
    }
    fn scroll(&mut self, index: usize) -> Result<(), Error> {
        let plot = index == 3;
        if !plot && self.in_plot_mode {
            return Ok(());
        }
        self.in_plot_mode = plot;
        let position = f64::from(
            self.scroller
                .item(index)
                .ok_or(Error::Contract("root scroller item missing"))?
                .state()
                .rect
                .x,
        )
        .trunc();
        self.scroller.scroll_to(position, true, false, false)
    }
    fn pop_to(&self, root: &WidgetHandle, frame: &Frame<'_>, index: usize, instant: bool) {
        let weak = root.downgrade();
        frame.navigation.push(NavigationRequest::PopTo {
            target: root.clone(),
            instant,
            callback: Some(Box::new(move || {
                if let Some(root) = weak.upgrade() {
                    if let Ok(mut main) = root.get_mut::<Main>() {
                        if let Layout::Compact(compact) = &mut main.layout {
                            if let Err(error) = compact.scroll(index) {
                                main.context
                                    .actions
                                    .push(Action::Failure(crate::Error::Ui(error)));
                            }
                        }
                    }
                }
            })),
        });
    }
    pub fn tick(
        &mut self,
        context: &Context,
        root: &WidgetHandle,
        onboarding: &WidgetHandle,
        stack: &openpilot_ui_framework::stack::NavigationStack,
        frame: &Frame<'_>,
    ) -> Result<(), Error> {
        if stack.contains(onboarding) {
            return Ok(());
        }
        let started = context.ui.borrow().started;
        if started != self.previous_onroad {
            self.previous_onroad = started;
            if started {
                self.onroad_delay = Some(frame.now);
            } else {
                self.scroll(1)?;
            }
        }
        if self
            .onroad_delay
            .is_some_and(|time| frame.now - time >= 2.5)
        {
            self.pop_to(root, frame, 2, false);
            self.onroad_delay = None;
        }
        if started {
            let plot = context.params.integer("ShowPlotMode")?;
            let connected = context.params.boolean("ClusterHudConnected")?;
            self.road.get_mut::<Road>()?.set_cluster_hud_connected(
                connected,
                context.ui.borrow().slow.show_camera_with_cluster,
            );
            let effective = if connected { 0 } else { plot };
            if effective != self.show_plot_mode {
                self.show_plot_mode = effective;
                if effective > 0 {
                    self.scroll(3)?;
                } else {
                    self.in_plot_mode = false;
                    self.scroll(2)?;
                }
            }
        }
        let standstill = messages::car_state(&context.messages.borrow().state)?.get_standstill();
        if !standstill && self.previous_standstill {
            self.pop_to(root, frame, 2, false);
        }
        self.previous_standstill = standstill;
        self.timeout(context, root, onboarding, stack, frame)?;
        Ok(())
    }
    pub fn timeout(
        &mut self,
        context: &Context,
        root: &WidgetHandle,
        onboarding: &WidgetHandle,
        stack: &openpilot_ui_framework::stack::NavigationStack,
        frame: &Frame<'_>,
    ) -> Result<(), Error> {
        if !self.timeout.replace(false) || stack.contains(onboarding) {
            return Ok(());
        }
        if context.ui.borrow().started {
            if !messages::car_state(&context.messages.borrow().state)?.get_standstill() {
                self.pop_to(root, frame, 2, false);
            }
        } else {
            frame.navigation.push(NavigationRequest::PopTo {
                target: root.clone(),
                instant: true,
                callback: None,
            });
            self.scroll(1)?;
        }
        Ok(())
    }
    pub fn layout(&mut self, rect: Rect) -> Result<(), Error> {
        for index in 0..self.children.len() {
            let widget = self
                .scroller
                .item_mut(index)
                .ok_or(Error::Contract("root scroller item missing"))?;
            widget.set_rect(Rect {
                width: rect.width,
                height: rect.height,
                ..widget.state().rect
            });
        }
        self.scroller.set_rect(rect);
        Ok(())
    }
    pub fn paint(
        &mut self,
        state: &WidgetState,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<RenderResult, Error> {
        if !self.setup {
            let alerts = self.alerts.get::<OffroadAlerts>()?.active_alerts();
            self.scroller.scroll_to(
                if alerts > 0 {
                    f64::from(
                        self.scroller
                            .item(0)
                            .ok_or(Error::Contract("root alerts item missing"))?
                            .state()
                            .rect
                            .x,
                    )
                } else {
                    f64::from(state.rect.width)
                },
                false,
                false,
                false,
            )?;
            self.setup = true;
        }
        self.scroller.state.enabled = state.enabled.get().into();
        let result = self.scroller.render(frame, draw)?;
        if self.scroll_home.replace(false) {
            self.scroll(1)?;
        }
        Ok(result)
    }
}
