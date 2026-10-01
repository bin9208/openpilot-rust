use crate::{
    draw::Draw,
    geometry::Rect,
    widget::{Frame, NavigationRequest, WidgetHandle},
    Error,
};
pub struct NavigationStack {
    pub(crate) widgets: Vec<WidgetHandle>,
    pub render_depth: usize,
    pub close_requested: bool,
}
impl Default for NavigationStack {
    fn default() -> Self {
        Self {
            widgets: Vec::new(),
            render_depth: 2,
            close_requested: false,
        }
    }
}
impl NavigationStack {
    pub fn len(&self) -> usize {
        self.widgets.len()
    }
    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty()
    }
    pub fn active(&self) -> Option<WidgetHandle> {
        self.widgets.last().cloned()
    }
    pub fn contains(&self, widget: &WidgetHandle) -> bool {
        self.widgets.iter().any(|value| value.same(widget))
    }
    pub fn push(&mut self, widget: WidgetHandle, frame: &Frame<'_>) -> Result<(), Error> {
        if self.contains(&widget) {
            eprintln!("Widget already in stack, cannot push again!");
            return Ok(());
        }
        if let Some(previous) = self.widgets.last() {
            previous.borrow_mut()?.state_mut().enabled = false.into();
        }
        self.widgets.push(widget.clone());
        let mut widget = widget.borrow_mut()?;
        widget.show(frame);
        widget.state_mut().enabled = true.into();
        Ok(())
    }
    pub fn pop(&mut self, index: Option<usize>, frame: &Frame<'_>) -> Result<(), Error> {
        if self.widgets.len() < 2 {
            eprintln!("At least one widget should remain on the stack, ignoring pop!");
            return Ok(());
        }
        let index = index.unwrap_or(self.widgets.len() - 1);
        if index == 0 || index >= self.widgets.len() {
            eprintln!("Invalid index {index} to pop, ignoring!");
            return Ok(());
        }
        if index == self.widgets.len() - 1 {
            self.widgets[index - 1].borrow_mut()?.state_mut().enabled = true.into();
        }
        self.widgets.remove(index).borrow_mut()?.hide(frame);
        Ok(())
    }
    pub fn pop_to(
        &mut self,
        target: &WidgetHandle,
        instant: bool,
        callback: Option<Box<dyn FnOnce()>>,
        frame: &Frame<'_>,
    ) -> Result<(), Error> {
        if !self.contains(target) {
            eprintln!("Widget not in stack, cannot pop to it!");
            return Ok(());
        }
        let top = self
            .active()
            .ok_or(Error::Contract("navigation stack unexpectedly empty"))?;
        if top.same(target) {
            if let Some(callback) = callback {
                callback();
            }
            return Ok(());
        }
        while self.widgets.len() > 1 && !self.widgets[self.widgets.len() - 2].same(target) {
            self.pop(Some(self.widgets.len() - 2), frame)?;
        }
        if instant {
            self.pop(None, frame)?;
        } else {
            top.borrow_mut()?.dismiss_navigation(callback, frame);
        }
        Ok(())
    }
    pub fn process(&mut self, frame: &Frame<'_>) -> Result<(), Error> {
        while let Some(request) = frame.navigation.pop() {
            match request {
                NavigationRequest::Close => self.close_requested = true,
                NavigationRequest::Push(widget) => self.push(widget, frame)?,
                NavigationRequest::Pop(callback) => {
                    self.pop(None, frame)?;
                    if let Some(callback) = callback {
                        callback();
                    }
                }
                NavigationRequest::PopAt(index) => self.pop(Some(index), frame)?,
                NavigationRequest::PopTo {
                    target,
                    instant,
                    callback,
                } => self.pop_to(&target, instant, callback, frame)?,
            }
        }
        Ok(())
    }
    pub fn render(
        &mut self,
        frame: &Frame<'_>,
        rect: Rect,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        let start = self.widgets.len().saturating_sub(self.render_depth);
        let visible = self.widgets[start..].to_vec();
        for handle in visible {
            {
                let mut widget = handle.borrow_mut()?;
                widget.set_rect(rect);
                widget.render(frame, draw)?;
                if let Some(request) = widget.take_navigation() {
                    frame.navigation.push(request);
                }
            }
            self.process(frame)?;
        }
        Ok(())
    }
}
