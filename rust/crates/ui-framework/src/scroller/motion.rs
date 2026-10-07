use super::*;
impl Scroller {
    pub(super) fn animate(&mut self, id: u64, mut x: f64, mut y: f64) -> (f64, f64) {
        let start = self.pending_lift.is_empty();
        if let Some(lift) = self.lifts.get_mut(&id) {
            let remove = if !self.pending_move.is_empty() {
                lift.update(20.0);
                if (lift.x - 20.0).abs() < 2.0 {
                    self.pending_lift.remove(&id);
                }
                false
            } else {
                lift.update(0.0);
                lift.x.abs() < 1.0
            };
            y -= lift.x;
            if remove {
                self.lifts.remove(&id);
            }
        }
        if let Some(movement) = self.moves.get_mut(&id) {
            let content = x - self.scroll_offset;
            let mut remove = false;
            if start {
                movement.update(content);
                let delta = (movement.x - content).abs();
                if delta < 10.0 {
                    self.pending_move.remove(&id);
                }
                remove = delta < 1.0;
            }
            x = movement.x + self.scroll_offset;
            if remove {
                self.moves.remove(&id);
            }
        }
        (x, y)
    }
    pub(super) fn get_scroll(&mut self, frame: &Frame<'_>) -> f64 {
        self.panel.enabled = (self.scrolling_enabled.get()
            && self.state.enabled.get()
            && !self
                .scrolling_to
                .is_some_and(|target| target.block_interrupt))
        .into();
        self.panel
            .update(self.state.rect, self.content_size, frame.events, frame.dt);
        if !self.snap_items {
            return self.panel.offset();
        }
        let rect = self.state.rect;
        let center = if self.horizontal {
            rect.x + rect.width / 2.0
        } else {
            rect.y + rect.height / 2.0
        };
        let mut closest = None;
        let mut delta = f64::INFINITY;
        for index in &self.visible {
            let rect = self.items[*index].widget.state().rect;
            let value = f64::from(if self.horizontal {
                rect.x + rect.width / 2.0
            } else {
                rect.y + rect.height / 2.0
            }) - f64::from(center);
            if value.abs() < delta.abs() {
                closest = Some(*index);
                delta = value;
            }
        }
        if closest.is_some() {
            if self.state.is_pressed() {
                self.snap_filter.x = 0.0;
            } else {
                let size = f64::from(if self.horizontal {
                    rect.width
                } else {
                    rect.height
                });
                let snap = (-delta / 10.0)
                    .min(-self.panel.offset() / 10.0)
                    .max((size - self.panel.offset() - self.content_size) / 10.0);
                self.snap_filter.update(snap);
            }
            self.panel
                .set_offset(self.panel.offset() + self.snap_filter.x);
        }
        self.panel.offset()
    }
    pub(super) fn paint_item(
        &mut self,
        index: usize,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        let rect = self.items[index].widget.state().rect;
        let viewport = self.state.rect;
        if rect.x >= viewport.x + viewport.width
            || rect.x + rect.width <= viewport.x
            || rect.y >= viewport.y + viewport.height
            || rect.y + rect.height <= viewport.y
        {
            return Ok(());
        }
        let valid = self.panel.touch_valid()
            && self.state.enabled.get()
            && !self
                .scrolling_to
                .is_some_and(|target| target.block_widget_interaction)
            && !self.moving_items();
        self.items[index].widget.state_mut().interaction_gate = valid;
        self.items[index].widget.render(frame, draw)?;
        if let Some(mut callback) = self.after_item.take() {
            let result = callback(self, frame);
            self.after_item = Some(callback);
            result?;
        }
        Ok(())
    }
}
