//! Shared confirmation and option dialogs from system/ui/widgets.
use crate::{
    button::{Button, ButtonStyle},
    callback::Callback,
    draw::Draw,
    geometry::Rect,
    html::HtmlRenderer,
    keys,
    label::Label,
    scroller_tici::Scroller,
    text::Font,
    widget::{DialogResult, Frame, NavigationRequest, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
fn result_button(
    text: &str,
    result: DialogResult,
    pending: &Rc<RefCell<VecDeque<DialogResult>>>,
) -> Button {
    let mut button = Button::new(text);
    let pending = pending.clone();
    button.state.click = Some(Box::new(move || pending.borrow_mut().push_back(result)));
    button
}
fn finish(
    pending: &RefCell<VecDeque<DialogResult>>,
    callback: &Option<Callback<DialogResult>>,
    frame: &Frame<'_>,
) {
    while let Some(result) = pending.borrow_mut().pop_front() {
        let callback = callback.clone();
        frame
            .navigation
            .push(NavigationRequest::Pop(Some(Box::new(move || {
                if let Some(callback) = callback {
                    callback.call(result);
                }
            }))));
    }
}
pub struct ConfirmDialog {
    pub state: WidgetState,
    pub callback: Option<Callback<DialogResult>>,
    pub label: Label,
    pub confirm: Button,
    pub cancel: Button,
    pub rich: bool,
    scroller: Scroller,
    pending: Rc<RefCell<VecDeque<DialogResult>>>,
}
impl ConfirmDialog {
    pub fn new(text: &str, confirm: &str, cancel: &str) -> Result<Self, Error> {
        let pending = Rc::new(RefCell::new(VecDeque::new()));
        let mut label = Label::new(text);
        label.size = 70.0;
        label.font = Font::Bold;
        label.color = u32::from_le_bytes([201, 201, 201, 255]);
        let mut html = HtmlRenderer::new(text, 50.0)?;
        html.center = true;
        let mut scroller = Scroller {
            spacing: 0.0,
            ..Default::default()
        };
        scroller.add(Box::new(html));
        let cancel = result_button(cancel, DialogResult::Cancel, &pending);
        let mut confirm = result_button(confirm, DialogResult::Confirm, &pending);
        confirm.set_style(ButtonStyle::Primary);
        Ok(Self {
            state: WidgetState::default(),
            callback: None,
            label,
            confirm,
            cancel,
            rich: false,
            scroller,
            pending,
        })
    }
    pub fn set_text(&mut self, text: &str) -> Result<(), Error> {
        if self.rich {
            self.scroller
                .item_mut::<HtmlRenderer>(0)
                .ok_or(Error::Contract("dialog HTML missing"))?
                .parse(text)?;
        } else {
            self.label.text = text.to_owned().into();
        }
        Ok(())
    }
}
impl Widget for ConfirmDialog {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let margin = if self.rich { 100.0 } else { 200.0 };
        let rect = Rect {
            x: margin,
            y: margin,
            width: self.state.rect.width - 2.0 * margin,
            height: self.state.rect.height - 2.0 * margin,
        };
        draw.rounded(rect, 0.0, u32::from_le_bytes([27, 27, 27, 255]))?;
        let text = Rect {
            x: rect.x + 50.0,
            y: rect.y + 10.0,
            width: rect.width - 100.0,
            height: rect.height - 230.0,
        };
        if self.rich {
            let html = self
                .scroller
                .item_mut::<HtmlRenderer>(0)
                .ok_or(Error::Contract("dialog HTML missing"))?;
            let height =
                crate::text_layout::float(html.total_height(draw, f64::from(text.width).trunc()));
            html.set_rect(Rect { height, ..text });
            self.scroller.set_rect(text);
            self.scroller.render(frame, draw)?;
        } else {
            self.label.set_rect(text);
            self.label.render(frame, draw)?;
        }
        if frame.keyboard.pressed.contains(&keys::ENTER) {
            self.pending.borrow_mut().push_back(DialogResult::Confirm);
        } else if frame.keyboard.pressed.contains(&keys::ESCAPE) {
            self.pending.borrow_mut().push_back(DialogResult::Cancel);
        }
        finish(&self.pending, &self.callback, frame);
        let width = ((rect.width - 150.0) / 2.0).floor();
        let y = rect.y + rect.height - 210.0;
        if self.cancel.label.text.get().is_empty() {
            self.confirm.set_rect(Rect {
                x: rect.x + 50.0,
                y,
                width: rect.width - 100.0,
                height: 160.0,
            });
            self.confirm.render(frame, draw)?;
            finish(&self.pending, &self.callback, frame);
        } else {
            self.confirm.set_rect(Rect {
                x: rect.x + rect.width - width - 50.0,
                y,
                width,
                height: 160.0,
            });
            self.confirm.render(frame, draw)?;
            finish(&self.pending, &self.callback, frame);
            self.cancel.set_rect(Rect {
                x: rect.x + 50.0,
                y,
                width,
                height: 160.0,
            });
            self.cancel.render(frame, draw)?;
            finish(&self.pending, &self.callback, frame);
        }
        Ok(RenderResult::None)
    }
}

mod option;
pub use option::MultiOptionDialog;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        geometry::{MouseEvent, Point},
        keys::KeyboardInput,
        widget::NavigationQueue,
    };

    #[test]
    fn confirmation_retains_each_release_and_result_order() {
        for count in [2, 3] {
            let mut dialog = ConfirmDialog::new("Question", "Continue", "Cancel").unwrap();
            let observed = Rc::new(RefCell::new(Vec::new()));
            let calls = observed.clone();
            dialog.callback = Some(Callback::new(move |result| calls.borrow_mut().push(result)));
            for _ in 0..count {
                dialog.confirm.state.release(0.0);
            }
            for _ in 0..count {
                dialog.cancel.state.release(0.0);
            }
            let keyboard = KeyboardInput::default();
            let navigation = NavigationQueue::default();
            let frame = Frame {
                index: 0,
                now: 0.0,
                monotonic: 0.0,
                keyboard: &keyboard,
                navigation: &navigation,
                dt: 0.05,
                target_fps: 20.0,
                awake: true,
                events: &[],
                last_event: MouseEvent::default(),
                cursor: Point::default(),
                wheel: 0.0,
                show_touches: false,
            };
            finish(&dialog.pending, &dialog.callback, &frame);
            assert!(observed.borrow().is_empty());
            let mut pops = 0;
            while let Some(request) = navigation.pop() {
                let NavigationRequest::Pop(Some(callback)) = request else {
                    panic!("expected callback pop")
                };
                pops += 1;
                callback();
            }
            assert_eq!(pops, count * 2);
            let expected = std::iter::repeat_n(DialogResult::Confirm, count)
                .chain(std::iter::repeat_n(DialogResult::Cancel, count))
                .collect::<Vec<_>>();
            assert_eq!(*observed.borrow(), expected);
            finish(&dialog.pending, &dialog.callback, &frame);
            assert!(navigation.pop().is_none());
        }
    }
}
