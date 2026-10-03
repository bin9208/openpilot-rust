//! Native MiciOffroadAlerts from selfdrive/ui/mici/layouts/offroad_alerts.py (MIT).
mod data;
mod item;
use crate::context::{actions::UpdaterAction, Action, Context};
pub use data::AlertData;
pub use item::{split_text, AlertItem, AlertSize};
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::{Draw, WHITE},
    scroller::Scroller,
    text::Font,
    text_layout::{Horizontal, Vertical},
    unified_label::UnifiedLabel,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};

pub struct OffroadAlerts {
    pub state: WidgetState,
    pub scroller: Scroller,
    pub sorted_alerts: Vec<AlertData>,
    context: Context,
    last_refresh: f64,
    empty: UnifiedLabel,
}
impl OffroadAlerts {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, crate::Error> {
        let mut scroller = Scroller::new(false, false, !context.pc, 20.0);
        scroller.spacing = 12.0;
        scroller.padding = 0.0;
        let sorted_alerts = data::catalog(&context)?;
        for data in &sorted_alerts {
            let mut item = AlertItem::new(data.clone(), canvas)?;
            if data.key == "UpdateAvailable" {
                let actions = context.actions.clone();
                item.state.click = Some(Box::new(move || {
                    actions.push(Action::Updater(UpdaterAction::Reboot))
                }));
            }
            scroller.add(Box::new(item))?;
        }
        let mut empty = UnifiedLabel::new(context.tr("no alerts"));
        empty.font = Font::Display;
        empty.size = 65.0;
        empty.color = WHITE;
        empty.horizontal = Horizontal::Center;
        empty.vertical = Vertical::Middle;
        Ok(Self {
            state: WidgetState::default(),
            scroller,
            sorted_alerts,
            context,
            last_refresh: 0.0,
            empty,
        })
    }
    pub fn active_alerts(&self) -> usize {
        self.sorted_alerts
            .iter()
            .filter(|alert| alert.visible)
            .count()
    }
    pub fn scrolling(&self) -> bool {
        self.scroller.panel.touch_valid()
    }
    pub fn refresh(&mut self) -> Result<usize, crate::Error> {
        data::refresh(&self.context, &mut self.sorted_alerts)?;
        for (index, data) in self.sorted_alerts.iter().enumerate() {
            let widget = self
                .scroller
                .item_mut(index)
                .ok_or(Error::Contract("alert item missing"))?;
            let item = (widget as &mut dyn std::any::Any)
                .downcast_mut::<AlertItem>()
                .ok_or(Error::Contract("alert item type"))?;
            item.update_alert_data(data.clone());
        }
        Ok(self.active_alerts())
    }
}
impl Widget for OffroadAlerts {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.scroller.show(frame);
        self.last_refresh = (self.context.now_monotonic)();
        if let Err(error) = self.refresh() {
            self.context.actions.push(Action::Failure(error))
        }
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.scroller.hide(frame)
    }
    fn update(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<(), Error> {
        let now = (self.context.now_monotonic)();
        if now - self.last_refresh >= 5.0 {
            self.refresh()?;
            self.last_refresh = now;
        }
        for index in 0..self.scroller.len() {
            let widget = self
                .scroller
                .item_mut(index)
                .ok_or(Error::Contract("alert item missing"))?;
            let item = (widget as &mut dyn std::any::Any)
                .downcast_mut::<AlertItem>()
                .ok_or(Error::Contract("alert item type"))?;
            item.prepare_layout(draw);
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if self.active_alerts() == 0 {
            self.empty.set_rect(self.state.rect);
            self.empty.render(frame, draw)
        } else {
            self.scroller.state.enabled = self.state.enabled.get().into();
            self.scroller.set_rect(self.state.rect);
            self.scroller.render(frame, draw)
        }
    }
}
