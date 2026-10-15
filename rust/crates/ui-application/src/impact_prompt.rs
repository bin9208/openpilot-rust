//! Global passive notice, with acknowledgement only after a successful rendered frame.
use crate::{context::Context, paint, params::Read, state::messages};
use openpilot_impact_dashcam::{COUNTDOWN_SECONDS, FEEDBACK_KEY, NOTICE_KEY, REBOOT_KEY};
use openpilot_ui_framework::{
    draw::Draw,
    geometry::{MouseEvent, Rect},
    text::Font,
    text_layout::{Horizontal, Vertical},
    Error,
};
use serde_json::json;
#[derive(Clone, Copy)]
pub struct Visibility {
    pub started: bool,
    pub alive: bool,
    pub valid: bool,
    pub received: f64,
    pub full: bool,
    pub critical: bool,
    pub rebooting: bool,
}
#[derive(Default, serde::Serialize)]
pub struct Policy {
    token: Option<String>,
    shown_since: Option<f64>,
    cancelled_token: String,
}
impl Policy {
    pub fn notice(&mut self, token: Option<&str>) {
        if self.token.as_deref() != token {
            self.shown_since = None;
        }
        self.token = token.map(str::to_owned);
    }
    pub fn touch(&mut self, started: bool, rebooting: bool, touched: bool) -> bool {
        if started && !rebooting && touched {
            if let Some(token) = &self.token {
                self.cancelled_token = token.clone();
                return true;
            }
        }
        false
    }
    pub fn present(&mut self, now: f64, input: Visibility) -> Option<f64> {
        let token = self.token.as_ref()?;
        if !input.started
            || token == &self.cancelled_token
            || !input.alive
            || !input.valid
            || now - input.received > 0.5
            || input.full
            || input.critical
        {
            return None;
        }
        let shown = *self.shown_since.get_or_insert(now);
        Some((COUNTDOWN_SECONDS - (now - shown)).ceil().max(0.0))
    }
}
pub struct Prompt {
    context: Context,
    policy: Policy,
}
impl Prompt {
    pub fn new(context: Context) -> Self {
        Self {
            context,
            policy: Policy::default(),
        }
    }
    pub fn input(&mut self, events: &mut Vec<MouseEvent>) -> Result<(), crate::Error> {
        let notice = self
            .context
            .memory
            .bytes(NOTICE_KEY)?
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .filter(serde_json::Value::is_object);
        self.policy.notice(
            notice
                .as_ref()
                .and_then(|notice| notice.get("token"))
                .and_then(serde_json::Value::as_str),
        );
        if self.policy.touch(
            self.context.ui.borrow().started,
            self.context.params.boolean(REBOOT_KEY)?,
            events
                .iter()
                .any(|event| event.down || event.pressed || event.released),
        ) {
            self.context.memory.put(
                FEEDBACK_KEY,
                json!({"token":self.policy.token,"cancel":true})
                    .to_string()
                    .as_bytes(),
            )?;
            events.clear();
        }
        Ok(())
    }
    pub fn render(
        &mut self,
        now: f64,
        width: f32,
        draw: &mut dyn Draw,
    ) -> Result<(), crate::Error> {
        let sm = self.context.messages.borrow();
        let topic = sm.state.topic("selfdriveState")?;
        let ss = messages::selfdrive_state(&sm.state)?;
        let rebooting = self.context.params.boolean(REBOOT_KEY)?;
        let input = Visibility {
            started: self.context.ui.borrow().started,
            alive: topic.alive,
            valid: topic.valid,
            received: topic.receive_time,
            full: u16::from(ss.get_alert_size()?) == 3,
            critical: u16::from(ss.get_alert_status()?) == 2,
            rebooting,
        };
        let Some(remaining) = self.policy.present(now, input) else {
            return Ok(());
        };
        let title = self.context.tr("Impact suspected");
        let (line, hint) = if rebooting {
            (
                self.context.tr("Rebooting into Dashcam Mode"),
                self.context.tr("openpilot disabled"),
            )
        } else {
            (
                self.context
                    .tr("Dashcam mode: reboot in {seconds}s")
                    .replace("{seconds}", &format!("{remaining:.0}")),
                self.context.tr("Touch anywhere to cancel"),
            )
        };
        let scale = width / 536.0;
        let rect = Rect {
            x: 8.0 * scale,
            y: 8.0 * scale,
            width: width - 16.0 * scale,
            height: 100.0 * scale,
        };
        draw.rounded_segments(rect, 0.15, 8, paint::color(165, 68, 0, 245), false)?;
        for (index, (text, size)) in [(title, 27.0), (line, 23.0), (hint, 21.0)]
            .iter()
            .enumerate()
        {
            paint::label(
                draw,
                Rect {
                    x: rect.x + 8.0 * scale,
                    y: rect.y
                        + (5.0
                            + f32::from(
                                u16::try_from(index)
                                    .map_err(|_| Error::Contract("notice label index"))?,
                            ) * 31.0)
                            * scale,
                    width: rect.width - 16.0 * scale,
                    height: 30.0 * scale,
                },
                paint::Label {
                    text,
                    font: if index == 0 { Font::Bold } else { Font::Normal },
                    size: f64::from((size * scale).trunc()),
                    color: paint::color(255, 255, 255, 255),
                    horizontal: Horizontal::Center,
                    vertical: Vertical::Middle,
                    elide: false,
                },
            )?;
        }
        if !rebooting {
            self.context.memory.put(
                FEEDBACK_KEY,
                json!({"token":self.policy.token,"visible":now})
                    .to_string()
                    .as_bytes(),
            )?;
        }
        Ok(())
    }
}
