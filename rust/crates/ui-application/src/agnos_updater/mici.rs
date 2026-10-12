use super::{
    view::{Action, Shared},
    worker::State,
};
use crate::{
    context::Action as ProductAction,
    mici::{
        settings::network::wifi::{self, Wifi, WifiContext},
        widgets::{big_button::BigButton, circle_button::CircleButton, dialog},
    },
    paint,
};
use openpilot_ui_framework::{
    button::Button,
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    navigation::NavWidget,
    network::WifiSession,
    scroller::Scroller,
    text::Font,
    text_layout::{float, Horizontal, Vertical},
    unified_label::UnifiedLabel,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use std::{cell::Cell, rc::Rc};

pub struct Pill {
    pub button: BigButton,
}
impl Pill {
    pub fn new(text: &str, disabled_background: bool, canvas: &mut Canvas) -> Result<Self, Error> {
        let button = BigButton::new(text, |path, size| {
            let mapped = match path {
                "icons_mici/buttons/button_rectangle.png" => {
                    if disabled_background {
                        "icons_mici/setup/continue_disabled.png"
                    } else {
                        "icons_mici/setup/continue.png"
                    }
                }
                "icons_mici/buttons/button_rectangle_pressed.png" => {
                    if disabled_background {
                        "icons_mici/setup/continue_disabled.png"
                    } else {
                        "icons_mici/setup/continue_pressed.png"
                    }
                }
                "icons_mici/buttons/button_rectangle_disabled.png" => {
                    "icons_mici/setup/continue_disabled.png"
                }
                path => path,
            };
            paint::texture(canvas, mapped, size)
        })?;
        Ok(Self { button })
    }
}
impl Widget for Pill {
    fn state(&self) -> &WidgetState {
        &self.button.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.button.state
    }
    fn set_position(&mut self, x: f32, y: f32) {
        self.button.set_position(x, y);
    }
    fn set_rect(&mut self, rect: Rect) {
        self.button.set_rect(rect);
    }
    fn mouse_release(
        &mut self,
        p: Point,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.button.mouse_release(p, frame, draw)
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.button
            .paint_with(frame, draw, |button, frame, draw, y| {
                button.label.text = button.text.clone().into();
                button.label.size = 48.0;
                button.label.horizontal = Horizontal::Center;
                button.label.vertical = Vertical::Middle;
                button.label.color = paint::color(
                    255,
                    255,
                    255,
                    if button.state.enabled.get() { 229 } else { 89 },
                );
                let rect = button.state.rect;
                button.label.set_rect(Rect {
                    x: rect.x + 40.0,
                    y: float(y + 23.0),
                    width: rect.width - 80.0,
                    height: rect.height - 46.0,
                });
                button.label.render(frame, draw)?;
                Ok(())
            })?;
        Ok(RenderResult::None)
    }
    fn render(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let gate = self.button.state.interaction_gate;
        if self.button.state.parent_rect.is_some()
            && self
                .button
                .grow_until
                .is_some_and(|until| frame.now < until)
        {
            self.button.state.interaction_gate = false;
        }
        let result = openpilot_ui_framework::widget::render_widget(self, frame, draw);
        self.button.state.interaction_gate = gate;
        result
    }
}
fn scroller(pc: bool, canvas: &mut Canvas) -> Result<Scroller, Error> {
    let mut s = Scroller::new(true, false, !pc, 20.0);
    s.indicator = Some(paint::texture(
        canvas,
        "icons_mici/settings/horizontal_scroll_indicator.png",
        (96, 48),
    )?);
    Ok(s)
}
fn grey(
    text: &str,
    value: &str,
    icon: Option<(&str, (i32, i32))>,
    canvas: &mut Canvas,
) -> Result<BigButton, Error> {
    let mut button = BigButton::new(text, |p, s| paint::texture(canvas, p, s))?;
    button.set_grey();
    button.value = value.into();
    if let Some((p, s)) = icon {
        button.icon = Some(paint::texture(canvas, p, s)?);
    }
    Ok(button)
}
fn pill(
    text: &str,
    action: Action,
    shared: &Rc<Shared>,
    canvas: &mut Canvas,
) -> Result<Pill, Error> {
    let mut button = Pill::new(text, false, canvas)?;
    let shared = shared.clone();
    button.button.state.click = Some(Box::new(move || shared.action(action)));
    Ok(button)
}
struct Progress {
    state: WidgetState,
    shared: Rc<Shared>,
    title: UnifiedLabel,
    percent: UnifiedLabel,
    wifi: Button,
}
impl Progress {
    fn new(shared: Rc<Shared>) -> Self {
        let mut title = UnifiedLabel::new("");
        title.size = 64.0;
        title.font = Font::Display;
        title.line_height = 0.8;
        title.color = paint::color(255, 255, 255, 229);
        let mut percent = UnifiedLabel::new("");
        percent.size = 132.0;
        percent.font = Font::Regular;
        percent.vertical = Vertical::Bottom;
        percent.color = paint::color(255, 255, 255, 149);
        let mut wifi = Button::new("Wi-Fi");
        wifi.label.size = 36.0;
        let model = shared.clone();
        wifi.state.click = Some(Box::new(move || model.action(Action::Network)));
        Self {
            state: WidgetState::default(),
            shared,
            title,
            percent,
            wifi,
        }
    }
}
impl Widget for Progress {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        draw.rounded(rect, 0.0, paint::color(0, 0, 0, 255))?;
        let state = self.shared.state.borrow();
        let text = if state.text == "waiting for internet" {
            "waiting for\ninternet"
        } else {
            &state.text
        };
        self.title.text = format!("{}...", text.replace('_', "_\n")).into();
        self.percent.text = format!("{}%", state.progress).into();
        let height = float(
            self.title
                .content_height(draw, f64::from(rect.width - 20.0)),
        );
        self.title.set_rect(Rect {
            x: rect.x + 12.0,
            y: rect.y + 2.0,
            width: rect.width,
            height,
        });
        self.percent.set_rect(Rect {
            x: rect.x + 12.0,
            y: rect.y + 18.0,
            width: rect.width - 180.0,
            height: rect.height,
        });
        self.wifi.set_rect(Rect {
            x: rect.x + rect.width - 160.0,
            y: rect.y + rect.height - 80.0,
            width: 148.0,
            height: 68.0,
        });
        self.title.render(frame, draw)?;
        self.percent.render(frame, draw)?;
        self.wifi.render(frame, draw)?;
        Ok(RenderResult::None)
    }
}
pub struct ProgressPage {
    pub nav: NavWidget,
}
impl Widget for ProgressPage {
    fn state(&self) -> &WidgetState {
        self.nav.state()
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        self.nav.state_mut()
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.nav.show(frame);
        self.nav.motion.bar_target = 0.0;
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.nav.hide(frame);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.nav.paint(frame, draw)
    }
    fn render(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.nav.render(frame, draw)
    }
    fn dismiss_navigation(&mut self, c: Option<Box<dyn FnOnce()>>, frame: &Frame<'_>) {
        self.nav.dismiss_navigation(c, frame);
    }
    fn take_navigation(&mut self) -> Option<NavigationRequest> {
        self.nav.take_navigation()
    }
}
#[derive(Default)]
struct NetworkFlags {
    prev_connected: Cell<bool>,
    prev_wifi: Cell<bool>,
    pending: Cell<Option<f64>>,
    end_requested: Cell<bool>,
}
pub struct Network {
    state: WidgetState,
    pub scroller: Scroller,
    shared: Rc<Shared>,
    session: WifiSession,
    flags: Rc<NetworkFlags>,
    end_after_show: bool,
    waiting_clicked: Rc<Cell<bool>>,
    grow_continue: bool,
    grow_wifi: bool,
}
impl Network {
    fn new(
        shared: Rc<Shared>,
        session: WifiSession,
        wifi: WidgetHandle,
        flags: Rc<NetworkFlags>,
        pc: bool,
        canvas: &mut Canvas,
    ) -> Result<Self, Error> {
        let mut scroller = scroller(pc, canvas)?;
        let mut hint = grey(
            "connect to\ninternet",
            "swipe down to go back",
            Some(("icons_mici/setup/small_slider/slider_arrow.png", (64, 56))),
            canvas,
        )?;
        hint.icon = Some(canvas.texture(
            "icons_mici/setup/small_slider/slider_arrow.png",
            openpilot_startup_ui::renderer::TextureOptions {
                width: Some(64),
                height: Some(56),
                flip_x: true,
                ..Default::default()
            },
        )?);
        scroller.add(Box::new(hint))?;
        scroller.add(wifi::network_button(session.clone(), wifi, canvas)?)?;
        scroller.add(Box::new(pill(
            "back to\nupdate",
            Action::Progress,
            &shared,
            canvas,
        )?))?;
        let mut waiting = Pill::new("waiting for\ninternet...", true, canvas)?;
        let waiting_clicked = Rc::new(Cell::new(false));
        let flag = waiting_clicked.clone();
        waiting.button.state.click = Some(Box::new(move || flag.set(true)));
        scroller.add(Box::new(waiting))?;
        Ok(Self {
            state: WidgetState::default(),
            scroller,
            shared,
            session,
            flags,
            end_after_show: false,
            waiting_clicked,
            grow_continue: false,
            grow_wifi: false,
        })
    }
    pub fn scroll_end(&mut self) -> Result<(), Error> {
        let remaining = self.scroller.panel.offset() + self.scroller.content_size
            - f64::from(self.state.rect.width);
        self.scroller.scroll_to(remaining, true, true, true)?;
        self.grow_continue = true;
        Ok(())
    }
}
impl Widget for Network {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.scroller.show(frame);
        self.grow_continue = false;
        self.grow_wifi = false;
        let connected = self.shared.connected.get();
        let wifi =
            self.session.snapshot().wifi_state.status == openpilot_wifi::ConnectStatus::Connected;
        self.flags.prev_connected.set(connected);
        self.flags.prev_wifi.set(wifi);
        self.flags.pending.set(None);
        self.flags.end_requested.set(false);
        self.end_after_show = connected || wifi;
        let _ = self.session.send(openpilot_wifi::Command::SetActive(true));
    }
    fn update(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<(), Error> {
        self.scroller
            .item_mut(3)
            .ok_or(Error::Contract("waiting button absent"))?
            .state_mut()
            .visible = (!self.shared.connected.get()).into();
        let requested = self.flags.end_requested.replace(false);
        let shown = self.end_after_show && self.state.rect.y == 0.0;
        if requested || shown {
            if shown {
                self.end_after_show = false;
            }
            let width = self.scroller.state.rect.width;
            self.scroller.layout(frame, draw)?;
            let remaining =
                self.scroller.panel.offset() + self.scroller.content_size - f64::from(width);
            self.scroller.scroll_to(remaining, true, true, true)?;
            self.grow_continue = true;
        }
        if self.grow_continue {
            let button = self
                .scroller
                .item_mut(2)
                .ok_or(Error::Contract("continue button absent"))?;
            if button.state().rect.x + button.state().rect.width
                < self.state.rect.x + self.state.rect.width + 50.0
            {
                self.grow_continue = false;
                let p = (button as &mut dyn std::any::Any)
                    .downcast_mut::<Pill>()
                    .ok_or(Error::Contract("continue button type"))?;
                p.button.grow_until = Some(frame.now + 0.65);
            }
        }
        if self.grow_wifi
            && (self
                .scroller
                .item(1)
                .ok_or(Error::Contract("wifi button absent"))?
                .state()
                .rect
                .x
                - 20.0)
                .abs()
                < 50.0
        {
            self.grow_wifi = false;
            wifi::grow_network_button(
                self.scroller
                    .item_mut(1)
                    .ok_or(Error::Contract("wifi button absent"))?,
                frame.now + 0.65,
            )?;
        }
        Ok(())
    }
    fn layout(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.scroller.state.enabled = self.state.enabled.get().into();
        self.scroller.set_rect(self.state.rect);
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller
            .item_mut(3)
            .ok_or(Error::Contract("waiting button absent"))?
            .state_mut()
            .visible = (!self.shared.connected.get()).into();
        self.scroller.set_rect(self.state.rect);
        let result = self.scroller.render(frame, draw)?;
        if self.waiting_clicked.replace(false) {
            let wifi = self
                .scroller
                .item(1)
                .ok_or(Error::Contract("wifi button absent"))?
                .state()
                .rect;
            let delta = f64::from(
                wifi.x + wifi.width / 2.0 - self.state.rect.x - self.state.rect.width / 2.0,
            );
            self.scroller.scroll_to(delta, true, true, true)?;
            self.grow_wifi = true;
        }
        Ok(result)
    }
}
pub struct Views {
    pub root: WidgetHandle,
    pub progress: WidgetHandle,
    pub failed: WidgetHandle,
    pub network: WidgetHandle,
    pub wifi: WidgetHandle,
    pub wifi_tick: openpilot_ui_framework::application::Tick,
    pub shared: Rc<Shared>,
    last_failure: Option<String>,
    flags: Rc<NetworkFlags>,
    ticks: openpilot_ui_framework::application::TickRegistry,
}
impl Views {
    pub fn new(
        shared: Rc<Shared>,
        context: WifiContext,
        session: WifiSession,
        canvas: &mut Canvas,
    ) -> Result<Self, Error> {
        let wifi = Wifi::with_context(context.clone(), session.clone(), canvas)?;
        let wifi_tick = wifi.tick.clone();
        let wifi = WidgetHandle::new(wifi.navigation());
        let mut root = scroller(context.pc, canvas)?;
        root.add(Box::new(grey(
            "updating system",
            "the device will\nrestart automatically",
            Some(("icons_mici/offroad_alerts/green_wheel.png", (64, 64))),
            canvas,
        )?))?;
        root.add(Box::new(pill(
            "Connect\nto Wi-Fi",
            Action::Network,
            &shared,
            canvas,
        )?))?;
        let root = WidgetHandle::new(root);
        let mut progress = NavWidget::new(Box::new(Progress::new(shared.clone())), 20.0, 240.0);
        progress.back_enabled = Box::new(|| false);
        let progress = WidgetHandle::new(ProgressPage { nav: progress });
        session.send(openpilot_wifi::Command::SetActive(true))?;
        let flags = Rc::new(NetworkFlags::default());
        let network = WidgetHandle::new(NavWidget::new(
            Box::new(Network::new(
                shared.clone(),
                session,
                wifi.clone(),
                flags.clone(),
                context.pc,
                canvas,
            )?),
            20.0,
            240.0,
        ));
        let mut failed = scroller(context.pc, canvas)?;
        failed.add(Box::new(pill("Retry", Action::Retry, &shared, canvas)?))?;
        failed.add(Box::new(pill(
            "Connect\nto Wi-Fi",
            Action::Network,
            &shared,
            canvas,
        )?))?;
        failed.add(Box::new(grey("update paused", "", None, canvas)?))?;
        let icon = paint::texture(canvas, "icons_mici/settings/device/reboot.png", (64, 70))?;
        let mut reboot = CircleButton::new(icon, |p, s| paint::texture(canvas, p, s))?;
        let actions = context.actions.clone();
        let model = shared.clone();
        reboot.state.click = Some(Box::new(move || {
            let model = model.clone();
            actions.push(ProductAction::MiciConfirm(dialog::Confirmation {
                title: "reboot\ndevice".into(),
                icon,
                callback: Rc::new(move || model.action(Action::Reboot)),
                exit_on_confirm: false,
                red: false,
            }));
        }));
        failed.add(Box::new(reboot))?;
        let mut failed = NavWidget::new(Box::new(failed), 20.0, 240.0);
        let model = shared.clone();
        failed.set_back_callback(Box::new(move || model.action(Action::Network)));
        let failed = WidgetHandle::new(failed);
        let ticks = openpilot_ui_framework::application::TickRegistry::default();
        ticks.add(wifi_tick.clone());
        Ok(Self {
            root,
            progress,
            failed,
            network,
            wifi,
            wifi_tick,
            shared,
            last_failure: None,
            flags,
            ticks,
        })
    }
    pub fn process_callbacks(&self) -> Result<(), Error> {
        self.ticks.run()
    }
    pub fn go(&self, destination: WidgetHandle, frame: &Frame<'_>) {
        let queue = frame.navigation.clone();
        frame.navigation.push(NavigationRequest::PopTo {
            target: self.root.clone(),
            instant: false,
            callback: Some(Box::new(move || {
                queue.push(NavigationRequest::Push(destination))
            })),
        });
    }
    pub fn failure(&mut self, state: &State, frame: &Frame<'_>) -> Result<(), Error> {
        if state.failure != self.last_failure {
            self.last_failure = state.failure.clone();
            if let Some(reason) = &state.failure {
                {
                    let mut nav = self.failed.get_mut::<NavWidget>()?;
                    let scroller = (nav.content.as_mut() as &mut dyn std::any::Any)
                        .downcast_mut::<Scroller>()
                        .ok_or(Error::Contract("failed scroller type"))?;
                    let button = (scroller
                        .item_mut(2)
                        .ok_or(Error::Contract("failure card absent"))?
                        as &mut dyn std::any::Any)
                        .downcast_mut::<BigButton>()
                        .ok_or(Error::Contract("failure card type"))?;
                    button.value = reason.clone();
                }
                self.root.borrow_mut()?.show(frame);
                self.go(self.failed.clone(), frame);
            }
        }
        Ok(())
    }
    pub fn network_tick(
        &mut self,
        frame: &Frame<'_>,
        stack: &openpilot_ui_framework::stack::NavigationStack,
        session: &WifiSession,
        connected: bool,
    ) -> Result<bool, Error> {
        let forgetting = {
            let mut nav = self.wifi.get_mut::<NavWidget>()?;
            (nav.content.as_mut() as &mut dyn std::any::Any)
                .downcast_mut::<Wifi>()
                .is_some_and(|wifi| wifi.any_network_forgetting())
        };
        let changing = forgetting
            || session.snapshot().wifi_state.status == openpilot_wifi::ConnectStatus::Connecting;
        let connected = connected && !changing;
        self.shared.connected.set(connected);
        let wifi_connected =
            session.snapshot().wifi_state.status == openpilot_wifi::ConnectStatus::Connected;
        let active = stack.active();
        if !active.as_ref().is_some_and(|w| w.same(&self.network)) && !stack.contains(&self.wifi) {
            return Ok(changing);
        }
        if (connected && !self.flags.prev_connected.get())
            || (wifi_connected && !self.flags.prev_wifi.get())
        {
            self.flags.pending.set(Some(frame.now));
        }
        self.flags.prev_connected.set(connected);
        self.flags.prev_wifi.set(wifi_connected);
        if self.flags.pending.get().is_some_and(|since| {
            frame.now - since > 0.7 || active.as_ref().is_some_and(|w| w.same(&self.network))
        }) {
            self.flags.pending.set(None);
            let network = self.network.clone();
            let flags = self.flags.clone();
            frame.navigation.push(NavigationRequest::PopTo {
                target: network,
                instant: false,
                callback: Some(Box::new(move || flags.end_requested.set(true))),
            });
        }
        Ok(changing)
    }
}
