use super::*;
impl BigButton {
    pub(super) fn paint_button(
        &mut self,
        frame: &Frame<'_>,
        draw: &mut dyn Draw,
    ) -> Result<(), Error> {
        if let Some(position) = self.position_base {
            self.state.rect.x = float(f64::from(position.x) + self.shake(frame.now));
            self.state.rect.y = position.y;
        }
        if matches!(self.kind, Kind::Grey) {
            draw.rounded_segments(
                self.state.rect,
                0.4,
                10,
                u32::from_le_bytes([255, 255, 255, 38]),
                false,
            )?;
            return self.content(frame, draw, f64::from(self.state.rect.y));
        }
        if self.grow_until.is_some_and(|until| frame.now >= until) {
            self.grow_until = None;
        }
        let rect = self.state.rect;
        let pressed = self.state.is_pressed();
        let enabled = self.state.enabled.get();
        let background = self.backgrounds[if !enabled { 2 } else { usize::from(pressed) }];
        let scale = self.scale.update(if pressed || self.grow_until.is_some() {
            1.07
        } else {
            1.0
        });
        let x = f64::from(rect.x) + f64::from(rect.width) * (1.0 - scale) / 2.0;
        let y = f64::from(rect.y) + f64::from(rect.height) * (1.0 - scale) / 2.0;
        if self.scroll {
            draw.rounded_segments(
                Rect {
                    x: float(x),
                    y: float(y),
                    width: float(f64::from(rect.width) * scale),
                    height: float(f64::from(rect.height) * scale),
                },
                0.4,
                7,
                u32::from_le_bytes([0, 0, 0, 127]),
                false,
            )?;
            self.content(frame, draw, y)?;
            background.draw(
                draw,
                Point {
                    x: float(x),
                    y: float(y),
                },
                float(scale),
                WHITE,
            )?;
        } else {
            background.draw(
                draw,
                Point {
                    x: float(x),
                    y: float(y),
                },
                float(scale),
                WHITE,
            )?;
            self.content(frame, draw, y)?;
        }
        Ok(())
    }
    fn content(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw, y: f64) -> Result<(), Error> {
        let grey = matches!(self.kind, Kind::Grey);
        let rect = self.state.rect;
        let vertical = if grey && self.text.is_empty() {
            18.0
        } else {
            23.0
        };
        let horizontal = if grey { 30.0 } else { 40.0 };
        let width = self.width_hint();
        self.label.text = self.text.clone().into();
        self.label.size = self.font_size.unwrap_or(if grey {
            36.0
        } else if self.text.chars().count() <= 18 {
            48.0
        } else {
            42.0
        });
        self.label.scroll = self.scroll;
        self.label.vertical = if self.value.is_empty() {
            Vertical::Bottom
        } else {
            Vertical::Top
        };
        self.label.color = u32::from_le_bytes([
            255,
            255,
            255,
            if self.state.enabled.get() { 229 } else { 89 },
        ]);
        self.label.set_rect(Rect {
            x: float(f64::from(rect.x) + horizontal),
            y: float(y + vertical),
            width: float(width),
            height: float(f64::from(rect.height) - vertical * 2.0),
        });
        self.label.render(frame, draw)?;
        if !self.value.is_empty() {
            self.sub_label.text = self.value.clone().into();
            self.sub_label.vertical = if grey && self.text.is_empty() {
                Vertical::Middle
            } else {
                Vertical::Bottom
            };
            let label_y = y + vertical + self.label.content_height(draw, width);
            self.sub_label.set_rect(Rect {
                x: float(f64::from(rect.x) + horizontal),
                y: float(label_y),
                width: float(width),
                height: float(y + f64::from(rect.height) - vertical - label_y),
            });
            self.sub_label.render(frame, draw)?;
        }
        if let Some(icon) = self.icon {
            let rotation = self
                .rotate_since
                .map_or(0.0, |time| (frame.now - time) * 180.0);
            draw.image(ImageDraw {
                id: icon.id,
                source: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: icon.width,
                    height: icon.height,
                },
                destination: Rect {
                    x: rect.x + rect.width - 30.0 - icon.width / 2.0,
                    y: float(y + 30.0 + f64::from(icon.height) / 2.0),
                    width: icon.width,
                    height: icon.height,
                },
                origin: Point {
                    x: icon.width / 2.0,
                    y: icon.height / 2.0,
                },
                rotation: float(rotation),
                tint: u32::from_le_bytes([255, 255, 255, 229]),
            })?;
        }
        match &self.kind {
            Kind::Toggle(checked) => self.pills[usize::from(*checked)].draw(
                draw,
                Point {
                    x: rect.x + rect.width - self.pills[1].width,
                    y: float(y),
                },
                1.0,
                WHITE,
            )?,
            Kind::Multiple { options, .. } => {
                let selected = options
                    .iter()
                    .position(|v| v == &self.value)
                    .ok_or(Error::Contract("multi-toggle selection missing"))?;
                use num_traits::ToPrimitive;
                for index in 0..options.len() {
                    self.pills[usize::from(index == selected)].draw(
                        draw,
                        Point {
                            x: rect.x + rect.width - self.pills[1].width,
                            y: float(
                                y + 35.0
                                    * index
                                        .to_f64()
                                        .ok_or(Error::Contract("toggle index overflow"))?,
                            ),
                        },
                        1.0,
                        WHITE,
                    )?;
                }
            }
            Kind::Button | Kind::Grey => {}
        }
        Ok(())
    }
}
