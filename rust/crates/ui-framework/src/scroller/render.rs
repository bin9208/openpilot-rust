use super::*;
impl Widget for Scroller {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn back_enabled(&self) -> bool {
        self.horizontal || self.panel.offset() >= -20.0
    }
    fn update(&mut self, _: &Frame<'_>, _draw: &mut dyn Draw) -> Result<(), Error> {
        if matches!(self.panel.state, ScrollState::Pressed | ScrollState::Manual)
            && self
                .scrolling_to
                .is_some_and(|target| !target.block_interrupt)
        {
            self.scrolling_to = None;
        }
        if let Some(target) = self.scrolling_to.filter(|_| self.pending_lift.is_empty()) {
            self.scroll_filter.update(target.offset);
            self.panel.set_offset(self.scroll_filter.x);
            if (self.scroll_filter.x - target.offset).abs() < 1.0 {
                self.panel.set_offset(target.offset);
                self.scrolling_to = None;
            }
        }

        Ok(())
    }
    fn layout(&mut self, frame: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.visible = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.widget.state().visible.get())
            .map(|(index, _)| index)
            .collect();
        self.content_size = self
            .visible
            .iter()
            .map(|index| {
                let rect = self.items[*index].widget.state().rect;
                f64::from(if self.horizontal {
                    rect.width
                } else {
                    rect.height
                })
            })
            .sum::<f64>()
            + self.spacing
                * (self
                    .visible
                    .len()
                    .to_f64()
                    .ok_or(Error::Contract("scroller length overflow"))?
                    - 1.0)
            + self.padding * 2.0;
        self.scroll_offset = self.get_scroll(frame);
        self.item_filter.update(self.scroll_offset);
        let mut position = 0.0;
        let rect = self.state.rect;
        for index in 0..self.visible.len() {
            let item_index = self.visible[index];
            let child = self.items[item_index].widget.state().rect;
            let spacing = if index > 0 {
                self.spacing
            } else {
                self.padding
            };
            let (x, y) = if self.horizontal {
                let x = f64::from(rect.x) + position + spacing + self.scroll_offset;
                let y =
                    f64::from(rect.y) + (f64::from(rect.height) - f64::from(child.height)) / 2.0;
                position += f64::from(child.width) + spacing;
                (x, y)
            } else {
                let x = f64::from(rect.x) + (f64::from(rect.width) - f64::from(child.width)) / 2.0;
                let y = f64::from(rect.y) + position + spacing + self.scroll_offset;
                position += f64::from(child.height) + spacing;
                (x, y)
            };
            let (x, y) = self.animate(self.items[item_index].id, x, y);
            self.items[item_index]
                .widget
                .set_position(float(x), float(y));
            self.items[item_index].widget.set_parent_rect(rect);
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        draw.scissor(Some(rect))?;
        for position in (0..self.visible.len()).rev() {
            let index = self.visible[position];
            if !self.lifts.contains_key(&self.items[index].id) {
                self.paint_item(index, frame, draw)?;
            }
        }
        self.overlay.update(if self.pending_move.is_empty() {
            0.0
        } else {
            0.65
        });
        if self.overlay.x > 0.01 {
            let alpha = (255.0 * self.overlay.x)
                .to_u8()
                .ok_or(Error::Contract("move overlay alpha out of range"))?;
            draw.rounded(rect, 0.0, u32::from_le_bytes([0, 0, 0, alpha]))?;
        }
        let ids: Vec<_> = self.lifts.keys().copied().collect();
        for id in ids {
            if let Some(index) = self.items.iter().position(|item| item.id == id) {
                self.paint_item(index, frame, draw)?;
            }
        }
        draw.scissor(None)?;
        if self.edge_shadows && self.horizontal {
            let dark = u32::from_le_bytes([0, 0, 0, 204]);
            draw.gradient(
                Rect {
                    x: rect.x.trunc(),
                    y: rect.y.trunc(),
                    width: 20.0,
                    height: rect.height.trunc(),
                },
                [dark, dark, 0, 0],
            )?;
            draw.gradient(
                Rect {
                    x: (rect.x + rect.width - 20.0).trunc(),
                    y: rect.y.trunc(),
                    width: 20.0,
                    height: rect.height.trunc(),
                },
                [0, 0, dark, dark],
            )?;
        }
        if self.horizontal && !self.visible.is_empty() {
            if let Some(indicator) = self.indicator {
                draw_indicator(draw, indicator, self.scroll_offset, self.content_size, rect)?;
            }
        }
        Ok(RenderResult::None)
    }
    fn show(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.show(frame);
        }
        for item in &mut self.items {
            item.widget.show(frame);
        }
        if self.reset_on_show {
            self.panel.set_offset(0.0);
        }
        self.overlay.x = 0.0;
        self.moves.clear();
        self.lifts.clear();
        self.pending_lift.clear();
        self.pending_move.clear();
        self.scrolling_to = None;
        self.scroll_filter.x = 0.0;
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.hide(frame);
        }
        for item in &mut self.items {
            item.widget.hide(frame);
        }
    }
}
