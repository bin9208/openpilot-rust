use crate::{
    animation::{Bounce, Filter},
    assets::Texture,
    draw::{Draw, ImageDraw},
    geometry::{Point, Rect},
    scroll::{ScrollPanel, ScrollState},
    text_layout::float,
    widget::{Frame, Property, RenderResult, Widget, WidgetState},
    Error,
};
use num_traits::ToPrimitive;
use std::collections::{BTreeMap, BTreeSet};
pub type AfterItem = Box<dyn FnMut(&mut Scroller, &Frame<'_>) -> Result<(), Error>>;
struct Item {
    id: u64,
    widget: Box<dyn Widget>,
}
#[derive(Clone, Copy)]
struct ScrollTo {
    offset: f64,
    block_interrupt: bool,
    block_widget_interaction: bool,
}
pub struct Scroller {
    pub state: WidgetState,
    pub panel: ScrollPanel,
    pub horizontal: bool,
    pub snap_items: bool,
    pub spacing: f64,
    pub padding: f64,
    pub scrolling_enabled: Property<bool>,
    pub reset_on_show: bool,
    pub indicator: Option<Texture>,
    pub edge_shadows: bool,
    pub content_size: f64,
    pub after_item: Option<AfterItem>,
    pub scroll_offset: f64,
    items: Vec<Item>,
    next_id: u64,
    visible: Vec<usize>,
    scrolling_to: Option<ScrollTo>,
    scroll_filter: Filter,
    snap_filter: Filter,
    item_filter: Bounce,
    overlay: Filter,
    moves: BTreeMap<u64, Filter>,
    lifts: BTreeMap<u64, Filter>,
    pending_lift: BTreeSet<u64>,
    pending_move: BTreeSet<u64>,
    fps: f64,
}
impl Scroller {
    pub fn new(horizontal: bool, snap_items: bool, tici: bool, fps: f64) -> Self {
        Self {
            state: WidgetState::default(),
            panel: ScrollPanel::new(horizontal, !snap_items, tici),
            horizontal,
            snap_items,
            spacing: 20.0,
            padding: 20.0,
            scrolling_enabled: true.into(),
            reset_on_show: true,
            indicator: None,
            edge_shadows: horizontal,
            content_size: 0.0,
            after_item: None,
            scroll_offset: 0.0,
            items: Vec::new(),
            next_id: 0,
            visible: Vec::new(),
            scrolling_to: None,
            scroll_filter: Filter::new(0.0, 0.15, fps),
            snap_filter: Filter::new(0.0, 0.05, fps),
            item_filter: Bounce::new(0.0, 0.05, fps, 1.0),
            overlay: Filter::new(0.0, 0.05, fps),
            moves: BTreeMap::new(),
            lifts: BTreeMap::new(),
            pending_lift: BTreeSet::new(),
            pending_move: BTreeSet::new(),
            fps,
        }
    }
    pub fn add(&mut self, widget: Box<dyn Widget>) -> Result<u64, Error> {
        let id = self.next_id;
        self.next_id = id
            .checked_add(1)
            .ok_or(Error::Contract("scroller item id overflow"))?;
        self.items.push(Item { id, widget });
        Ok(id)
    }
    pub fn reorder_items(&mut self, ids: &[u64]) -> Result<(), Error> {
        let mut seen = BTreeSet::new();
        if ids
            .iter()
            .any(|id| !seen.insert(*id) || !self.items.iter().any(|item| item.id == *id))
        {
            return Err(Error::Contract(
                "scroller reorder requires unique existing ids",
            ));
        }
        let mut items: BTreeMap<_, _> = std::mem::take(&mut self.items)
            .into_iter()
            .map(|item| (item.id, item))
            .collect();
        for id in ids {
            self.items.push(
                items
                    .remove(id)
                    .ok_or(Error::Contract("scroller reorder id vanished"))?,
            );
        }
        Ok(())
    }
    pub fn item_id(&self, index: usize) -> Option<u64> {
        self.items.get(index).map(|item| item.id)
    }
    pub fn item(&self, index: usize) -> Option<&dyn Widget> {
        self.items.get(index).map(|item| item.widget.as_ref())
    }
    pub fn item_mut(&mut self, index: usize) -> Option<&mut (dyn Widget + 'static)> {
        match self.items.get_mut(index) {
            Some(item) => Some(item.widget.as_mut()),
            None => None,
        }
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn auto_scrolling(&self) -> bool {
        self.scrolling_to.is_some()
    }
    pub fn moving_items(&self) -> bool {
        !self.moves.is_empty() || !self.lifts.is_empty()
    }
    pub fn scroll_to(
        &mut self,
        position: f64,
        smooth: bool,
        block_interrupt: bool,
        block_widget_interaction: bool,
    ) -> Result<(), Error> {
        if !smooth && (block_interrupt || block_widget_interaction) {
            return Err(Error::Contract("instant scroll cannot block interaction"));
        }
        if position.abs() < 1.0 {
            return Ok(());
        }
        let offset = self.panel.offset() - position;
        if smooth {
            self.scroll_filter.x = self.panel.offset();
            self.scrolling_to = Some(ScrollTo {
                offset,
                block_interrupt,
                block_widget_interaction,
            });
        } else {
            self.panel.set_offset(offset);
        }
        Ok(())
    }
    pub fn move_item(&mut self, from: usize, to: usize) -> Result<(), Error> {
        if !self.horizontal {
            return Err(Error::Contract(
                "move animation requires horizontal scroller",
            ));
        }
        if from == to {
            return Ok(());
        }
        if self.moving_items() {
            eprintln!("Already moving items, cannot move from {from} to {to}");
            return Ok(());
        }
        if from >= self.items.len() || to >= self.items.len() {
            return Err(Error::Contract("scroller move index out of bounds"));
        }
        let item = self.items.remove(from);
        let id = item.id;
        self.items.insert(to, item);
        for item in &self.items[from.min(to)..=from.max(to)] {
            self.moves.insert(
                item.id,
                Filter::new(
                    f64::from(item.widget.state().rect.x) - self.scroll_offset,
                    0.15,
                    self.fps,
                ),
            );
            self.pending_move.insert(item.id);
        }
        self.lifts.insert(id, Filter::new(0.0, 0.15, self.fps));
        self.pending_lift.insert(id);
        Ok(())
    }
}
mod indicator;
mod motion;
mod render;
pub use indicator::draw_indicator;
