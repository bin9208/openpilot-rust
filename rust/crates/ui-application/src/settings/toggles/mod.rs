mod build;
mod changes;
mod copy;
mod refresh;
use crate::context::{Action, Context, Event};
use openpilot_ui_framework::{
    assets::Texture,
    callback::Callback,
    canvas::Canvas,
    draw::Draw,
    list::{ListItem, MultipleButtonAction, ToggleAction},
    scroller_tici::Scroller,
    widget::{Frame, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};
struct Definition {
    key: &'static str,
    title: &'static str,
    description: &'static str,
    icon: &'static str,
    restart: bool,
}
enum Change {
    Toggle(&'static str, bool),
    Personality(usize),
}
pub struct Toggles {
    pub state: WidgetState,
    context: Context,
    pub scroller: Scroller,
    indices: BTreeMap<&'static str, usize>,
    locked: BTreeSet<&'static str>,
    icons: [Texture; 2],
    changes: Rc<RefCell<VecDeque<Change>>>,
    handle: Option<openpilot_ui_framework::widget::WeakWidgetHandle>,
    is_release: bool,
}
impl Toggles {
    pub fn create(context: Context, canvas: &mut Canvas) -> Result<WidgetHandle, Error> {
        let widget = WidgetHandle::new(Self::new(context.clone(), canvas)?);
        widget.get_mut::<Self>()?.handle = Some(widget.downgrade());
        let weak = widget.downgrade();
        let actions = context.actions.clone();
        context.listen(
            Event::Engaged,
            Callback::new(move |()| {
                if let Some(widget) = weak.upgrade() {
                    let result = (|| -> Result<(), crate::Error> {
                        widget.get_mut::<Toggles>()?.refresh()?;
                        Ok(())
                    })();
                    if let Err(error) = result {
                        actions.push(Action::Failure(error));
                    }
                }
            }),
        );
        Ok(widget)
    }
    fn item(&mut self, key: &str) -> Result<&mut ListItem, Error> {
        let index = *self
            .indices
            .get(key)
            .ok_or(Error::Contract("toggle item missing"))?;
        self.scroller
            .item_mut(index)
            .ok_or(Error::Contract("toggle item type mismatch"))
    }
    fn toggle(&mut self, key: &str) -> Result<&mut ToggleAction, Error> {
        self.item(key)?
            .action_mut::<ToggleAction>()
            .ok_or(Error::Contract("toggle action type mismatch"))
    }
    fn personality(&mut self) -> Result<&mut MultipleButtonAction, Error> {
        self.item("LongitudinalPersonality")?
            .action_mut::<MultipleButtonAction>()
            .ok_or(Error::Contract("personality action missing"))
    }
    fn update_icon(&mut self) -> Result<(), Error> {
        let value = self.toggle("ExperimentalMode")?.toggle.value();
        let icon = self.icons[usize::from(value)];
        self.item("ExperimentalMode")?.icon = Some(icon);
        Ok(())
    }
}
impl Widget for Toggles {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.scroller.show(frame);
        if let Err(error) = self.refresh() {
            self.context.actions.push(Action::Failure(error.into()));
        }
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let value = {
            let messages = self.context.messages.borrow();
            if messages
                .state
                .topic("selfdriveState")
                .map_err(crate::Error::from)?
                .updated
            {
                Some(i32::from(u16::from(
                    crate::state::messages::selfdrive_state(&messages.state)?
                        .get_personality()
                        .map_err(crate::Error::from)?,
                )))
            } else {
                None
            }
        };
        if let Some(value) = value {
            let change = {
                let ui = self.context.ui.borrow();
                ui.personality != value && ui.started
            };
            if change {
                self.personality()?.set_selected(
                    usize::try_from(value)
                        .map_err(|_| Error::Contract("personality index negative"))?,
                );
            }
            self.context.ui.borrow_mut().personality = value;
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller.set_rect(self.state.rect);
        self.scroller.render(frame, draw)?;
        self.process_changes()?;
        Ok(RenderResult::None)
    }
}
