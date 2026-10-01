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
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
fn result_button(
    text: &str,
    result: DialogResult,
    pending: &Rc<Cell<Option<DialogResult>>>,
) -> Button {
    let mut button = Button::new(text);
    let pending = pending.clone();
    button.state.click = Some(Box::new(move || pending.set(Some(result))));
    button
}
fn finish(
    pending: &Cell<Option<DialogResult>>,
    callback: &Option<Callback<DialogResult>>,
    frame: &Frame<'_>,
) {
    if let Some(result) = pending.take() {
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
    pending: Rc<Cell<Option<DialogResult>>>,
}
impl ConfirmDialog {
    pub fn new(text: &str, confirm: &str, cancel: &str) -> Result<Self, Error> {
        let pending = Rc::new(Cell::new(None));
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
            self.pending.set(Some(DialogResult::Confirm));
        } else if frame.keyboard.pressed.contains(&keys::ESCAPE) {
            self.pending.set(Some(DialogResult::Cancel));
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
