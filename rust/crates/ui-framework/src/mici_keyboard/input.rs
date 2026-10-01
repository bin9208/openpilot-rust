use super::{CapsState, Layer, MiciKeyboard};
use crate::geometry::{MouseEvent, Point};
impl MiciKeyboard {
    pub fn handle_event(&mut self, event: MouseEvent, now: f64) {
        let keyboard_y = self.state.rect.y + self.state.rect.height - self.background.height;
        if event.pressed {
            self.touch_started = event.pos.y > keyboard_y;
            self.release_started = false;
            self.dragging = self.touch_started;
            self.closest = None;
            self.selected_at = None;
            self.unselect_at = None;
        } else if event.released {
            self.release_started = self.touch_started;
            self.touch_started = false;
            self.dragging = false;
            if event.pos.y <= keyboard_y {
                self.closest = None;
            }
        }
        if event.down && self.dragging {
            self.closest = self.closest_key(event.pos);
            if self.selected_at.is_none() {
                self.selected_at = Some(now);
            }
            if event.pos.y <= keyboard_y {
                self.closest = None;
            }
        }
    }
    fn closest_key(&self, position: Point) -> Option<(usize, f64)> {
        let mut closest = None;
        let mut best = f64::INFINITY;
        for id in self.rows[self.layer.index()].iter().flatten().copied() {
            let key = &self.keys[id];
            let distance = (f64::from(key.original.x) - f64::from(position.x)).abs()
                + (f64::from(key.original.y)
                    - (f64::from(position.y) - f64::from(self.state.rect.y)))
                .abs();
            if distance < best
                && self.closest.is_none_or(|(previous, old_distance)| {
                    id == previous || distance < old_distance - 5.0
                })
            {
                closest = Some((id, distance));
                best = distance;
            }
        }
        closest
    }
    pub fn release(&mut self, now: f64) {
        if !self.release_started {
            self.closest = None;
            self.selected_at = None;
            self.unselect_at = None;
            self.dragging = false;
            self.touch_started = false;
            return;
        }
        if let Some((id, _)) = self.closest {
            if id == self.caps_key {
                self.uppercase(true);
            } else if self.number_keys.contains(&id) {
                self.set_layer(Layer::Special);
            } else if id == self.abc_key {
                self.uppercase(false);
            } else if id == self.special_key {
                self.set_layer(Layer::SuperSpecial);
            } else {
                self.text.push_str(&self.keys[id].value);
                if self.caps == CapsState::Upper {
                    self.uppercase(false);
                }
                if self.auto_return.contains(&self.keys[id].value)
                    && matches!(self.layer, Layer::Special | Layer::SuperSpecial)
                {
                    self.uppercase(false);
                }
            }
        }
        self.unselect_at = Some(if now - self.selected_at.unwrap_or(0.0) < 0.075 {
            now + 0.075
        } else {
            now
        });
        self.release_started = false;
        self.dragging = false;
        self.touch_started = false;
    }
}
