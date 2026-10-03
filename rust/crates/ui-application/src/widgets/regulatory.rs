//! Regulatory viewers using the original big and compact HTML/scroll policies.
use crate::{context::Context, paint};
use openpilot_startup_ui::scroll::Scroll;
use openpilot_ui_framework::{
    assets::Texture,
    button::{Button, ButtonStyle},
    canvas::Canvas,
    draw::{Draw, WHITE},
    geometry::{Point, Rect},
    html::HtmlRenderer,
    navigation::NavWidget,
    scroll::ScrollPanel,
    text_layout::float,
    widget::{Frame, NavigationRequest, RenderResult, Widget, WidgetState},
    Error,
};
use std::{cell::Cell, rc::Rc};
pub struct Regulatory {
    state: WidgetState,
    content: HtmlRenderer,
    button: Button,
    legacy: Scroll,
    modern: ScrollPanel,
    logo: Option<Texture>,
    offset: Rc<Cell<f64>>,
    clicked: Rc<Cell<bool>>,
}
impl Regulatory {
    pub fn new(context: &Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let path = context.source_root.join(if context.big {
            "openpilot/selfdrive/assets/offroad/fcc.html"
        } else {
            "openpilot/selfdrive/assets/offroad/mici_fcc.html"
        });
        let content = HtmlRenderer::new(&std::fs::read_to_string(path)?, 48.0)?;
        let mut button = Button::new(context.tr("OK"));
        button.set_style(ButtonStyle::Primary);
        let clicked = Rc::new(Cell::new(false));
        let flag = clicked.clone();
        button.state.click = Some(Box::new(move || flag.set(true)));
        Ok(Self {
            state: WidgetState::default(),
            content,
            button,
            legacy: Scroll::default(),
            modern: ScrollPanel::new(false, true, !context.pc),
            logo: if context.big {
                None
            } else {
                Some(paint::texture(
                    canvas,
                    "icons_mici/settings/device/fcc_logo.png",
                    (76, 64),
                )?)
            },
            offset: Rc::default(),
            clicked,
        })
    }
    pub fn navigation(self) -> NavWidget {
        let offset = self.offset.clone();
        let mut nav = NavWidget::new(Box::new(self), 20.0, 240.0);
        nav.motion.back_area = 1.0;
        nav.back_enabled = Box::new(move || offset.get() >= -20.0);
        nav
    }
    fn big(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let scroll = Rect {
            x: rect.x + 50.0,
            y: rect.y + 50.0,
            width: rect.width - 100.0,
            height: rect.height - 280.0,
        };
        let height = self
            .content
            .total_height(draw, f64::from(scroll.width.trunc()));
        let offset = self
            .legacy
            .update(scroll, float(height), frame.events, float(frame.wheel));
        draw.scissor(Some(scroll))?;
        self.content.set_rect(Rect {
            y: scroll.y + offset,
            height: float(height),
            ..scroll
        });
        self.content.render(frame, draw)?;
        draw.scissor(None)?;
        let width = ((rect.width - 150.0) / 3.0).floor();
        self.button.set_rect(Rect {
            x: rect.x + rect.width - 50.0 - width,
            y: rect.y + rect.height - 210.0,
            width,
            height: 160.0,
        });
        self.button.render(frame, draw)?;
        if self.clicked.replace(false) {
            frame.navigation.push(NavigationRequest::Pop(None));
        }
        Ok(RenderResult::Value(-1))
    }
}
impl Widget for Regulatory {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, _: &Frame<'_>) {
        if self.logo.is_some() {
            self.modern.set_offset(0.0);
            self.offset.set(0.0);
        }
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let Some(logo) = self.logo else {
            return self.big(frame, draw);
        };
        let rect = self.state.rect;
        let height = self
            .content
            .total_height(draw, f64::from(rect.width.trunc()))
            + f64::from(logo.height)
            + 20.0;
        self.modern.enabled = self.state.enabled.get().into();
        let offset = self
            .modern
            .update(rect, height, frame.events, frame.dt)
            .round_ties_even();
        self.offset.set(self.modern.offset());
        self.content.set_rect(Rect {
            y: rect.y + float(offset) + logo.height + 20.0,
            height: float(height),
            ..rect
        });
        self.content.render(frame, draw)?;
        logo.draw(
            draw,
            Point {
                x: rect.x + 20.0,
                y: rect.y + 20.0 + float(offset),
            },
            1.0,
            WHITE,
        )?;
        Ok(RenderResult::None)
    }
}
