//! Live driver-monitoring tutorial from mici/layouts/onboarding.py (MIT).
use super::{
    cards,
    dm_progress::{Input, Progress},
};
use crate::{
    context::{Action, Context, Event},
    mici::onroad::driver_camera::Preview,
    paint,
    params::Read,
};
use openpilot_ui_framework::{
    assets::Texture,
    callback::Callback,
    canvas::Canvas,
    draw::{Draw, RoundedOutline, BLACK},
    geometry::{Point, Rect},
    navigation::NavWidget,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use std::{cell::Cell, rc::Rc};
struct SmallCircle {
    state: WidgetState,
    icon: Texture,
    backgrounds: [Texture; 3],
}
impl SmallCircle {
    fn new(canvas: &mut Canvas, path: &str, size: (i32, i32)) -> Result<Self, Error> {
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        Ok(Self {
            state,
            icon: paint::texture(canvas, path, size)?,
            backgrounds: [
                paint::texture(canvas, "icons_mici/setup/small_button.png", (100, 100))?,
                paint::texture(
                    canvas,
                    "icons_mici/setup/small_button_pressed.png",
                    (100, 100),
                )?,
                paint::texture(
                    canvas,
                    "icons_mici/setup/small_button_disabled.png",
                    (100, 100),
                )?,
            ],
        })
    }
}
impl Widget for SmallCircle {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let r = self.state.rect;
        let enabled = self.state.enabled.get();
        let index = if enabled {
            usize::from(self.state.is_pressed())
        } else {
            2
        };
        self.backgrounds[index].draw(
            draw,
            Point { x: r.x, y: r.y },
            1.0,
            paint::color(255, 255, 255, 255),
        )?;
        self.icon.draw(
            draw,
            Point {
                x: r.x + (r.width - self.icon.width) / 2.0,
                y: r.y + (r.height - self.icon.height) / 2.0,
            },
            1.0,
            paint::color(255, 255, 255, if enabled { 255 } else { 89 }),
        )?;
        Ok(RenderResult::None)
    }
}
struct BadFace {
    state: WidgetState,
    cards: cards::Cards,
    active: Rc<Cell<bool>>,
}
impl Widget for BadFace {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.active.set(true);
        self.cards.show(frame);
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.active.set(false);
        self.cards.hide(frame);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.cards.state.enabled = self.state.enabled.get().into();
        self.cards.set_rect(self.state.rect);
        self.cards.render(frame, draw)
    }
}
pub struct Tutorial {
    pub state: WidgetState,
    context: Context,
    pub preview: Preview,
    pub progress: Progress,
    back: SmallCircle,
    good: SmallCircle,
    bad_page: WidgetHandle,
    bad_active: Rc<Cell<bool>>,
    push_bad: Rc<Cell<bool>>,
    interaction: Rc<Cell<bool>>,
    inactivity: Callback<()>,
}
impl Tutorial {
    pub fn new(context: Context, canvas: &mut Canvas, next: Rc<dyn Fn()>) -> Result<Self, Error> {
        let preview = Preview::new(context.clone(), canvas)?;
        Self::with_preview(context, canvas, preview, next)
    }
    pub fn with_preview(
        context: Context,
        canvas: &mut Canvas,
        preview: Preview,
        next: Rc<dyn Fn()>,
    ) -> Result<Self, Error> {
        let bad_active = Rc::new(Cell::new(false));
        let dismiss = Rc::new(Cell::new(false));
        let request = dismiss.clone();
        let cards = cards::bad_face(context.clone(), canvas, Rc::new(move || request.set(true)))?;
        let mut bad_nav = NavWidget::new(
            Box::new(BadFace {
                state: WidgetState::default(),
                cards,
                active: bad_active.clone(),
            }),
            20.0,
            240.0,
        );
        bad_nav.after_content = Some(Box::new(move |nav, _| {
            if dismiss.replace(false) {
                nav.dismiss(None);
            }
        }));
        let bad_page = WidgetHandle::new(bad_nav);
        let push_bad = Rc::new(Cell::new(false));
        let request = push_bad.clone();
        let mut back = SmallCircle::new(
            canvas,
            "icons_mici/setup/driver_monitoring/dm_question.png",
            (28, 48),
        )?;
        back.state.click = Some(Box::new(move || request.set(true)));
        let mut good = SmallCircle::new(
            canvas,
            "icons_mici/setup/driver_monitoring/dm_check.png",
            (42, 42),
        )?;
        good.state.enabled = false.into();
        good.state.click = Some(Box::new(move || next()));
        let interaction = Rc::new(Cell::new(true));
        let gate = interaction.clone();
        back.state.touch_valid = Some(Box::new(move || gate.get()));
        let gate = interaction.clone();
        good.state.touch_valid = Some(Box::new(move || gate.get()));
        let params = context.params.clone();
        let actions = context.actions.clone();
        let inactivity = Callback::new(move |()| {
            if let Err(e) = params.put_bool("IsDriverViewEnabled", false) {
                actions.push(Action::Failure(e));
            }
        });
        context.listen(Event::InteractiveTimeout, inactivity.clone());
        Ok(Self {
            state: WidgetState::default(),
            preview,
            context,
            progress: Progress::default(),
            back,
            good,
            bad_page,
            bad_active,
            push_bad,
            interaction,
            inactivity,
        })
    }
    pub fn navigation(self) -> NavWidget {
        NavWidget::new(Box::new(self), 20.0, 240.0)
    }
    fn update_progress(&mut self, frame: &Frame<'_>) -> Result<(), crate::Error> {
        if self.context.device.borrow().awake
            && !self.context.params.boolean("IsDriverViewEnabled")?
        {
            self.context
                .params
                .put_bool_nonblocking("IsDriverViewEnabled", true)?;
        }
        let sm = self.context.messages.borrow();
        let topic = sm.state.topic("driverMonitoringState")?;
        if topic.receive_frame == 0 {
            return Ok(());
        }
        let dm = match topic.event()?.which()? {
            openpilot_cereal::log_capnp::event::Which::DriverMonitoringState(value) => value?,
            _ => return Err(crate::Error::Contract("expected driver monitoring state")),
        };
        let orientation = self.preview.driver_orientation()?;
        self.progress.update(&Input {
            received: true,
            face_detected: dm.get_vision_policy_state()?.get_face_detected(),
            orientation,
            bad_face_page: self.bad_active.get(),
            fps: frame.target_fps,
        });
        self.good.state.enabled = self.progress.good_enabled.into();
        Ok(())
    }
}
impl Drop for Tutorial {
    fn drop(&mut self) {
        self.context
            .callbacks
            .borrow_mut()
            .retain(|(_, callback)| !callback.same(&self.inactivity));
    }
}
impl Widget for Tutorial {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.preview.show(frame);
        self.progress.show();
    }
    fn update(&mut self, frame: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.interaction.set(self.state.enabled.get());
        self.update_progress(frame)?;
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let r = self.state.rect;
        self.preview.set_rect(r);
        self.preview.render(frame, draw)?;
        let y = openpilot_ui_framework::text_layout::float(
            (f64::from(r.y) + f64::from(r.height) - 80.0).trunc(),
        );
        let height = r.y.trunc() + r.height.trunc() - y;
        draw.gradient(
            Rect {
                x: r.x.trunc(),
                y,
                width: r.width.trunc(),
                height,
            },
            [0, BLACK, BLACK, 0],
        )?;
        draw.ring(self.progress.ring(r, self.preview.is_rhd()))?;
        if self.preview.has_frame() {
            self.back
                .set_position(r.x + 8.0, r.y + r.height - self.back.state.rect.height);
            self.back.render(frame, draw)?;
            self.good.set_position(
                openpilot_ui_framework::text_layout::float(
                    f64::from(r.x) + f64::from(r.width)
                        - f64::from(self.good.state.rect.width)
                        - 8.0,
                ),
                openpilot_ui_framework::text_layout::float(
                    f64::from(r.y) + f64::from(r.height) - f64::from(self.good.state.rect.height),
                ),
            );
            self.good.render(frame, draw)?;
        }
        draw.scissor(Some(Rect {
            x: r.x.trunc(),
            y: r.y.trunc(),
            width: r.width.trunc(),
            height: r.height.trunc(),
        }))?;
        let result = draw.rounded_outline(
            r,
            RoundedOutline {
                roundness: 0.2 * 1.02,
                segments: 10,
                thickness: 50.0,
                color: BLACK,
            },
        );
        draw.scissor(None)?;
        result?;
        if self.push_bad.replace(false) {
            frame
                .navigation
                .push(NavigationRequest::Push(self.bad_page.clone()));
        }
        Ok(RenderResult::None)
    }
}
