use crate::{
    animation::{Bounce, Filter},
    draw::{Draw, BLACK},
    geometry::{MouseEvent, Point, Rect},
    text_layout::float,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetState},
    Error,
};
use num_traits::ToPrimitive;

mod motion;
pub use motion::NavMotion;
pub struct NavWidget {
    pub state: WidgetState,
    pub motion: NavMotion,
    pub content: Box<dyn Widget>,
    pub back_enabled: Box<dyn Fn() -> bool>,
    on_back: Option<crate::callback::Callback<()>>,
    pub on_shown: Option<Box<dyn FnOnce()>>,
    dismiss_callback: Option<Box<dyn FnOnce()>>,
    pop_requested: bool,
    window_height: f64,
}
impl NavWidget {
    pub fn new(content: Box<dyn Widget>, fps: f64, window_height: f64) -> Self {
        Self {
            state: WidgetState::default(),
            motion: NavMotion::new(fps),
            content,
            back_enabled: Box::new(|| true),
            on_back: None,
            on_shown: None,
            dismiss_callback: None,
            pop_requested: false,
            window_height,
        }
    }
    pub fn set_back_callback(&mut self, mut callback: Box<dyn FnMut()>) {
        self.on_back = Some(crate::callback::Callback::new(move |()| callback()));
    }
    pub fn dismiss(&mut self, callback: Option<Box<dyn FnOnce()>>) {
        if !self.motion.playing_dismiss {
            self.motion.dismiss();
            self.dismiss_callback = callback;
        }
    }
}
impl Widget for NavWidget {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.show(frame);
        }
        self.content.show(frame);
        self.motion.show(self.window_height, frame.now);
        self.dismiss_callback = None;
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        for child in &mut self.state.children {
            child.hide(frame);
        }
        self.content.hide(frame);
    }
    fn update(&mut self, frame: &Frame<'_>, _draw: &mut dyn Draw) -> Result<(), Error> {
        let (y, pop, shown) = self.motion.update(
            self.state.enabled.get(),
            f64::from(self.state.rect.height),
            frame.last_event,
            frame.now,
        );
        if shown {
            if let Some(callback) = self.on_shown.take() {
                callback();
            }
        }
        self.pop_requested |= pop;
        self.set_position(self.state.rect.x, float(y));

        Ok(())
    }
    fn mouse_event(
        &mut self,
        event: MouseEvent,
        _: &Frame<'_>,
        _draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.motion.event(
            event,
            f64::from(self.state.rect.height),
            (self.back_enabled)() && self.content.back_enabled(),
        );
        Ok(())
    }
    fn layout(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<(), Error> {
        let rect = self.state.rect;
        let alpha = if rect.height > 0.0 {
            (200.0 * (1.0 - f64::from(rect.y) / f64::from(rect.height)).clamp(0.0, 1.0))
                .to_u8()
                .ok_or(Error::Contract("navigation alpha out of range"))?
        } else {
            0
        };
        draw.rounded(
            Rect {
                x: 0.0,
                y: 0.0,
                width: rect.width,
                height: rect.height,
            },
            0.0,
            u32::from_le_bytes([0, 0, 0, alpha]),
        )?;
        draw.rounded(
            Rect {
                height: rect.height + 20.0,
                ..rect
            },
            0.0,
            BLACK,
        )
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.content.set_rect(self.state.rect);
        self.content.state_mut().enabled =
            (self.state.enabled.get() && !self.motion.is_dismissing()).into();
        self.content.render(frame, draw)
    }
    fn render(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let result = crate::widget::render_widget(self, frame, draw)?;
        let (y, alpha) = self.motion.bar(frame.now);
        let rect = Rect {
            x: self.state.rect.x + (self.state.rect.width - 205.0) / 2.0,
            y: float(y),
            width: 205.0,
            height: 8.0,
        };
        let white = (255.0 * 0.9 * alpha)
            .to_u8()
            .ok_or(Error::Contract("navigation bar alpha out of range"))?;
        let black = (255.0 * 0.3 * alpha)
            .to_u8()
            .ok_or(Error::Contract("navigation bar alpha out of range"))?;
        draw.rounded_segments(
            rect,
            1.0,
            6,
            u32::from_le_bytes([255, 255, 255, white]),
            false,
        )?;
        draw.rounded_segments(rect, 1.0, 6, u32::from_le_bytes([0, 0, 0, black]), true)?;
        Ok(result)
    }
    fn dismiss_navigation(&mut self, callback: Option<Box<dyn FnOnce()>>, _: &Frame<'_>) {
        self.dismiss(callback);
    }
    fn take_navigation(&mut self) -> Option<NavigationRequest> {
        if !std::mem::take(&mut self.pop_requested) {
            return None;
        }
        if let Some(callback) = self.dismiss_callback.take() {
            Some(NavigationRequest::Pop(Some(callback)))
        } else {
            let callback = self
                .on_back
                .clone()
                .map(|callback| Box::new(move || callback.call(())) as Box<dyn FnOnce()>);
            Some(NavigationRequest::Pop(callback))
        }
    }
}
