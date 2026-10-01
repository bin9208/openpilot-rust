use super::*;
pub struct NavMotion {
    pub drag_start: Option<Point>,
    pub dragging_down: bool,
    pub playing_dismiss: bool,
    pub position: Bounce,
    pub bar_position: Filter,
    pub bar_alpha: Filter,
    pub bar_target: f64,
    pub bar_fade_time: f64,
    pub bar_show_time: f64,
    pub back_area: f64,
}
impl NavMotion {
    pub fn new(fps: f64) -> Self {
        Self {
            drag_start: None,
            dragging_down: false,
            playing_dismiss: false,
            position: Bounce::new(0.0, 0.1, fps, 1.0),
            bar_position: Filter::new(0.0, 0.1, fps),
            bar_alpha: Filter::new(1.0, 0.1, fps),
            bar_target: 1.0,
            bar_fade_time: 0.0,
            bar_show_time: 0.0,
            back_area: 0.65,
        }
    }
    pub fn show(&mut self, height: f64, now: f64) {
        self.drag_start = None;
        self.dragging_down = false;
        self.playing_dismiss = false;
        self.position.position.update_alpha(0.1);
        self.position.position.x = height;
        self.position.velocity.x = 0.0;
        self.bar_position.x = -14.0;
        self.bar_show_time = now;
        self.bar_target = 1.0;
        self.bar_alpha.x = 1.0;
        self.bar_fade_time = now;
    }
    pub fn dismiss(&mut self) {
        if !self.playing_dismiss {
            self.playing_dismiss = true;
            self.position.position.update_alpha(0.2);
        }
    }
    pub fn is_dismissing(&self) -> bool {
        self.dragging_down || self.playing_dismiss
    }
    pub fn event(&mut self, event: MouseEvent, height: f64, back_enabled: bool) {
        if self.playing_dismiss {
            return;
        }
        if event.pressed {
            self.position.position.update_alpha(0.04);
            if f64::from(event.pos.y) < height * self.back_area && back_enabled {
                self.drag_start = Some(event.pos);
            }
        } else if event.down {
            if let Some(start) = self.drag_start {
                let horizontal = (event.pos.x - start.x).abs() > 60.0;
                let upward = event.pos.y - start.y < -60.0;
                if !(horizontal || upward) {
                    if event.pos.y - start.y > 40.0 {
                        self.dragging_down = true;
                    }
                } else if !self.dragging_down {
                    self.drag_start = None;
                }
            }
        } else if event.released {
            self.position.position.update_alpha(0.1);
            if self
                .drag_start
                .is_some_and(|start| event.pos.y - start.y > 80.0)
            {
                self.playing_dismiss = true;
            }
            self.drag_start = None;
            self.dragging_down = false;
        }
    }
    /// Returns (rendered y, should pop, shown animation completed).
    pub fn update(
        &mut self,
        enabled: bool,
        height: f64,
        last: MouseEvent,
        now: f64,
    ) -> (f64, bool, bool) {
        let mut y = 0.0;
        let mut shown = false;
        if self.dragging_down {
            self.bar_target = 1.0;
            self.bar_fade_time = now;
        }
        if !enabled {
            self.drag_start = None;
        }
        if let Some(start) = self.drag_start {
            y = (f64::from(last.pos.y) - f64::from(start.y)).max(0.0);
            if y < 80.0 {
                y /= 2.0;
            }
        }
        if self.playing_dismiss {
            y = height + 64.0;
        }
        y = self.position.update(y);
        if y.abs() < 1.0 && self.position.velocity.x.abs() < 0.5 {
            y = 0.0;
            self.position.position.x = 0.0;
            self.position.velocity.x = 0.0;
            shown = true;
        }
        let pop = y > height + 54.0;
        if pop {
            self.playing_dismiss = false;
            self.drag_start = None;
            self.dragging_down = false;
        }
        (y, pop, shown)
    }
    pub fn bar(&mut self, now: f64) -> (f64, f64) {
        if self.drag_start.is_some() || self.playing_dismiss {
            self.bar_position.x = 6.0 + self.position.position.x;
        } else if now - self.bar_show_time < 0.4 {
            self.bar_position.x = -14.0;
        } else {
            self.bar_position.update(6.0);
        }
        if now - self.bar_fade_time > 2.0 {
            self.bar_target = 0.0;
        }
        let alpha = self.bar_alpha.update(self.bar_target);
        (self.bar_position.x, alpha)
    }
}
