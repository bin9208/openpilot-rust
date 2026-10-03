mod support;
use openpilot_ui_framework::{
    draw::Draw,
    geometry::{MouseEvent, Rect},
    scroller::Scroller,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use serde::Deserialize;
use std::{cell::RefCell, rc::Rc};
#[derive(Deserialize)]
struct Input {
    horizontal: bool,
    snap: bool,
    frames: Vec<InputFrame>,
}
#[derive(Deserialize)]
struct InputFrame {
    now: f64,
    events: Vec<MouseEvent>,
    show: bool,
    enabled: bool,
    visible: Vec<bool>,
    scroll: Option<(f64, bool, bool, bool)>,
    movement: Option<(usize, usize)>,
}
#[derive(Default)]
struct Empty(WidgetState);
impl Widget for Empty {
    fn state(&self) -> &WidgetState {
        &self.0
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.0
    }
    fn paint(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<RenderResult, Error> {
        Ok(RenderResult::None)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Input = serde_json::from_reader(std::io::stdin())?;
    let mut scroller = Scroller::new(input.horizontal, input.snap, false, 20.0);
    scroller.edge_shadows = false;
    scroller.set_rect(Rect {
        x: 10.0,
        y: 10.0,
        width: 516.0,
        height: 220.0,
    });
    let counts = Rc::new(RefCell::new(vec![0; 4]));
    for (index, width) in [140.0, 190.0, 90.0, 240.0].into_iter().enumerate() {
        let mut item = Empty::default();
        item.0.rect = Rect {
            x: 0.0,
            y: 0.0,
            width,
            height: 140.0,
        };
        let counts = counts.clone();
        item.0.click = Some(Box::new(move || counts.borrow_mut()[index] += 1));
        scroller.add(Box::new(item))?;
    }
    let mut last = MouseEvent::default();
    let mut output = Vec::new();
    for input in input.frames {
        if let Some(event) = input.events.last() {
            last = *event;
        }
        let frame = Frame {
            index: 0,
            now: input.now,
            monotonic: 0.0,
            keyboard: &Default::default(),
            navigation: &Default::default(),
            dt: 0.05,
            target_fps: 20.0,
            awake: true,
            events: &input.events,
            last_event: last,
            cursor: openpilot_ui_framework::geometry::Point::default(),
            wheel: 0.0,
            show_touches: false,
        };
        scroller.state.enabled = input.enabled.into();
        for (index, visible) in input.visible.into_iter().enumerate() {
            scroller
                .item_mut(index)
                .ok_or("item missing")?
                .state_mut()
                .visible = visible.into();
        }
        if input.show {
            scroller.show(&frame);
        }
        if let Some((from, to)) = input.movement {
            scroller.move_item(from, to)?;
        }
        if let Some((position, smooth, interrupt, widget)) = input.scroll {
            scroller.scroll_to(position, smooth, interrupt, widget)?;
        }
        scroller.render(&frame, &mut support::NoDraw)?;
        let items: Vec<_> = (0..scroller.len())
            .map(|index| {
                let rect = scroller.item(index).expect("fixture item").state().rect;
                serde_json::json!({"id":scroller.item_id(index),"x":rect.x,"y":rect.y})
            })
            .collect();
        output.push(serde_json::json!({"offset":scroller.panel.offset(),"velocity":scroller.panel.velocity,"state":scroller.panel.state,"content":scroller.content_size,"auto":scroller.auto_scrolling(),"moving":scroller.moving_items(),"items":items,"clicks":*counts.borrow()}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
