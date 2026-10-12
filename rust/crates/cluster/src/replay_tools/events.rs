use super::{Layout, Rect, ToolCommand, ToolsState};
use openpilot_sdl2::gui::Event;
#[derive(Debug, PartialEq)]
pub enum UiAction {
    Command(ToolCommand),
    ChooseLog,
}
#[derive(Default)]
pub struct InputState {
    dragging_seek: bool,
}
impl InputState {
    #[must_use]
    pub const fn dragging_seek(&self) -> bool {
        self.dragging_seek
    }
    pub fn handle(
        &mut self,
        event: Event,
        layout: &Layout,
        state: &mut ToolsState,
    ) -> Option<UiAction> {
        let seek = |x| {
            UiAction::Command(ToolCommand::Seek {
                value: super::seek_value(x, layout.seek, state.duration_s),
            })
        };
        match event {
            Event::Quit | Event::WindowEvent { event: 14, .. } => {
                Some(UiAction::Command(ToolCommand::Closed))
            }
            Event::KeyDown {
                key: 32,
                repeat: false,
            } => Some(UiAction::Command(ToolCommand::TogglePause)),
            Event::KeyDown { key, repeat: false }
                if key == 1_073_741_904 || key == 1_073_741_903 =>
            {
                Some(UiAction::Command(ToolCommand::Seek {
                    value: (state.playback_s + if key == 1_073_741_904 { -5. } else { 5. })
                        .min(state.duration_s)
                        .max(0.),
                }))
            }
            Event::MouseDown { x, y, button: 1 } => {
                if layout.open.contains(x, y) {
                    Some(UiAction::ChooseLog)
                } else if layout.previous.contains(x, y) {
                    Some(UiAction::Command(ToolCommand::PreviousLog))
                } else if layout.pause.contains(x, y) {
                    Some(UiAction::Command(ToolCommand::TogglePause))
                } else if layout.next.contains(x, y) {
                    Some(UiAction::Command(ToolCommand::NextLog))
                } else if Rect::new(
                    layout.seek.x,
                    layout.seek.y - 10,
                    layout.seek.width,
                    layout.seek.height + 20,
                )
                .contains(x, y)
                {
                    self.dragging_seek = true;
                    Some(seek(x))
                } else if let Some((name, _)) =
                    layout.options.iter().find(|(_, rect)| rect.contains(x, y))
                {
                    let value = !state.options.get(*name).copied().unwrap_or(false);
                    state.options.insert((*name).into(), value);
                    Some(UiAction::Command(ToolCommand::Option {
                        name: (*name).into(),
                        value,
                    }))
                } else {
                    Some(UiAction::Command(ToolCommand::TogglePause))
                }
            }
            Event::MouseUp { x, button: 1, .. } => {
                let action = self.dragging_seek.then(|| seek(x));
                self.dragging_seek = false;
                action
            }
            Event::MouseMove { x, .. } if self.dragging_seek => Some(seek(x)),
            Event::WindowEvent { .. }
            | Event::KeyDown { .. }
            | Event::MouseDown { .. }
            | Event::MouseUp { .. }
            | Event::MouseMove { .. }
            | Event::Other(_) => None,
        }
    }
}
