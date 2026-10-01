use crate::{
    draw::Draw,
    geometry::{Point, Rect},
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use num_traits::ToPrimitive;
use openpilot_startup_ui::scroll::Scroll;
pub struct Scroller {
    pub state: WidgetState,
    pub items: Vec<Box<dyn Widget>>,
    pub panel: Scroll,
    pub spacing: f64,
    pub line_separator: bool,
    pub pad_end: bool,
}
impl Default for Scroller {
    fn default() -> Self {
        Self {
            state: WidgetState::default(),
            items: Vec::new(),
            panel: Scroll::default(),
            spacing: 40.0,
            line_separator: false,
            pad_end: true,
        }
    }
}
impl Scroller {
    pub fn add(&mut self, mut item: Box<dyn Widget>) {
        item.state_mut().touch_valid = None;
        self.items.push(item);
    }
    pub fn item_mut<T: Widget>(&mut self, index: usize) -> Option<&mut T> {
        self.items
            .get_mut(index)
            .and_then(|item| (item.as_mut() as &mut dyn std::any::Any).downcast_mut::<T>())
    }
}
impl Widget for Scroller {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let visible: Vec<_> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.state().visible.get())
            .map(|(index, _)| index)
            .collect();
        let mut sequence = Vec::new();
        for index in visible {
            if self.line_separator && !sequence.is_empty() {
                sequence.push(None);
            }
            sequence.push(Some(index));
        }
        let content: f64 = sequence
            .iter()
            .map(|index| {
                index.map_or(1.0, |index| {
                    f64::from(self.items[index].state().rect.height)
                })
            })
            .sum::<f64>()
            + self.spacing
                * sequence
                    .len()
                    .to_f64()
                    .ok_or(Error::Contract("TICI item count overflow"))?
            - if self.pad_end { 0.0 } else { self.spacing };
        self.panel
            .update(rect, float(content), frame.events, float(frame.wheel));
        let offset = self.panel.offset;
        draw.scissor(Some(rect))?;
        let mut height = 0.0;
        for (position, index) in sequence.into_iter().enumerate() {
            let spacing = if position == 0 { 0.0 } else { self.spacing };
            let y = f64::from(rect.y) + height + spacing + offset;
            if let Some(index) = index {
                let item = &mut self.items[index];
                height += f64::from(item.state().rect.height) + spacing;
                item.set_position(rect.x, float(y));
                item.set_parent_rect(rect);
                item.state_mut().interaction_gate = self.panel.touch_valid();
                item.render(frame, draw)?;
            } else {
                height += 1.0 + spacing;
                draw.line(
                    Point {
                        x: rect.x.trunc() + 40.0,
                        y: float(y).trunc(),
                    },
                    Point {
                        x: (rect.x + rect.width).trunc() - 40.0,
                        y: float(y).trunc(),
                    },
                    1.0,
                    u32::from_le_bytes([130, 130, 130, 255]),
                )?;
            }
        }
        draw.scissor(None)?;
        Ok(RenderResult::None)
    }
    fn show(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.show(frame);
        }
        self.panel.set_offset(0.0);
        for item in &mut self.items {
            item.show(frame);
        }
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.hide(frame);
        }
        for item in &mut self.items {
            item.hide(frame);
        }
    }
}
pub struct LineSeparator {
    pub state: WidgetState,
}
impl LineSeparator {
    pub fn new(height: f32) -> Self {
        let mut state = WidgetState::default();
        state.rect.height = height;
        Self { state }
    }
}
impl Widget for LineSeparator {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn set_parent_rect(&mut self, rect: Rect) {
        self.state.parent_rect = Some(rect);
        self.state.rect.width = rect.width;
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        draw.line(
            Point {
                x: rect.x.trunc() + 40.0,
                y: rect.y.trunc(),
            },
            Point {
                x: (rect.x + rect.width).trunc() - 40.0,
                y: rect.y.trunc(),
            },
            1.0,
            u32::from_le_bytes([130, 130, 130, 255]),
        )?;
        Ok(RenderResult::None)
    }
}
