use super::Preview;
use crate::context::{Context, Event};
use crate::onroad::driver_camera::Navigation;
use openpilot_ui_framework::{
    callback::Callback,
    canvas::Canvas,
    draw::Draw,
    geometry::Point,
    navigation::NavWidget,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
pub struct Dialog {
    preview: Preview,
    context: Context,
    navigation: Navigation,
    callback: Callback<()>,
}
pub fn dialog(context: Context, canvas: &mut Canvas) -> Result<NavWidget, Error> {
    with_camera(context, canvas, "camerad")
}
pub fn with_camera(context: Context, canvas: &mut Canvas, name: &str) -> Result<NavWidget, Error> {
    let preview = Preview::with_camera(context.clone(), canvas, name, 20.0, false)?;
    let navigation = Navigation::default();
    let pending = navigation.clone();
    let callback = Callback::new(move |()| pending.pop());
    context.listen(Event::InteractiveTimeout, callback.clone());
    Ok(NavWidget::new(
        Box::new(Dialog {
            preview,
            context,
            navigation,
            callback,
        }),
        20.0,
        240.0,
    ))
}
impl Dialog {
    pub fn is_rhd(&self) -> bool {
        self.preview.is_rhd()
    }
    pub fn has_frame(&self) -> bool {
        self.preview.has_frame()
    }
}
impl Drop for Dialog {
    fn drop(&mut self) {
        self.context
            .callbacks
            .borrow_mut()
            .retain(|(_, callback)| !callback.same(&self.callback));
    }
}
impl Widget for Dialog {
    fn state(&self) -> &WidgetState {
        self.preview.state()
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        self.preview.state_mut()
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.navigation.bind(frame);
        self.preview.show(frame);
    }
    fn hide(&mut self, frame: &Frame<'_>) {
        self.preview.hide(frame);
    }
    fn update(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<(), Error> {
        self.navigation.bind(frame);
        self.preview.update(frame, draw)
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.preview.paint(frame, draw)
    }
    fn mouse_release(
        &mut self,
        point: Point,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.preview.mouse_release(point, frame, draw)
    }
}
