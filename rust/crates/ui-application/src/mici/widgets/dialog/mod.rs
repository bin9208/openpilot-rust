//! Compact product dialogs from selfdrive/ui/mici/widgets/dialog.py.
use super::big_button::BigButton;
use crate::paint;
use openpilot_ui_framework::{
    assets::Texture,
    canvas::Canvas,
    draw::Draw,
    geometry::Rect,
    navigation::NavWidget,
    slider::{Slider, SliderAssets},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::Cell, rc::Rc};
mod input;
pub use input::{InputDialog, InputOptions};

pub fn information(
    canvas: &mut Canvas,
    title: &str,
    description: &str,
) -> Result<NavWidget, Error> {
    let mut card = BigButton::new(title, |path, size| paint::texture(canvas, path, size))?;
    card.set_grey();
    card.value = description.into();
    Ok(navigation(Box::new(Information {
        state: WidgetState::default(),
        card,
    })))
}
pub struct Confirmation {
    pub title: String,
    pub icon: Texture,
    pub callback: Rc<dyn Fn()>,
    pub exit_on_confirm: bool,
    pub red: bool,
}
pub fn confirmation(canvas: &mut Canvas, options: Confirmation) -> Result<NavWidget, Error> {
    let assets = SliderAssets::big(canvas, options.icon, options.red)?;
    let mut slider = Slider::new(options.title, assets, 20.0, true);
    let confirmed = Rc::new(Cell::new(false));
    let flag = confirmed.clone();
    slider.on_confirm = Some(Box::new(move || flag.set(true)));
    let mut nav = navigation(Box::new(slider));
    nav.on_update = Some(Box::new(|nav, frame| {
        if nav.motion.is_dismissing() {
            if let Some(slider) =
                (nav.content.as_mut() as &mut dyn std::any::Any).downcast_mut::<Slider>()
            {
                if !slider.confirmed() {
                    slider.reset(frame.now);
                }
            }
        }
    }));
    nav.after_content = Some(Box::new(move |nav, _| {
        if confirmed.replace(false) {
            let callback = options.callback.clone();
            if options.exit_on_confirm {
                nav.dismiss(Some(Box::new(move || callback())));
            } else {
                callback();
            }
        }
    }));
    Ok(nav)
}
fn navigation(content: Box<dyn Widget>) -> NavWidget {
    let mut nav = NavWidget::new(content, 20.0, 240.0);
    nav.state.rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 536.0,
        height: 240.0,
    };
    nav
}
struct Information {
    state: WidgetState,
    card: BigButton,
}
impl Widget for Information {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        self.card.set_position(
            rect.x + (rect.width - self.card.state.rect.width) / 2.0,
            rect.y + (rect.height - self.card.state.rect.height) / 2.0,
        );
        self.card.render(frame, draw)
    }
}
