use super::*;
pub struct MultiOptionDialog {
    pub state: WidgetState,
    pub title: Label,
    pub current: String,
    pub selection: Rc<RefCell<String>>,
    pub callback: Option<Callback<DialogResult>>,
    pub cancel: Button,
    pub select: Button,
    options: Vec<String>,
    scroller: Scroller,
    pending: Rc<Cell<Option<DialogResult>>>,
}
impl MultiOptionDialog {
    pub fn new(title: &str, options: Vec<String>, current: &str) -> Self {
        let mut label = Label::new(title);
        label.size = 70.0;
        label.font = Font::Bold;
        let selection = Rc::new(RefCell::new(current.to_owned()));
        let mut scroller = Scroller {
            spacing: 25.0,
            ..Default::default()
        };
        for option in &options {
            let mut button = Button::new(option);
            button.label.horizontal = crate::text_layout::Horizontal::Left;
            button.label.padding = 50.0;
            button.label.elide = true;
            let selected = selection.clone();
            let value = option.clone();
            button.state.click = Some(Box::new(move || *selected.borrow_mut() = value.clone()));
            scroller.add(Box::new(button));
        }
        let pending = Rc::new(Cell::new(None));
        let cancel = result_button("Cancel", DialogResult::Cancel, &pending);
        let mut select = result_button("Select", DialogResult::Confirm, &pending);
        select.set_style(ButtonStyle::Primary);
        Self {
            state: WidgetState::default(),
            title: label,
            current: current.to_owned(),
            selection,
            callback: None,
            cancel,
            select,
            options,
            scroller,
            pending,
        }
    }
    pub fn set_option_font(&mut self, font: Font) {
        for item in &mut self.scroller.items {
            if let Some(button) = (item.as_mut() as &mut dyn std::any::Any).downcast_mut::<Button>()
            {
                button.label.font = font;
            }
        }
    }
}
impl Widget for MultiOptionDialog {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let outer = self.state.rect;
        let rect = Rect {
            x: outer.x + 50.0,
            y: outer.y + 50.0,
            width: outer.width - 100.0,
            height: outer.height - 100.0,
        };
        draw.rounded_segments(rect, 0.02, 20, u32::from_le_bytes([30, 30, 30, 255]), false)?;
        let content = Rect {
            x: rect.x + 50.0,
            y: rect.y + 50.0,
            width: rect.width - 100.0,
            height: rect.height - 100.0,
        };
        crate::label::gui_label(
            draw,
            Rect {
                height: 70.0,
                ..content
            },
            &self.title.text.get(),
            crate::text_layout::TextStyle {
                font: Font::Bold,
                size: 70.0,
                spacing: 0.0,
                color: self.title.color,
            },
            (
                crate::text_layout::Horizontal::Left,
                crate::text_layout::Vertical::Middle,
            ),
            true,
        )?;
        let options = Rect {
            y: content.y + 120.0,
            height: content.height - 330.0,
            ..content
        };
        for (index, option) in self.options.iter().enumerate() {
            let button = self
                .scroller
                .item_mut::<Button>(index)
                .ok_or(Error::Contract("option button missing"))?;
            button.set_style(if *option == *self.selection.borrow() {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Normal
            });
            button.set_rect(Rect {
                x: 0.0,
                y: 0.0,
                width: options.width,
                height: 135.0,
            });
        }
        self.scroller.set_rect(options);
        self.scroller.render(frame, draw)?;
        let width = (content.width - 50.0) / 2.0;
        let y = content.y + content.height - 160.0;
        self.cancel.set_rect(Rect {
            x: content.x,
            y,
            width,
            height: 160.0,
        });
        self.cancel.render(frame, draw)?;
        finish(&self.pending, &self.callback, frame);
        self.select.state.enabled = (*self.selection.borrow() != self.current).into();
        self.select.set_rect(Rect {
            x: content.x + width + 50.0,
            y,
            width,
            height: 160.0,
        });
        self.select.render(frame, draw)?;
        finish(&self.pending, &self.callback, frame);
        Ok(RenderResult::None)
    }
}
