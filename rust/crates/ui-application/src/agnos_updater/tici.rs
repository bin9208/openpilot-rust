use super::view::{Action, Shared};
use crate::paint;
use openpilot_ui_framework::{
    button::{Button, ButtonStyle},
    canvas::Canvas,
    draw::Draw,
    geometry::Rect,
    label::gui_label,
    network::{Context, WifiManagerUi},
    text::Font,
    text_layout::{float, Horizontal, TextStyle, Vertical},
    unified_label::UnifiedLabel,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::Cell, rc::Rc};
const MARGIN: f32 = 50.0;
const BUTTON_HEIGHT: f32 = 160.0;
const BUTTON_WIDTH: f32 = 400.0;
const PROGRESS_HEIGHT: f32 = 72.0;
const BODY_SIZE: f64 = 65.0;
const PROGRESS_BACKGROUND: u32 = paint::color(41, 41, 41, 255);
const PROGRESS_COLOR: u32 = paint::color(54, 77, 239, 255);

pub struct View {
    state: WidgetState,
    shared: Rc<Shared>,
    pub wifi_screen: Rc<Cell<bool>>,
    wifi: WifiManagerUi,
    wifi_button: Button,
    back_button: Button,
    retry_button: Button,
    reboot_button: Button,
    detail: UnifiedLabel,
}
impl View {
    pub fn new(shared: Rc<Shared>, context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let wifi_screen = Rc::new(Cell::new(false));
        let mut wifi_button = Button::new("Connect to Wi-Fi");
        let screen = wifi_screen.clone();
        wifi_button.state.click = Some(Box::new(move || screen.set(true)));
        let mut back_button = Button::new("Back");
        let screen = wifi_screen.clone();
        back_button.state.click = Some(Box::new(move || screen.set(false)));
        let mut retry_button = Button::new("Retry");
        retry_button.set_style(ButtonStyle::Primary);
        let model = shared.clone();
        let screen = wifi_screen.clone();
        retry_button.state.click = Some(Box::new(move || {
            screen.set(false);
            model.action(Action::Retry);
        }));
        let mut reboot_button = Button::new("Reboot");
        let model = shared.clone();
        reboot_button.state.click = Some(Box::new(move || model.action(Action::Reboot)));
        let wifi = WifiManagerUi::new(context, |path, size| paint::texture(canvas, path, size))?;
        let mut detail = UnifiedLabel::new("");
        detail.size = BODY_SIZE;
        detail.line_height = 1.0 / 0.9;
        detail.elide = false;
        Ok(Self {
            state: WidgetState::default(),
            shared,
            wifi_screen,
            wifi,
            wifi_button,
            back_button,
            retry_button,
            reboot_button,
            detail,
        })
    }
}
impl Widget for View {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        if self.wifi_screen.get() {
            let panel = Rect {
                x: rect.x + MARGIN,
                y: rect.y + MARGIN,
                width: rect.width - MARGIN * 2.0,
                height: rect.height - BUTTON_HEIGHT - MARGIN * 3.0,
            };
            draw.rounded_segments(panel, 0.035, 10, paint::color(51, 51, 51, 255), false)?;
            self.wifi.set_rect(Rect {
                x: panel.x + 50.0,
                width: panel.width - 100.0,
                ..panel
            });
            self.wifi.render(frame, draw)?;
            self.back_button.set_rect(Rect {
                x: MARGIN,
                y: rect.height - MARGIN - BUTTON_HEIGHT,
                width: BUTTON_WIDTH,
                height: BUTTON_HEIGHT,
            });
            self.back_button.render(frame, draw)?;
            return Ok(RenderResult::None);
        }
        let state = self.shared.state.borrow();
        let title = Rect {
            x: MARGIN + 100.0,
            y: 330.0,
            width: rect.width - MARGIN * 2.0 - 200.0,
            height: 100.0,
        };
        gui_label(
            draw,
            title,
            &state.text,
            TextStyle {
                font: Font::SemiBold,
                size: 90.0,
                spacing: 0.0,
                color: paint::color(255, 255, 255, 229),
            },
            (Horizontal::Left, Vertical::Middle),
            true,
        )?;
        let bar = Rect {
            y: 530.0,
            height: PROGRESS_HEIGHT,
            ..title
        };
        draw.rounded_segments(bar, 0.5, 10, PROGRESS_BACKGROUND, false)?;
        if state.progress > 0 {
            draw.rounded_segments(
                Rect {
                    width: bar.width * f32::from(state.progress) / 100.0,
                    ..bar
                },
                0.5,
                10,
                PROGRESS_COLOR,
                false,
            )?;
        }
        let bottom = rect.height - MARGIN - BUTTON_HEIGHT;
        if let Some(failure) = &state.failure {
            self.detail.text = failure.clone().into();
            self.detail.set_rect(Rect {
                x: MARGIN + 100.0,
                y: 650.0,
                width: title.width,
                height: float(BODY_SIZE * draw.font_scale() * 2.0),
            });
            self.detail.render(frame, draw)?;
            let width = (rect.width - MARGIN * 4.0) / 3.0;
            self.retry_button.set_rect(Rect {
                x: MARGIN,
                y: bottom,
                width,
                height: BUTTON_HEIGHT,
            });
            self.wifi_button.set_rect(Rect {
                x: MARGIN * 2.0 + width,
                y: bottom,
                width,
                height: BUTTON_HEIGHT,
            });
            self.reboot_button.set_rect(Rect {
                x: MARGIN * 3.0 + width * 2.0,
                y: bottom,
                width,
                height: BUTTON_HEIGHT,
            });
            self.retry_button.render(frame, draw)?;
            self.wifi_button.render(frame, draw)?;
            self.reboot_button.render(frame, draw)?;
        } else {
            self.wifi_button.set_rect(Rect {
                x: MARGIN,
                y: bottom,
                width: (rect.width - MARGIN * 3.0) / 2.0,
                height: BUTTON_HEIGHT,
            });
            self.wifi_button.render(frame, draw)?;
        }
        Ok(RenderResult::None)
    }
}
