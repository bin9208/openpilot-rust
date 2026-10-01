//! Source: selfdrive/ui/layouts/home.py (MIT).
use super::{alerts::Alert, experimental::ExperimentalModeButton};
use crate::{
    context::Context,
    paint::{self, Label, Text},
    params::Read,
    widgets::{prime::PrimeWidget, setup::SetupWidget},
};
use openpilot_ui_framework::{
    callback::Callback,
    canvas::Canvas,
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{float, Horizontal},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum HomeState {
    #[default]
    Home,
    Update,
    Alerts,
}
pub struct Home {
    pub state: WidgetState,
    context: Context,
    pub current: HomeState,
    pub update_alert: Alert,
    pub offroad_alert: Alert,
    pub last_refresh: f64,
    pub update_available: bool,
    pub alert_count: usize,
    pub version: String,
    previous_update: bool,
    previous_alerts: bool,
    pub header: Rect,
    pub content: Rect,
    pub left: Rect,
    pub right: Rect,
    pub update_notification: Rect,
    pub alert_notification: Rect,
    prime: PrimeWidget,
    setup: SetupWidget,
    experimental: ExperimentalModeButton,
}
impl Home {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        Ok(Self {
            state: WidgetState::default(),
            current: HomeState::Home,
            update_alert: Alert::update(context.clone())?,
            offroad_alert: Alert::offroad(context.clone()),
            last_refresh: 0.0,
            update_available: false,
            alert_count: 0,
            version: String::new(),
            previous_update: false,
            previous_alerts: false,
            header: Rect::default(),
            content: Rect::default(),
            left: Rect::default(),
            right: Rect::default(),
            update_notification: Rect {
                width: 200.0,
                height: 70.0,
                ..Rect::default()
            },
            alert_notification: Rect {
                width: 220.0,
                height: 70.0,
                ..Rect::default()
            },
            prime: PrimeWidget::new(context.clone()),
            setup: SetupWidget::new(context.clone()),
            experimental: ExperimentalModeButton::new(context.clone(), canvas)?,
            context,
        })
    }
    pub fn set_settings_callback(&mut self, callback: Callback<()>) {
        self.experimental.state.click = Some(Box::new(move || callback.call(())));
    }
    pub fn set_state(&mut self, state: HomeState, frame: &Frame<'_>) {
        if state != self.current {
            match state {
                HomeState::Home => self.experimental.show(frame),
                HomeState::Update => self.update_alert.show(frame),
                HomeState::Alerts => self.offroad_alert.show(frame),
            }
            match self.current {
                HomeState::Home => {}
                HomeState::Update => self.update_alert.hide(frame),
                HomeState::Alerts => self.offroad_alert.hide(frame),
            }
        }
        self.current = state;
    }
    pub fn refresh(&mut self, frame: &Frame<'_>) -> Result<(), crate::Error> {
        let description = self.context.params.string("UpdaterCurrentDescription")?;
        self.version = if description.is_empty() {
            "openpilot".into()
        } else {
            format!("openpilot {description}")
        };
        let update = self.update_alert.refresh()? > 0;
        let count = self.offroad_alert.refresh()?;
        let alerts = count > 0;
        if !update && !alerts {
            self.set_state(HomeState::Home, frame);
        } else if update
            && (!self.previous_update || (!alerts && self.current == HomeState::Alerts))
        {
            self.set_state(HomeState::Update, frame);
        } else if alerts
            && (!self.previous_alerts || (!update && self.current == HomeState::Update))
        {
            self.set_state(HomeState::Alerts, frame);
        }
        self.update_available = update;
        self.alert_count = count;
        self.previous_update = update;
        self.previous_alerts = alerts;
        Ok(())
    }
    fn header(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        let mut width = self.header.width;
        if self.update_available {
            width -= self.update_notification.width;
            self.notification(
                draw,
                self.update_notification,
                &self.context.tr("UPDATE"),
                if self.current == HomeState::Update {
                    paint::color(75, 95, 255, 255)
                } else {
                    paint::color(54, 77, 239, 255)
                },
            )?;
        }
        if self.alert_count > 0 {
            width -= self.alert_notification.width;
            let count = i64::try_from(self.alert_count)
                .map_err(|_| Error::Contract("alert count overflow"))?;
            let text = self
                .context
                .trn("{} ALERT", "{} ALERTS", count)
                .replace("{}", &count.to_string());
            self.notification(
                draw,
                self.alert_notification,
                &text,
                if self.current == HomeState::Alerts {
                    paint::color(255, 70, 70, 255)
                } else {
                    paint::color(226, 44, 44, 255)
                },
            )?;
        }
        if self.update_available || self.alert_count > 0 {
            width -= 37.5;
        }
        let mut label = Label::new(&self.version, 48.0);
        label.color = WHITE;
        label.horizontal = Horizontal::Right;
        paint::label(
            draw,
            Rect {
                x: self.header.x + self.header.width - width,
                width,
                ..self.header
            },
            label,
        )
    }
    fn notification(
        &self,
        draw: &mut dyn Draw,
        rect: Rect,
        text: &str,
        color: u32,
    ) -> Result<(), Error> {
        draw.rounded_segments(rect, 0.3, 10, color, false)?;
        let size =
            openpilot_ui_framework::text_layout::measure(draw, Font::Medium, text, 40.0, 0.0);
        paint::text(
            draw,
            Point {
                x: float(
                    (f64::from(rect.x)
                        + ((f64::from(rect.width) - f64::from(size.x)) / 2.0).floor())
                    .trunc(),
                ),
                y: float(
                    (f64::from(rect.y)
                        + ((f64::from(rect.height) - f64::from(size.y)) / 2.0).floor())
                    .trunc(),
                ),
            },
            Text {
                value: text,
                font: Font::Medium,
                size: 40.0,
                spacing: 0.0,
                color: WHITE,
            },
        )
    }
}
impl Widget for Home {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.experimental.show(frame);
        self.last_refresh = (self.context.now_monotonic)();
        if let Err(e) = self.refresh(frame) {
            self.context
                .actions
                .push(crate::context::Action::Failure(e));
        }
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let rect = self.state.rect;
        self.header = Rect {
            x: rect.x + 40.0,
            y: rect.y + 40.0,
            width: rect.width - 80.0,
            height: 80.0,
        };
        self.content = Rect {
            x: rect.x + 40.0,
            y: rect.y + 145.0,
            width: rect.width - 80.0,
            height: rect.height - 185.0,
        };
        self.left = Rect {
            width: self.content.width - 775.0,
            ..self.content
        };
        self.right = Rect {
            x: self.content.x + self.left.width + 25.0,
            width: 750.0,
            ..self.content
        };
        self.update_notification.x = self.header.x;
        self.update_notification.y = self.header.y + 10.0;
        self.alert_notification.x = self.header.x + if self.update_available { 220.0 } else { 0.0 };
        self.alert_notification.y = self.header.y + 10.0;
        Ok(())
    }
    fn mouse_release(
        &mut self,
        position: Point,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.state.release(frame.now);
        if self.update_available && self.update_notification.contains(position) {
            self.set_state(HomeState::Update, frame);
        } else if self.alert_count > 0 && self.alert_notification.contains(position) {
            self.set_state(HomeState::Alerts, frame);
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let now = (self.context.now_monotonic)();
        if now - self.last_refresh >= 10.0 {
            self.refresh(frame)?;
            self.last_refresh = now;
        }
        self.header(draw)?;
        match self.current {
            HomeState::Home => {
                self.prime.set_rect(self.left);
                self.prime.render(frame, draw)?;
                self.experimental.set_rect(Rect {
                    height: 125.0,
                    ..self.right
                });
                self.experimental.render(frame, draw)?;
                self.setup.set_rect(Rect {
                    y: self.right.y + 150.0,
                    height: self.right.height - 150.0,
                    ..self.right
                });
                self.setup.render(frame, draw)?;
            }
            HomeState::Update => {
                self.update_alert.set_rect(self.content);
                self.update_alert.render(frame, draw)?;
            }
            HomeState::Alerts => {
                self.offroad_alert.set_rect(self.content);
                self.offroad_alert.render(frame, draw)?;
            }
        }
        if self.update_alert.dismiss.replace(false) || self.offroad_alert.dismiss.replace(false) {
            self.set_state(HomeState::Home, frame);
        }
        Ok(RenderResult::None)
    }
}
