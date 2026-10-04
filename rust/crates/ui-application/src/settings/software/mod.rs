//! Large-display software settings, including the updater idle transition latch.
mod actions;
mod refresh;
mod time;
use crate::{context::Context, params::Read};
use openpilot_ui_framework::{
    callback::Callback,
    draw::Draw,
    list::{ButtonAction, ListItem, TextAction},
    scroller_tici::Scroller,
    widget::{Frame, RenderResult, WeakWidgetHandle, Widget, WidgetHandle, WidgetState},
    Error,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
#[derive(Clone, Copy)]
enum Change {
    Download,
    Install,
    Branch,
    Uninstall,
}
pub struct Software {
    state: WidgetState,
    context: Context,
    pub scroller: Scroller,
    waiting: Option<f64>,
    changes: Rc<RefCell<VecDeque<Change>>>,
    handle: Option<WeakWidgetHandle>,
    state_text: [(String, String); 3],
}
impl Software {
    pub fn create(context: Context) -> Result<WidgetHandle, Error> {
        let widget = WidgetHandle::new(Self::new(context)?);
        widget.get_mut::<Self>()?.handle = Some(widget.downgrade());
        Ok(widget)
    }
    fn new(context: Context) -> Result<Self, Error> {
        let changes = Rc::new(RefCell::new(VecDeque::new()));
        let mut scroller = Scroller {
            spacing: 0.0,
            line_separator: true,
            ..Default::default()
        };
        let mut warning = ListItem::new("")?;
        warning.title = context.text("Updates are only downloaded while the car is off.");
        scroller.add(Box::new(warning));
        let mut version = ListItem::new("")?;
        version.title = context.text("Current Version");
        version.action = Some(Box::new(TextAction::new(
            context.params.string("UpdaterCurrentDescription")?,
            crate::paint::color(170, 170, 170, 255),
        )));
        scroller.add(Box::new(version));
        for (title, text, change) in [
            ("Download", "CHECK", Change::Download),
            ("Install Update", "INSTALL", Change::Install),
            ("Target Branch", "SELECT", Change::Branch),
            ("Uninstall", "UNINSTALL", Change::Uninstall),
        ] {
            let mut item = ListItem::new("")?;
            item.title = context.text(title);
            let mut action = ButtonAction::new("");
            action.text = context.text(text);
            if matches!(change, Change::Install) {
                item.state.visible = false.into();
            }
            if matches!(change, Change::Branch) {
                item.state.visible = (!context.params.boolean("IsTestedBranch")?).into();
                action.value = context.params.string("UpdaterTargetBranch")?.into();
            }
            item.action = Some(Box::new(action));
            let queue = changes.clone();
            item.callback = Some(Callback::new(move |()| {
                queue.borrow_mut().push_back(change)
            }));
            scroller.add(Box::new(item));
        }
        let state_text = ["checking...", "downloading...", "finalizing update..."]
            .map(|state| (state.into(), context.tr(state)));
        Ok(Self {
            state: WidgetState::default(),
            context,
            scroller,
            waiting: None,
            changes,
            handle: None,
            state_text,
        })
    }
    fn item(&mut self, index: usize) -> Result<&mut ListItem, Error> {
        self.scroller
            .item_mut(index)
            .ok_or(Error::Contract("software item"))
    }
    fn button(&mut self, index: usize) -> Result<&mut ButtonAction, Error> {
        self.item(index)?
            .action_mut()
            .ok_or(Error::Contract("software button"))
    }
}
impl Widget for Software {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.scroller.show(frame);
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.refresh()
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller.set_rect(self.state.rect);
        let result = self.scroller.render(frame, draw)?;
        loop {
            let next = self.changes.borrow_mut().pop_front();
            let Some(change) = next else { break };
            self.change(change)?;
        }
        Ok(result)
    }
}
