//! Compact onboarding cards from mici/layouts/onboarding.py and system/ui/mici_setup.py.
use crate::{
    context::{Action, Context},
    mici::widgets::{big_button::BigButton, circle_button::CircleButton, dialog::Confirmation},
    paint, qr,
};
use openpilot_ui_framework::{
    animation::Bounce,
    assets::Texture,
    canvas::Canvas,
    draw::{Draw, BLACK, WHITE},
    geometry::{Point, Rect},
    navigation::NavWidget,
    scroller::Scroller,
    text::Font,
    text_layout::{float, Horizontal, Vertical},
    unified_label::UnifiedLabel,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::rc::Rc;

pub struct Pill {
    pub state: WidgetState,
    backgrounds: [Texture; 3],
    label: UnifiedLabel,
    bounce: Bounce,
}
impl Pill {
    fn new(canvas: &mut Canvas, text: &str, callback: Rc<dyn Fn()>) -> Result<Self, Error> {
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 402.0,
            height: 180.0,
        };
        state.click_delay = Some(0.075);
        state.click = Some(Box::new(move || callback()));
        let mut label = UnifiedLabel::new(text);
        label.size = 48.0;
        label.font = Font::Bold;
        label.horizontal = Horizontal::Center;
        label.vertical = Vertical::Middle;
        label.line_height = 0.9;
        Ok(Self {
            state,
            backgrounds: [
                paint::texture(canvas, "icons_mici/setup/continue.png", (402, 180))?,
                paint::texture(canvas, "icons_mici/setup/continue_pressed.png", (402, 180))?,
                paint::texture(canvas, "icons_mici/setup/continue_disabled.png", (402, 180))?,
            ],
            label,
            bounce: Bounce::new(1.0, 0.1, 20.0, 2.0),
        })
    }
}
impl Widget for Pill {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let pressed = self.state.is_pressed();
        let enabled = self.state.enabled.get();
        let scale = self.bounce.update(if pressed { 1.07 } else { 1.0 });
        let x = f64::from(rect.x) + f64::from(rect.width) * (1.0 - scale) / 2.0;
        let y = f64::from(rect.y) + f64::from(rect.height) * (1.0 - scale) / 2.0;
        self.backgrounds[if !enabled { 2 } else { usize::from(pressed) }].draw(
            draw,
            Point {
                x: float(x),
                y: float(y),
            },
            float(scale),
            WHITE,
        )?;
        self.label.color = paint::color(255, 255, 255, if enabled { 229 } else { 89 });
        self.label.set_rect(Rect {
            x: rect.x + 40.0,
            y: float(y + 23.0),
            width: rect.width - 80.0,
            height: rect.height - 46.0,
        });
        self.label.render(frame, draw)?;
        Ok(RenderResult::None)
    }
}
pub struct Cards {
    pub state: WidgetState,
    pub scroller: Scroller,
    context: Context,
    warm_driver: bool,
    black: bool,
}
impl Cards {
    fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let mut scroller = Scroller::new(true, false, !context.pc, 20.0);
        scroller.indicator = Some(paint::texture(
            canvas,
            "icons_mici/settings/horizontal_scroll_indicator.png",
            (96, 48),
        )?);
        Ok(Self {
            state: WidgetState::default(),
            scroller,
            context,
            warm_driver: false,
            black: false,
        })
    }
    pub fn navigation(self) -> NavWidget {
        NavWidget::new(Box::new(self), 20.0, 240.0)
    }
}
impl Widget for Cards {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.scroller.show(frame);
        if self.warm_driver {
            if let Err(e) = self
                .context
                .params
                .put_bool_nonblocking("IsDriverViewEnabled", true)
            {
                self.context.actions.push(Action::Failure(e));
            }
        }
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.scroller.hide(frame);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if self.black {
            draw.rounded(self.state.rect, 0.0, BLACK)?;
        }
        self.scroller.state.enabled = self.state.enabled.get().into();
        self.scroller.set_rect(self.state.rect);
        self.scroller.render(frame, draw)
    }
}
fn grey(
    canvas: &mut Canvas,
    title: &str,
    description: &str,
    icon: Option<(&str, (i32, i32))>,
) -> Result<BigButton, Error> {
    let mut card = BigButton::new(title, |path, size| paint::texture(canvas, path, size))?;
    card.set_grey();
    card.value = description.into();
    if let Some((path, size)) = icon {
        card.icon = Some(paint::texture(canvas, path, size)?);
    }
    Ok(card)
}
fn confirmation(
    context: &Context,
    canvas: &mut Canvas,
    title: &str,
    path: &str,
    callback: Rc<dyn Fn()>,
    exit: bool,
    red: bool,
) -> Result<CircleButton, Error> {
    let icon = paint::texture(canvas, path, (64, 64))?;
    let mut button = CircleButton::new(icon, |path, size| paint::texture(canvas, path, size))?;
    button.red = red;
    let context = context.clone();
    let title = title.to_owned();
    button.state.click = Some(Box::new(move || {
        context.actions.push(Action::MiciConfirm(Confirmation {
            title: title.clone(),
            icon,
            callback: callback.clone(),
            exit_on_confirm: exit,
            red,
        }))
    }));
    Ok(button)
}
struct TermsQr {
    state: WidgetState,
    texture: qr::texture::Texture,
}
impl TermsQr {
    fn new(draw: &mut dyn Draw) -> Result<Self, Error> {
        let mut texture = qr::texture::Texture::mici();
        texture.set_data(Some("https://comma.ai/terms"), draw)?;
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 170.0,
            height: 170.0,
        };
        Ok(Self { state, texture })
    }
}
impl Widget for TermsQr {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let r = self.state.rect;
        self.texture.draw(
            draw,
            Rect {
                x: r.x.round_ties_even(),
                y: r.y.round_ties_even(),
                ..r
            },
        )?;
        Ok(RenderResult::None)
    }
}
pub fn terms(
    context: Context,
    canvas: &mut Canvas,
    accept: Rc<dyn Fn()>,
    decline: Rc<dyn Fn()>,
) -> Result<Cards, Error> {
    let mut cards = Cards::new(context.clone(), canvas)?;
    cards.black = true;
    cards.scroller.add(Box::new(grey(
        canvas,
        "terms and\nconditions",
        "scroll to continue",
        Some(("icons_mici/setup/green_info.png", (64, 64))),
    )?))?;
    let mut qr_card = grey(
        canvas,
        "swipe for QR code",
        "or go to https://comma.ai/terms",
        None,
    )?;
    qr_card.icon = Some(canvas.texture(
        "icons_mici/setup/small_slider/slider_arrow.png",
        openpilot_startup_ui::renderer::TextureOptions {
            width: Some(64),
            height: Some(56),
            flip_x: true,
            ..Default::default()
        },
    )?);
    cards.scroller.add(Box::new(qr_card))?;
    cards.scroller.add(Box::new(TermsQr::new(canvas)?))?;
    cards.scroller.add(Box::new(grey(
        canvas,
        "",
        "You must accept the Terms & Conditions to use openpilot.",
        None,
    )?))?;
    cards.scroller.add(Box::new(confirmation(
        &context,
        canvas,
        "accept\nterms",
        "icons_mici/setup/driver_monitoring/dm_check.png",
        accept,
        true,
        false,
    )?))?;
    cards.scroller.add(Box::new(confirmation(
        &context,
        canvas,
        "decline &\nuninstall",
        "icons_mici/setup/cancel.png",
        decline,
        false,
        true,
    )?))?;
    Ok(cards)
}
pub fn attention(
    context: Context,
    canvas: &mut Canvas,
    continue_callback: Rc<dyn Fn()>,
) -> Result<Cards, Error> {
    let mut cards = Cards::new(context, canvas)?;
    cards.scroller.add(Box::new(grey(
        canvas,
        "what is openpilot?",
        "scroll to continue",
        Some(("icons_mici/setup/green_info.png", (64, 64))),
    )?))?;
    for text in [
        "1. openpilot is a driver assistance system.",
        "2. You must pay attention at all times.",
        "3. You must be ready to take over at any time.",
        "4. You are fully responsible for driving the car.",
    ] {
        cards
            .scroller
            .add(Box::new(grey(canvas, "", text, None)?))?;
    }
    cards
        .scroller
        .add(Box::new(Pill::new(canvas, "next", continue_callback)?))?;
    Ok(cards)
}
pub fn pre_dm(
    context: Context,
    canvas: &mut Canvas,
    continue_callback: Rc<dyn Fn()>,
) -> Result<Cards, Error> {
    let mut cards = Cards::new(context, canvas)?;
    cards.warm_driver = true;
    cards.scroller.add(Box::new(grey(
        canvas,
        "driver monitoring\ncheck",
        "scroll to continue",
        Some(("icons_mici/setup/green_dm.png", (64, 64))),
    )?))?;
    for text in [
        "Next, we'll check if comma four can detect the driver properly.",
        "openpilot uses the cabin camera to check if the driver is distracted.",
        "If it does not have a clear view of the driver, unplug and remount before continuing.",
    ] {
        cards
            .scroller
            .add(Box::new(grey(canvas, "", text, None)?))?;
    }
    cards
        .scroller
        .add(Box::new(Pill::new(canvas, "next", continue_callback)?))?;
    Ok(cards)
}
pub fn bad_face(context: Context, canvas: &mut Canvas, back: Rc<dyn Fn()>) -> Result<Cards, Error> {
    let mut cards = Cards::new(context, canvas)?;
    cards.scroller.add(Box::new(grey(
        canvas,
        "looking for driver",
        "make sure comma\nfour can see your face",
        Some(("icons_mici/setup/orange_dm.png", (64, 64))),
    )?))?;
    cards.scroller.add(Box::new(grey(
        canvas,
        "",
        "Remount if your face is blocked, or driver monitoring has difficulty tracking your face.",
        None,
    )?))?;
    cards
        .scroller
        .add(Box::new(Pill::new(canvas, "back", back)?))?;
    Ok(cards)
}
pub fn record_front(
    context: Context,
    canvas: &mut Canvas,
    continue_callback: Rc<dyn Fn()>,
) -> Result<Cards, Error> {
    let mut cards = Cards::new(context.clone(), canvas)?;
    cards.scroller.add(Box::new(grey(
        canvas,
        "driver camera data",
        "do you want to share video data for training?",
        Some(("icons_mici/setup/green_dm.png", (64, 64))),
    )?))?;
    cards.scroller.add(Box::new(grey(
        canvas,
        "",
        "Sharing your data with comma helps improve openpilot for everyone.",
        None,
    )?))?;
    for (title, path, value) in [
        (
            "allow data uploading",
            "icons_mici/setup/driver_monitoring/dm_check.png",
            true,
        ),
        ("no, don't upload", "icons_mici/setup/cancel.png", false),
    ] {
        let c = context.clone();
        let next = continue_callback.clone();
        let callback = Rc::new(
            move || match c.params.put_bool_nonblocking("RecordFront", value) {
                Ok(()) => next(),
                Err(e) => c.actions.push(Action::Failure(e)),
            },
        );
        cards.scroller.add(Box::new(confirmation(
            &context, canvas, title, path, callback, false, false,
        )?))?;
    }
    Ok(cards)
}
