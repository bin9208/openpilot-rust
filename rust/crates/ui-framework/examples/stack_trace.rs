use openpilot_ui_framework::{
    draw::Draw,
    stack::NavigationStack,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use serde::Deserialize;
use std::{cell::RefCell, rc::Rc};
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Operation {
    Push { id: usize },
    Pop { index: Option<usize> },
    PopTo { id: usize, instant: bool },
}
struct Probe {
    id: usize,
    state: WidgetState,
    events: Rc<RefCell<Vec<String>>>,
}
impl Widget for Probe {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<RenderResult, Error> {
        Ok(RenderResult::None)
    }
    fn show(&mut self, _: &Frame<'_>) {
        self.events.borrow_mut().push(format!("show:{}", self.id));
    }
    fn hide(&mut self, _: &Frame<'_>) {
        self.events.borrow_mut().push(format!("hide:{}", self.id));
    }
    fn dismiss_navigation(&mut self, callback: Option<Box<dyn FnOnce()>>, frame: &Frame<'_>) {
        self.events
            .borrow_mut()
            .push(format!("dismiss:{}", self.id));
        frame.navigation.push(NavigationRequest::Pop(callback));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let operations: Vec<Operation> = serde_json::from_reader(std::io::stdin())?;
    let events = Rc::new(RefCell::new(Vec::new()));
    let widgets: Vec<_> = (0..4)
        .map(|id| {
            WidgetHandle::new(Probe {
                id,
                state: WidgetState::default(),
                events: events.clone(),
            })
        })
        .collect();
    let mut stack = NavigationStack::default();
    let frame = Frame {
        now: 0.0,
        monotonic: 0.0,
        keyboard: &Default::default(),
        navigation: &Default::default(),
        dt: 0.05,
        target_fps: 20.0,
        awake: true,
        events: &[],
        last_event: Default::default(),
        cursor: Default::default(),
        wheel: 0.0,
        show_touches: false,
    };
    let mut results = Vec::new();
    for operation in operations {
        match operation {
            Operation::Push { id } => stack.push(widgets[id].clone(), &frame)?,
            Operation::Pop { index } => stack.pop(index, &frame)?,
            Operation::PopTo { id, instant } => {
                let log = events.clone();
                stack.pop_to(
                    &widgets[id],
                    instant,
                    Some(Box::new(move || log.borrow_mut().push("callback".into()))),
                    &frame,
                )?;
            }
        }
        stack.process(&frame)?;
        let active = stack
            .active()
            .and_then(|value| widgets.iter().position(|widget| widget.same(&value)));
        let enabled: Vec<_> = widgets
            .iter()
            .map(|widget| widget.borrow().map(|value| value.state().enabled.get()))
            .collect::<Result<_, _>>()?;
        results.push(serde_json::json!({"active":active,"enabled":enabled,"contains":widgets.iter().map(|widget|stack.contains(widget)).collect::<Vec<_>>(),"events":*events.borrow(),"len":stack.len()}));
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
