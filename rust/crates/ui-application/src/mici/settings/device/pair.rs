use crate::{
    context::{Action, Context, Page},
    mici::widgets::big_button::BigButton,
    paint,
    params::Read,
};
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub(super) struct Pair {
    button: BigButton,
    context: Context,
}
impl Pair {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let mut button = BigButton::new("pair", |path, size| paint::texture(canvas, path, size))?;
        button.value = "connect.comma.ai".into();
        button.font_size = Some(64.0);
        button.icon = Some(paint::texture(
            canvas,
            "icons_mici/settings/comma_icon.png",
            (33, 60),
        )?);
        Ok(Self { button, context })
    }
}
impl Widget for Pair {
    fn state(&self) -> &WidgetState {
        &self.button.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.button.state
    }
    fn set_position(&mut self, x: f32, y: f32) {
        self.button.set_position(x, y);
    }
    fn set_rect(&mut self, rect: Rect) {
        self.button.set_rect(rect);
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        if self.context.prime.is_paired() {
            self.button.text = "paired".into();
            self.button.value = if self.context.prime.is_prime() {
                "subscribed"
            } else {
                "upgrade to prime"
            }
            .into();
        } else {
            self.button.text = "pair".into();
            self.button.value = "connect.comma.ai".into();
        }
        Ok(())
    }
    fn mouse_release(
        &mut self,
        position: Point,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.button.mouse_release(position, frame, draw)?;
        if self.context.prime.is_paired() {
            return Ok(());
        }
        let error = if !self.context.system_time_valid()? {
            Some("Please connect to Wi-Fi to complete initial pairing.")
        } else if matches!(
            self.context.params.string("DongleId")?.as_str(),
            "" | "UnregisteredDevice"
        ) {
            Some("Device must be registered with the comma.ai backend to pair.")
        } else {
            None
        };
        if let Some(error) = error {
            self.context.actions.push(Action::MiciAlert {
                title: String::new(),
                description: self.context.tr(error),
            });
        } else {
            self.context.open(Page::Pairing);
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.button.paint(frame, draw)
    }
}
