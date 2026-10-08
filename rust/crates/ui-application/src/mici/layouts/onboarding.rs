//! Source: selfdrive/ui/mici/layouts/onboarding.py (MIT).
use super::{
    cards::{self, Cards},
    tutorial::Tutorial,
};
use crate::{
    context::{Action, Context},
    params::Read,
};
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::{Draw, BLACK},
    geometry::Rect,
    navigation::NavWidget,
    widget::{
        Frame, NavigationQueue, NavigationRequest, RenderResult, Widget, WidgetHandle, WidgetState,
    },
    Error,
};
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
struct Forward {
    navigation: Option<NavigationQueue>,
    pending: Vec<WidgetHandle>,
}
type Pushes = Rc<RefCell<Forward>>;
fn bind(pushes: &Pushes, frame: &Frame<'_>) {
    let mut pushes = pushes.borrow_mut();
    pushes.navigation = Some(frame.navigation.clone());
    for widget in pushes.pending.drain(..) {
        frame.navigation.push(NavigationRequest::Push(widget));
    }
}
pub struct QueuedCards {
    pub state: WidgetState,
    pub cards: Cards,
    pushes: Pushes,
}
impl QueuedCards {
    fn new(cards: Cards, pushes: Pushes) -> Self {
        Self {
            state: WidgetState::default(),
            cards,
            pushes,
        }
    }
}
impl Widget for QueuedCards {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.cards.show(frame);
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.cards.hide(frame);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.cards.state.enabled = self.state.enabled.get().into();
        self.cards.set_rect(self.state.rect);
        bind(&self.pushes, frame);
        self.cards.render(frame, draw)
    }
}
pub struct QueuedTutorial {
    state: WidgetState,
    pub tutorial: Tutorial,
    pushes: Pushes,
}
impl Widget for QueuedTutorial {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.tutorial.show(frame);
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.tutorial.hide(frame);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.tutorial.state.enabled = self.state.enabled.get().into();
        self.tutorial.set_rect(self.state.rect);
        bind(&self.pushes, frame);
        self.tutorial.render(frame, draw)
    }
}
fn forward(target: WidgetHandle) -> (Pushes, Rc<dyn Fn()>) {
    let queue = Rc::new(RefCell::new(Forward::default()));
    let pushes = queue.clone();
    (
        queue,
        Rc::new(move || {
            let mut pushes = pushes.borrow_mut();
            if let Some(navigation) = &pushes.navigation {
                navigation.push(NavigationRequest::Push(target.clone()));
            } else {
                pushes.pending.push(target.clone());
            }
        }),
    )
}
fn navigation(content: impl Widget) -> WidgetHandle {
    WidgetHandle::new(NavWidget::new(Box::new(content), 20.0, 240.0))
}
struct ReviewTraining {
    context: Context,
    content: QueuedCards,
}
impl Widget for ReviewTraining {
    fn state(&self) -> &WidgetState {
        self.content.state()
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        self.content.state_mut()
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.content.show(frame);
        self.context
            .actions
            .push(Action::SetInteractiveTimeout(Some(300)));
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.content.hide(frame);
        self.context
            .actions
            .push(Action::SetInteractiveTimeout(None));
        if let Err(error) = self
            .context
            .params
            .put_bool_nonblocking("IsDriverViewEnabled", false)
        {
            self.context.actions.push(Action::Failure(error));
        }
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.content.paint(frame, draw)
    }
}
pub fn review_training(
    context: Context,
    canvas: &mut Canvas,
    completed: Rc<dyn Fn()>,
    camera: &str,
) -> Result<NavWidget, Error> {
    let record_front = navigation(cards::record_front(context.clone(), canvas, completed)?);
    let (pushes, next) = forward(record_front);
    let preview = crate::mici::onroad::driver_camera::Preview::with_camera(
        context.clone(),
        canvas,
        camera,
        20.0,
        true,
    )?;
    let tutorial = navigation(QueuedTutorial {
        state: WidgetState::default(),
        tutorial: Tutorial::with_preview(context.clone(), canvas, preview, next)?,
        pushes,
    });
    let (pushes, next) = forward(tutorial);
    let pre_dm = navigation(QueuedCards::new(
        cards::pre_dm(context.clone(), canvas, next)?,
        pushes,
    ));
    let (pushes, next) = forward(pre_dm);
    let content = QueuedCards::new(cards::attention(context.clone(), canvas, next)?, pushes);
    Ok(NavWidget::new(
        Box::new(ReviewTraining { context, content }),
        20.0,
        240.0,
    ))
}
pub struct Onboarding {
    pub state: WidgetState,
    context: Context,
    accepted_initially: bool,
    trained_initially: bool,
    pub terms: QueuedCards,
    pub training: WidgetHandle,
    pub pre_dm: WidgetHandle,
    pub tutorial: WidgetHandle,
    pub record_front: WidgetHandle,
    completed: Rc<dyn Fn()>,
}
impl Onboarding {
    pub fn new(
        context: Context,
        canvas: &mut Canvas,
        completed: Rc<dyn Fn()>,
    ) -> Result<Self, Error> {
        Self::build(context, canvas, completed, None)
    }
    pub fn with_preview(
        context: Context,
        canvas: &mut Canvas,
        completed: Rc<dyn Fn()>,
        preview: crate::mici::onroad::driver_camera::Preview,
    ) -> Result<Self, Error> {
        Self::build(context, canvas, completed, Some(preview))
    }
    fn build(
        context: Context,
        canvas: &mut Canvas,
        completed: Rc<dyn Fn()>,
        preview: Option<crate::mici::onroad::driver_camera::Preview>,
    ) -> Result<Self, Error> {
        let accepted_initially =
            context.params.string("HasAcceptedTerms")? == openpilot_runtime_version::TERMS_VERSION;
        let trained_initially = context.params.string("CompletedTrainingVersion")?
            == openpilot_runtime_version::TRAINING_VERSION;
        let params = context.params.clone();
        let actions = context.actions.clone();
        let done = completed.clone();
        let finish = Rc::new(move || {
            let result = (|| -> Result<(), crate::Error> {
                params.put(
                    "CompletedTrainingVersion",
                    openpilot_runtime_version::TRAINING_VERSION.as_bytes(),
                )?;
                params.put_bool_nonblocking("IsDriverViewEnabled", false)?;
                Ok(())
            })();
            match result {
                Ok(()) => done(),
                Err(e) => actions.push(Action::Failure(e)),
            }
        });
        let record_front = navigation(cards::record_front(context.clone(), canvas, finish)?);
        let (pushes, next) = forward(record_front.clone());
        let tutorial = navigation(QueuedTutorial {
            state: WidgetState::default(),
            tutorial: match preview {
                Some(preview) => Tutorial::with_preview(context.clone(), canvas, preview, next)?,
                None => Tutorial::new(context.clone(), canvas, next)?,
            },
            pushes,
        });
        let (pushes, next) = forward(tutorial.clone());
        let pre_dm = navigation(QueuedCards::new(
            cards::pre_dm(context.clone(), canvas, next)?,
            pushes,
        ));
        let (pushes, next) = forward(pre_dm.clone());
        let training = navigation(QueuedCards::new(
            cards::attention(context.clone(), canvas, next)?,
            pushes,
        ));
        let (pushes, next) = forward(training.clone());
        let params = context.params.clone();
        let actions = context.actions.clone();
        let accept = Rc::new(move || {
            match params.put(
                "HasAcceptedTerms",
                openpilot_runtime_version::TERMS_VERSION.as_bytes(),
            ) {
                Ok(()) => next(),
                Err(e) => actions.push(Action::Failure(e)),
            }
        });
        let params = context.params.clone();
        let actions = context.actions.clone();
        let decline = Rc::new(move || {
            if let Err(e) = params.put_bool("DoUninstall", true) {
                actions.push(Action::Failure(e));
            }
        });
        let terms = QueuedCards::new(
            cards::terms(context.clone(), canvas, accept, decline)?,
            pushes,
        );
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 536.0,
            height: 240.0,
        };
        Ok(Self {
            state,
            context,
            accepted_initially,
            trained_initially,
            terms,
            training,
            pre_dm,
            tutorial,
            record_front,
            completed,
        })
    }
    pub fn completed(&self) -> bool {
        self.accepted_initially && self.trained_initially
    }
    pub fn close(&self) -> Result<(), crate::Error> {
        self.context
            .params
            .put_bool_nonblocking("IsDriverViewEnabled", false)?;
        (self.completed)();
        Ok(())
    }
}
impl Widget for Onboarding {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, _: &Frame<'_>) {
        self.context
            .actions
            .push(Action::SetInteractiveTimeout(Some(300)));
        self.context
            .actions
            .push(Action::SetOffroadBrightness(Some(100)));
    }
    fn hide(&mut self, _: &Frame<'_>) {
        self.context
            .actions
            .push(Action::SetInteractiveTimeout(None));
        self.context
            .actions
            .push(Action::SetOffroadBrightness(None));
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        draw.rounded(self.state.rect, 0.0, BLACK)?;
        self.terms.state.enabled = self.state.enabled.get().into();
        self.terms.set_rect(self.state.rect);
        self.terms.render(frame, draw)
    }
}
