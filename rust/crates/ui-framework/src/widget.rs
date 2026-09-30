use crate::{
    draw::Draw,
    geometry::{MouseEvent, Point, Rect},
    Error,
};

pub enum Property<T> {
    Value(T),
    Dynamic(Box<dyn Fn() -> T>),
}
impl<T: Clone> Property<T> {
    pub fn get(&self) -> T {
        match self {
            Self::Value(value) => value.clone(),
            Self::Dynamic(get) => get(),
        }
    }
}
impl<T> From<T> for Property<T> {
    fn from(value: T) -> Self {
        Self::Value(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogResult {
    Cancel,
    Confirm,
    NoAction,
}
impl DialogResult {
    pub const fn code(self) -> i32 {
        match self {
            Self::Cancel => 0,
            Self::Confirm => 1,
            Self::NoAction => -1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderResult {
    #[default]
    None,
    Bool(bool),
    Dialog(DialogResult),
    Value(i32),
}

pub struct Frame<'a> {
    pub now: f64,
    pub dt: f64,
    pub target_fps: f64,
    pub awake: bool,
    pub events: &'a [MouseEvent],
    pub last_event: MouseEvent,
    pub wheel: f64,
    pub show_touches: bool,
}

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
    pressed: [bool; 2],
    tracking: [bool; 2],
    release_time: Option<f64>,
    was_awake: bool,
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
    fn expire(&mut self, now: f64) {
        if self.release_time.is_some_and(|time| now >= time) {
            self.release_time = None;
        }
    }
}

pub trait Widget {
    fn state(&self) -> &WidgetState;
    fn state_mut(&mut self) -> &mut WidgetState;
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error>;
    fn update(&mut self, _frame: &Frame<'_>) {}
    fn layout(&mut self, _frame: &Frame<'_>, _draw: &mut dyn Draw) -> Result<(), Error> {
        Ok(())
    }
    fn layout_changed(&mut self) {}
    fn mouse_press(&mut self, _position: Point, _frame: &Frame<'_>) {}
    fn mouse_release(&mut self, _position: Point, frame: &Frame<'_>) {
        self.state_mut().release(frame.now);
    }
    fn mouse_event(&mut self, _event: MouseEvent, _frame: &Frame<'_>) {}
    fn show(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state_mut().children {
            child.show(frame);
        }
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state_mut().children {
            child.hide(frame);
        }
    }
    fn set_rect(&mut self, rect: Rect) {
        let old = self.state().rect;
        self.state_mut().rect = rect;
        if old.x != rect.x
            || old.y != rect.y
            || old.width != rect.width
            || old.height != rect.height
        {
            self.layout_changed();
        }
    }
    fn set_position(&mut self, x: f32, y: f32) {
        self.set_rect(Rect {
            x,
            y,
            ..self.state().rect
        });
    }
    fn render(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.update(frame);
        self.state_mut().expire(frame.now);
        if !self.state().visible.get() {
            return Ok(RenderResult::None);
        }
        self.layout(frame, draw)?;
        let result = self.paint(frame, draw)?;
        if frame.show_touches {
            draw.border(
                self.state().rect,
                0.0,
                u32::from_le_bytes([230, 41, 55, 255]),
            )?;
        }
        if self.state().enabled.get() && self.state().was_awake {
            self.process_mouse_events(frame);
        } else {
            let state = self.state_mut();
            state.pressed = [false; 2];
            state.tracking = [false; 2];
        }
        self.state_mut().was_awake = frame.awake;
        Ok(result)
    }
    fn process_mouse_events(&mut self, frame: &Frame<'_>) {
        let hit = self.state().hit_rect();
        let valid = self
            .state()
            .touch_valid
            .as_ref()
            .is_none_or(|valid| valid());
        for event in frame.events {
            let slot = usize::from(event.slot);
            if slot >= 2 || (!self.state().multi_touch && slot != 0) {
                continue;
            }
            let inside = hit.contains(event.pos);
            if event.pressed && valid {
                if inside {
                    self.mouse_press(event.pos, frame);
                    self.state_mut().pressed[slot] = true;
                    self.state_mut().tracking[slot] = true;
                    self.mouse_event(*event, frame);
                }
            } else if !valid {
                self.state_mut().pressed[slot] = false;
                self.state_mut().tracking[slot] = false;
            } else if event.released {
                self.mouse_event(*event, frame);
                if self.state().pressed[slot] && inside {
                    self.mouse_release(event.pos, frame);
                }
                self.state_mut().pressed[slot] = false;
                self.state_mut().tracking[slot] = false;
            } else if inside {
                if self.state().tracking[slot] {
                    self.state_mut().pressed[slot] = true;
                    self.mouse_event(*event, frame);
                }
            } else {
                self.state_mut().pressed[slot] = false;
                self.mouse_event(*event, frame);
            }
        }
    }
}

pub fn intersection(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let width = (a.x + a.width).min(b.x + b.width) - x;
    let height = (a.y + a.height).min(b.y + b.height) - y;
    if width <= 0.0 || height <= 0.0 {
        Rect::default()
    } else {
        Rect {
            x,
            y,
            width,
            height,
        }
    }
}
