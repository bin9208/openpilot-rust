use super::assets::Assets;
use crate::{
    context::{Action, Context},
    mici::widgets::dialog::Confirmation,
};
use openpilot_ui_framework::{
    assets::Texture,
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::rc::Rc;
pub(super) struct Forget {
    pub state: WidgetState,
    context: Context,
    callback: Rc<dyn Fn()>,
    backgrounds: [Texture; 2],
    trash: Texture,
    confirm_icon: Texture,
}
impl Forget {
    pub fn new(context: Context, callback: Rc<dyn Fn()>, assets: &Assets) -> Result<Self, Error> {
        let mut state = WidgetState::default();
        state.rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 108.0,
            height: 108.0,
        };
        Ok(Self {
            state,
            context,
            callback,
            backgrounds: [
                assets.get(
                    "icons_mici/settings/network/new/forget_button.png",
                    (84, 84),
                )?,
                assets.get(
                    "icons_mici/settings/network/new/forget_button_pressed.png",
                    (84, 84),
                )?,
            ],
            trash: assets.get("icons_mici/settings/network/new/trash.png", (29, 35))?,
            confirm_icon: assets.get("icons_mici/settings/network/new/trash.png", (54, 64))?,
        })
    }
}
impl Widget for Forget {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn mouse_release(
        &mut self,
        _: Point,
        frame: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.state.release(frame.now);
        self.context.actions.push(Action::MiciConfirm(Confirmation {
            title: "slide to forget".into(),
            icon: self.confirm_icon,
            callback: self.callback.clone(),
            red: true,
            exit_on_confirm: true,
        }));
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        for icon in [
            self.backgrounds[usize::from(self.state.is_pressed())],
            self.trash,
        ] {
            icon.draw(
                draw,
                Point {
                    x: float(
                        f64::from(rect.x) + (f64::from(rect.width) - f64::from(icon.width)) / 2.0,
                    ),
                    y: float(
                        f64::from(rect.y) + (f64::from(rect.height) - f64::from(icon.height)) / 2.0,
                    ),
                },
                1.0,
                WHITE,
            )?;
        }
        Ok(RenderResult::None)
    }
}
