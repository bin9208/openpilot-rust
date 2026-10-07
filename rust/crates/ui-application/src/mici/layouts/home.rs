//! Source: selfdrive/ui/mici/layouts/home.py (MIT).
use crate::{context::Context, paint, params::Read, state::messages};
use chrono::{Datelike, TimeZone};
use openpilot_ui_framework::{
    assets::Texture,
    callback::Callback,
    canvas::Canvas,
    draw::Draw,
    geometry::{Point, Rect},
    text::Font,
    text_layout::float,
    unified_label::UnifiedLabel,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
struct NetworkIcon {
    wifi: [Texture; 5],
    cell: [Texture; 5],
}
impl NetworkIcon {
    fn new(canvas: &mut Canvas) -> Result<Self, Error> {
        let texture = |canvas: &mut Canvas, name: &str, size| {
            paint::texture(
                canvas,
                &format!("icons_mici/settings/network/{name}.png"),
                size,
            )
        };
        Ok(Self {
            wifi: [
                texture(canvas, "wifi_strength_slash", (50, 44))?,
                texture(canvas, "wifi_strength_none", (50, 37))?,
                texture(canvas, "wifi_strength_low", (50, 37))?,
                texture(canvas, "wifi_strength_medium", (50, 37))?,
                texture(canvas, "wifi_strength_full", (50, 37))?,
            ],
            cell: [
                texture(canvas, "cell_strength_none", (54, 36))?,
                texture(canvas, "cell_strength_low", (54, 36))?,
                texture(canvas, "cell_strength_medium", (54, 36))?,
                texture(canvas, "cell_strength_high", (54, 36))?,
                texture(canvas, "cell_strength_full", (54, 36))?,
            ],
        })
    }
    fn paint(
        &self,
        draw: &mut dyn Draw,
        rect: Rect,
        network: u16,
        strength: u16,
    ) -> Result<(), Error> {
        let strength = if strength > 0 {
            strength.saturating_add(1).min(5)
        } else {
            0
        };
        let texture = match network {
            1 => {
                self.wifi[match strength {
                    0 => 1,
                    2 => 2,
                    3 => 3,
                    4 | 5 => 4,
                    _ => 2,
                }]
            }
            2..=5 => {
                self.cell[match strength {
                    0 => 0,
                    2 => 1,
                    3 => 2,
                    4 => 3,
                    5 => 4,
                    _ => 0,
                }]
            }
            _ => self.wifi[0],
        };
        let y = rect.y + (rect.height - texture.height) / 2.0
            - if texture.id == self.wifi[0].id {
                (self.wifi[0].height - self.wifi[1].height) / 2.0
            } else {
                0.0
            };
        texture.draw(
            draw,
            Point {
                x: rect.x + (rect.width - texture.width) / 2.0,
                y,
            },
            1.0,
            paint::color(255, 255, 255, 229),
        )
    }
}
#[derive(serde::Serialize)]
pub struct Version {
    pub version: String,
    pub branch: String,
    pub commit: String,
    pub date: String,
}
pub struct Home {
    pub state: WidgetState,
    context: Context,
    pub last_refresh: f64,
    pub version: Option<Version>,
    pub experimental: bool,
    pub address: String,
    mouse_down: Option<f64>,
    pub did_long_press: bool,
    pressed_previous: bool,
    carrot_pressed: bool,
    settings: Texture,
    carrot: Texture,
    experimental_icon: Texture,
    mic: Texture,
    network: NetworkIcon,
    pub carrot_rect: Rect,
    title: UnifiedLabel,
    version_label: UnifiedLabel,
    branch: UnifiedLabel,
    date: UnifiedLabel,
    ip: UnifiedLabel,
    pub on_settings: Option<Callback<()>>,
    pub on_carrot_web: Option<Callback<()>>,
}
impl Home {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        fn label(value: &str, size: f64, font: Font, gray: bool) -> UnifiedLabel {
            let mut l = UnifiedLabel::new(value);
            l.size = size;
            l.font = font;
            l.wrap = false;
            l.max_width = Some(480.0);
            l.state.rect = Rect {
                x: 0.0,
                y: 0.0,
                width: 480.0,
                height: float(size),
            };
            if gray {
                l.color = paint::color(130, 130, 130, 255);
            }
            l
        }
        let mut branch = label("", 30.0, Font::Regular, true);
        branch.scroll = true;
        branch.wrap = true;
        branch.max_width = None;
        Ok(Self {
            state: WidgetState::default(),
            context,
            last_refresh: 0.0,
            version: None,
            experimental: false,
            address: "Offline".into(),
            mouse_down: None,
            did_long_press: false,
            pressed_previous: false,
            carrot_pressed: false,
            settings: paint::texture(canvas, "icons_mici/settings.png", (48, 48))?,
            carrot: paint::texture(canvas, "icons/carrot_web.png", (48, 48))?,
            experimental_icon: paint::texture(
                canvas,
                "icons_mici/experimental_mode.png",
                (48, 48),
            )?,
            mic: paint::texture(canvas, "icons_mici/microphone.png", (32, 46))?,
            network: NetworkIcon::new(canvas)?,
            carrot_rect: Rect::default(),
            title: label("carrotpilot", 55.0, Font::Display, false),
            version_label: label("", 30.0, Font::Regular, false),
            branch,
            date: label("", 30.0, Font::Regular, true),
            ip: label("", 30.0, Font::Regular, true),
            on_settings: None,
            on_carrot_web: None,
        })
    }
    fn read_version(&self) -> Result<Option<Version>, crate::Error> {
        let version = self.context.params.string("Version")?;
        let branch = self.context.params.string("GitBranch")?;
        let commit = self.context.params.string("GitCommit")?;
        if version.is_empty() || branch.is_empty() || commit.is_empty() {
            return Ok(None);
        }
        let raw = self.context.params.string("GitCommitDate")?;
        let date = raw
            .trim_matches('\'')
            .split(openpilot_ui_framework::text::whitespace)
            .find(|part| !part.is_empty())
            .and_then(crate::params::typed::integer_text)
            .and_then(|v| v.parse::<i64>().ok())
            .and_then(|t| chrono::Local.timestamp_opt(t, 0).single())
            .filter(|t| (1..=9999).contains(&t.year()))
            .map(|t| format!("{}/{:02}/{:02}", t.year(), t.month(), t.day()))
            .unwrap_or_default();
        Ok(Some(Version {
            version,
            branch,
            commit: commit.chars().take(7).collect(),
            date,
        }))
    }
    fn refresh_params(&mut self) -> Result<(), crate::Error> {
        self.experimental = self.context.params.boolean("ExperimentalMode")?;
        Ok(())
    }
}
impl Widget for Home {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, _: &Frame<'_>) {
        let result = (|| -> Result<(), crate::Error> {
            self.version = self.read_version()?;
            self.refresh_params()
        })();
        if let Err(e) = result {
            self.context
                .actions
                .push(crate::context::Action::Failure(e));
        }
    }
    fn update(&mut self, frame: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let pressed = self.state.is_pressed();
        let now = (self.context.now_monotonic)();
        if pressed && !self.pressed_previous {
            self.mouse_down = Some(now);
        } else if !pressed && self.pressed_previous {
            self.mouse_down = None;
            self.did_long_press = false;
        }
        self.pressed_previous = pressed;
        if self
            .mouse_down
            .is_some_and(|start| !self.carrot_pressed && now - start > 0.5)
        {
            if self.context.ui.borrow().slow.has_longitudinal_control {
                self.experimental = !self.experimental;
                self.context
                    .params
                    .put_bool("ExperimentalMode", self.experimental)?;
            }
            self.mouse_down = None;
            self.did_long_press = true;
        }
        if frame.now - self.last_refresh > 5.0 {
            self.version = self.read_version()?;
            let address = self.context.memory.string("NetworkAddress")?;
            let address = openpilot_ui_framework::text::trim(&address);
            self.address = if address.is_empty() || address == "0.0.0.0" {
                "Offline".into()
            } else {
                address.into()
            };
            self.last_refresh = frame.now;
            self.refresh_params()?;
        }
        Ok(())
    }
    fn mouse_press(
        &mut self,
        position: Point,
        _: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        self.carrot_pressed = self.carrot_rect.contains(position);
        Ok(())
    }
    fn mouse_release(
        &mut self,
        position: Point,
        _: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        if self.carrot_pressed && self.carrot_rect.contains(position) {
            if let Some(c) = &self.on_carrot_web {
                c.call(());
            }
        } else if !self.did_long_press {
            if let Some(c) = &self.on_settings {
                c.call(());
            }
        }
        self.carrot_pressed = false;
        self.did_long_press = false;
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        let x = rect.x + 6.0;
        let y = rect.y - 5.0;
        self.title.set_position(x, y);
        self.title.render(frame, draw)?;
        if let Some(version) = &self.version {
            let release =
                openpilot_runtime_version::RELEASE_BRANCHES.contains(&version.branch.as_str());
            self.version_label.text = format!(" {}", version.version).into();
            self.version_label.set_position(
                float(f64::from(x) + self.title.text_width() + 8.0),
                y + 25.0,
            );
            self.version_label.render(frame, draw)?;
            self.branch
                .set_max_width(draw, Some(f64::from(rect.width) - 16.0));
            self.branch.text = if release {
                String::from("release").into()
            } else {
                version.branch.clone().into()
            };
            self.branch.set_position(x, y + 67.0);
            self.branch.render(frame, draw)?;
            self.date.text = if release {
                version.date.clone()
            } else {
                format!("{} ({})", version.date, version.commit)
            }
            .into();
            self.date.set_position(x, y + 103.0);
            self.date.render(frame, draw)?;
            self.ip.text = self.address.clone().into();
            self.ip.set_position(x, y + 139.0);
            self.ip.render(frame, draw)?;
        }
        let y = rect.y + rect.height - 48.0;
        let x = rect.x + 8.0;
        self.settings
            .draw(draw, Point { x, y }, 1.0, paint::color(255, 255, 255, 229))?;
        let sm = self.context.messages.borrow();
        let ds = messages::device_state(&sm.state)?;
        self.network.paint(
            draw,
            Rect {
                x: x + 66.0,
                y: y + 2.0,
                width: 54.0,
                height: 44.0,
            },
            ds.get_network_type()
                .map(u16::from)
                .unwrap_or_else(|capnp::NotInSchema(raw)| raw),
            ds.get_network_strength()
                .map(u16::from)
                .unwrap_or_else(|capnp::NotInSchema(raw)| raw),
        )?;
        self.carrot_rect = Rect {
            x: x + 138.0,
            y,
            width: 48.0,
            height: 48.0,
        };
        self.carrot.draw(
            draw,
            Point {
                x: self.carrot_rect.x,
                y,
            },
            1.0,
            paint::color(255, 255, 255, 229),
        )?;
        let mut x = x + 204.0;
        if self.experimental {
            self.experimental_icon.draw(
                draw,
                Point { x, y },
                1.0,
                paint::color(255, 255, 255, 255),
            )?;
            x += 66.0;
        }
        if self.context.ui.borrow().recording_audio {
            self.mic.draw(
                draw,
                Point { x, y: y + 1.0 },
                1.0,
                paint::color(255, 255, 255, 255),
            )?;
        }
        Ok(RenderResult::None)
    }
}
