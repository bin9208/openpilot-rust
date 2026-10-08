use super::{assets::Assets, card::Card, model::Model, scanning::Scanning};
use crate::context::{Action, Context};
use openpilot_ui_framework::{
    application::Tick,
    canvas::Canvas,
    draw::Draw,
    navigation::NavWidget,
    network::WifiSession,
    scroller::Scroller,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use openpilot_wifi::Command;
use std::rc::Rc;
pub struct Wifi {
    state: WidgetState,
    pub scroller: Scroller,
    pub(super) model: Rc<Model>,
    assets: Rc<Assets>,
    scanning_id: u64,
    revision: Option<u64>,
    pub tick: Tick,
}
impl Wifi {
    pub fn new(context: Context, session: WifiSession, canvas: &mut Canvas) -> Result<Self, Error> {
        Self::with_assets(context, session, Rc::new(Assets::new(canvas)?))
    }
    pub(super) fn with_assets(
        context: Context,
        session: WifiSession,
        assets: Rc<Assets>,
    ) -> Result<Self, Error> {
        let model = Model::new(context.clone(), session.clone());
        let mut scroller = Scroller::new(true, false, !context.pc, 20.0);
        scroller.indicator = Some(assets.get(
            "icons_mici/settings/horizontal_scroll_indicator.png",
            (96, 48),
        )?);
        let scanning_id = scroller.add(Box::new(Scanning::new(&assets)?))?;
        let callback_model = model.clone();
        scroller.after_item = Some(Box::new(move |scroller, _| {
            apply_moves(scroller, &callback_model)
        }));
        let callback_model = model.clone();
        let tick = Tick::new(move || {
            if let Err(error) = session.process() {
                context.actions.push(Action::Failure(error.into()));
            }
            callback_model.process_callbacks();
        });
        Ok(Self {
            state: WidgetState::default(),
            scroller,
            model,
            assets,
            scanning_id,
            tick,
            revision: None,
        })
    }
    pub fn navigation(self) -> NavWidget {
        NavWidget::new(Box::new(self), 20.0, 240.0)
    }
    pub fn any_network_forgetting(&self) -> bool {
        self.model
            .cards
            .borrow()
            .iter()
            .any(|card| card.borrow().forgetting)
    }
    fn sync_cards(&mut self) -> Result<(), Error> {
        let revision = self.model.revision.get();
        if self.revision == Some(revision) && !self.model.re_sort.get() {
            return Ok(());
        }
        let cards = self.model.cards.borrow().clone();
        for card in &cards {
            let ssid = card.borrow().network.ssid.clone();
            if find_card(&self.scroller, &ssid).is_none() {
                self.scroller.add(Box::new(Card::new(
                    self.model.clone(),
                    card.clone(),
                    &self.assets,
                )?))?;
            }
        }
        let mut ids = Vec::new();
        if self.model.re_sort.replace(false) {
            for card in &cards {
                let index = find_card(&self.scroller, &card.borrow().network.ssid)
                    .ok_or(Error::Contract("network card missing"))?;
                ids.push(
                    self.scroller
                        .item_id(index)
                        .ok_or(Error::Contract("network card id missing"))?,
                );
            }
        } else {
            for index in 0..self.scroller.len() {
                let id = self
                    .scroller
                    .item_id(index)
                    .ok_or(Error::Contract("network item id missing"))?;
                if id != self.scanning_id {
                    ids.push(id);
                }
            }
        }
        ids.push(self.scanning_id);
        self.scroller.reorder_items(&ids)?;
        update_order(&self.scroller, &self.model);
        self.revision = Some(revision);
        Ok(())
    }
}
fn find_card(scroller: &Scroller, ssid: &str) -> Option<usize> {
    (0..scroller.len()).find(|index| {
        scroller
            .item(*index)
            .and_then(|item| (item as &dyn std::any::Any).downcast_ref::<Card>())
            .is_some_and(|card| card.shared.borrow().network.ssid == ssid)
    })
}
fn update_order(scroller: &Scroller, model: &Model) {
    *model.order.borrow_mut() = (0..scroller.len())
        .filter_map(|index| {
            scroller
                .item(index)
                .and_then(|item| (item as &dyn std::any::Any).downcast_ref::<Card>())
                .map(|card| card.shared.borrow().network.ssid.clone())
        })
        .collect();
}
fn apply_moves(scroller: &mut Scroller, model: &Model) -> Result<(), Error> {
    let mut changed = false;
    loop {
        let movement = model.moves.borrow_mut().pop_front();
        let Some(movement) = movement else { break };
        if let Some(index) = find_card(scroller, &movement.ssid).filter(|index| *index > 0) {
            scroller.move_item(index, 0)?;
            changed = true;
            if movement.scroll {
                scroller.scroll_to(scroller.panel.offset(), true, false, false)?;
            }
        }
    }
    model.pending_move.set(false);
    if changed {
        update_order(scroller, model);
    }
    Ok(())
}
impl Widget for Wifi {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.scroller.show(frame);
        self.model
            .report(self.model.session.send(Command::SetActive(true)));
        self.model.show();
        let result = self.sync_cards();
        self.model.report(result);
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.scroller.hide(frame);
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.sync_cards()?;
        if let Some(ssid) = self.model.session.snapshot().wifi_state.ssid {
            self.model.request_move(&ssid, false);
        }
        apply_moves(&mut self.scroller, &self.model)
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller.state.enabled = self.state.enabled.get().into();
        self.scroller.set_rect(self.state.rect);
        self.scroller.render(frame, draw)
    }
}
