use super::*;
pub struct WidgetState {
    pub rect: Rect,
    pub parent_rect: Option<Rect>,
    pub children: Vec<Box<dyn Widget>>,
    pub enabled: Property<bool>,
    pub visible: Property<bool>,
    pub touch_valid: Option<Box<dyn Fn() -> bool>>,
    pub click: Option<Box<dyn FnMut()>>,
    pub click_delay: Option<f64>,
    pub multi_touch: bool,
    pub interaction_gate: bool,
    pub(super) pressed: [bool; 2],
    pub(super) tracking: [bool; 2],
    pub(super) release_time: Option<f64>,
    pub(super) was_awake: bool,
}
impl Default for WidgetState {
    fn default() -> Self {
        Self {
            rect: Rect::default(),
            parent_rect: None,
            children: Vec::new(),
            enabled: true.into(),
            visible: true.into(),
            touch_valid: None,
            click: None,
            click_delay: None,
            multi_touch: false,
            interaction_gate: true,
            pressed: [false; 2],
            tracking: [false; 2],
            release_time: None,
            was_awake: true,
        }
    }
}
impl WidgetState {
    pub fn is_pressed(&self) -> bool {
        self.pressed.iter().any(|value| *value) || self.release_time.is_some()
    }
    pub fn hit_rect(&self) -> Rect {
        self.parent_rect
            .map_or(self.rect, |parent| intersection(self.rect, parent))
    }
    pub fn add_child(&mut self, child: Box<dyn Widget>) {
        self.children.push(child);
    }
    pub fn release(&mut self, now: f64) {
        if let Some(delay) = self.click_delay {
            self.release_time = Some(now + delay);
        }
        if let Some(click) = &mut self.click {
            click();
        }
    }
    pub(super) fn expire(&mut self, now: f64) {
        if self.release_time.is_some_and(|time| now >= time) {
            self.release_time = None;
        }
    }
}
