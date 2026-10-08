//! Original big-display pairing dialog; token failures retain the empty-token URL.
mod render;
use crate::{
    api,
    context::Context,
    paint,
    params::Read,
    qr::{texture::Texture, Correction},
};
use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    button::IconButton,
    canvas::Canvas,
    draw::Draw,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetState},
    Error,
};
pub struct Pairing {
    pub state: WidgetState,
    pub last_generation: f64,
    pub url: Box<dyn FnMut() -> String>,
    context: Context,
    qr: Texture,
    close: IconButton,
}
impl Pairing {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let source = context.clone();
        let close = IconButton::new(paint::texture(canvas, "icons/close.png", (80, 80))?, 20.0);
        Ok(Self {
            state: WidgetState::default(),
            last_generation: f64::NEG_INFINITY,
            url: Box::new(move || pairing_url(&source)),
            context,
            qr: Texture::new(Correction::Low),
            close,
        })
    }
}
impl Widget for Pairing {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, frame: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        if self.context.prime.is_paired() {
            frame.navigation.push(NavigationRequest::Pop(None));
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if frame.monotonic - self.last_generation >= 300.0 {
            let url = (self.url)();
            self.qr.destroy();
            self.qr.set_data(Some(&url), draw)?;
            self.last_generation = frame.monotonic;
        }
        self.render_content(frame, draw)?;
        Ok(RenderResult::Value(-1))
    }
}

pub fn pairing_url(context: &Context) -> String {
    let identity = context.params.string("DongleId").unwrap_or_default();
    let now = (context.now_wall)();
    let token = api::pairing(
        &identity,
        now.timestamp().to_f64().unwrap_or(f64::NAN),
        &context.persist_root,
    )
    .unwrap_or_else(|error| {
        openpilot_startup_ui::logging::emit(
            openpilot_logging::record::Level::Error,
            format!("Failed to get pairing token: {error}"),
        );
        String::new()
    });
    format!("https://connect.comma.ai/?pair={token}")
}
