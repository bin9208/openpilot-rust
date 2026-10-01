//! Source: selfdrive/ui/layouts/onboarding.py (MIT).
use super::training::Training;
use crate::{
    context::{Action, Context},
    params::Read,
};
use openpilot_ui_framework::{
    button::{Button, ButtonStyle},
    callback::Callback,
    draw::Draw,
    geometry::Rect,
    label::Label,
    text::Font,
    text_layout::Horizontal,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::Cell, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Page {
    Terms,
    Training,
    Decline,
}
struct Terms {
    state: WidgetState,
    title: Label,
    description: Label,
    decline: Button,
    accept: Button,
}
fn label(text: String, font: Font) -> Label {
    let mut label = Label::new(text);
    label.size = 90.0;
    label.font = font;
    label.horizontal = Horizontal::Left;
    label
}
impl Terms {
    fn new(context: &Context, page: Rc<Cell<Page>>, accepted: Rc<Cell<bool>>) -> Self {
        let mut decline = Button::new(context.tr("Decline"));
        let state = page.clone();
        decline.state.click = Some(Box::new(move || state.set(Page::Decline)));
        let mut accept = Button::new(context.tr("Agree"));
        accept.set_style(ButtonStyle::Primary);
        let c = context.clone();
        accept.state.click = Some(Box::new(move || {
            match c.params.put(
                "HasAcceptedTerms",
                openpilot_runtime_version::TERMS_VERSION.as_bytes(),
            ) {
                Ok(()) => {
                    page.set(Page::Training);
                    accepted.set(true);
                }
                Err(e) => c.actions.push(Action::Failure(e)),
            }
        }));
        Self{state:WidgetState::default(),title:label(context.tr("Welcome to openpilot"),Font::Bold),description:label(context.tr("You must accept the Terms and Conditions to use openpilot. Read the latest terms at https://comma.ai/terms before continuing."),Font::Medium),decline,accept}
    }
}
impl Widget for Terms {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let r = self.state.rect;
        let x = r.x + 165.0;
        let y = r.y + 165.0;
        self.title.set_rect(Rect {
            x,
            y,
            width: r.width - x,
            height: 90.0,
        });
        self.title.render(frame, draw)?;
        let y = y - 100.0;
        self.description.set_rect(Rect {
            x,
            y,
            width: r.width - x,
            height: r.height - y - 250.0,
        });
        self.description.render(frame, draw)?;
        let y = r.y + r.height - 205.0;
        let width = (r.width - 135.0) / 2.0;
        self.decline.set_rect(Rect {
            x: r.x + 45.0,
            y,
            width,
            height: 160.0,
        });
        self.decline.render(frame, draw)?;
        self.accept.set_rect(Rect {
            x: r.x + 90.0 + width,
            y,
            width,
            height: 160.0,
        });
        self.accept.render(frame, draw)?;
        Ok(RenderResult::Value(-1))
    }
}
struct Decline {
    state: WidgetState,
    text: Label,
    back: Button,
    uninstall: Button,
}
impl Decline {
    fn new(context: &Context, page: Rc<Cell<Page>>) -> Self {
        let mut back = Button::new(context.tr("Back"));
        back.state.click = Some(Box::new(move || page.set(Page::Terms)));
        let mut uninstall = Button::new(context.tr("Decline, uninstall openpilot"));
        uninstall.set_style(ButtonStyle::Danger);
        let c = context.clone();
        uninstall.state.click = Some(Box::new(move || {
            match c.params.put_bool("DoUninstall", true) {
                Ok(()) => c.actions.push(Action::Exit),
                Err(e) => c.actions.push(Action::Failure(e)),
            }
        }));
        Self {
            state: WidgetState::default(),
            text: label(
                context.tr("You must accept the Terms and Conditions in order to use openpilot."),
                Font::Medium,
            ),
            back,
            uninstall,
        }
    }
}
impl Widget for Decline {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let r = self.state.rect;
        let y = r.y + r.height - 205.0;
        let width = (r.width - 135.0) / 2.0;
        self.back.set_rect(Rect {
            x: r.x + 45.0,
            y,
            width,
            height: 160.0,
        });
        self.back.render(frame, draw)?;
        self.uninstall.set_rect(Rect {
            x: r.x + 90.0 + width,
            y,
            width,
            height: 160.0,
        });
        self.uninstall.render(frame, draw)?;
        let height = y - 245.0;
        self.text.set_rect(Rect {
            x: r.x + 165.0,
            y: r.y + (y - height) / 2.0 + 10.0,
            width: r.width - 330.0,
            height,
        });
        self.text.render(frame, draw)?;
        Ok(RenderResult::None)
    }
}
pub struct Onboarding {
    pub state: WidgetState,
    context: Context,
    pub page: Rc<Cell<Page>>,
    accepted_initially: bool,
    trained_initially: bool,
    accepted_now: Rc<Cell<bool>>,
    terms: Terms,
    decline: Decline,
    pub training: Option<Training>,
}
impl Onboarding {
    pub fn new(context: Context) -> Result<Self, Error> {
        let accepted_initially =
            context.params.string("HasAcceptedTerms")? == openpilot_runtime_version::TERMS_VERSION;
        let trained_initially = context.params.string("CompletedTrainingVersion")?
            == openpilot_runtime_version::TRAINING_VERSION;
        let page = Rc::new(Cell::new(if accepted_initially {
            Page::Training
        } else {
            Page::Terms
        }));
        let accepted_now = Rc::new(Cell::new(false));
        Ok(Self {
            state: WidgetState::default(),
            terms: Terms::new(&context, page.clone(), accepted_now.clone()),
            decline: Decline::new(&context, page.clone()),
            context,
            page,
            accepted_initially,
            trained_initially,
            accepted_now,
            training: None,
        })
    }
    pub fn completed(&self) -> bool {
        self.accepted_initially && self.trained_initially
    }
}
impl Widget for Onboarding {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        if self.training.is_none() {
            let mut training = Training::new(self.context.clone(), draw)?;
            let c = self.context.clone();
            training.completed = Some(Callback::new(move |()| {
                if let Err(e) = c.params.put(
                    "CompletedTrainingVersion",
                    openpilot_runtime_version::TRAINING_VERSION.as_bytes(),
                ) {
                    c.actions.push(Action::Failure(e));
                }
            }));
            self.training = Some(training);
        }
        if self.page.get() == Page::Terms {
            self.terms.set_rect(self.state.rect);
            self.terms.render(frame, draw)?;
        }
        if self.accepted_now.replace(false) && self.trained_initially {
            frame.navigation.push(NavigationRequest::Pop(None));
        }
        match self.page.get() {
            Page::Training => {
                let training = self
                    .training
                    .as_mut()
                    .ok_or(Error::Contract("training missing"))?;
                training.set_rect(self.state.rect);
                training.render(frame, draw)?;
            }
            Page::Decline => {
                self.decline.set_rect(self.state.rect);
                self.decline.render(frame, draw)?;
            }
            Page::Terms => {}
        }
        Ok(RenderResult::Value(-1))
    }
}
