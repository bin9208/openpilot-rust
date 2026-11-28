//! Read-only, bounded automatic-indicator presentation shared by all
//! displays, and the persistent Carrot-B1 link badge. Port of carrot's
//! `common/indicator_display.py` and `selfdrive/ui/b1_badge.py`.
use crate::paint::color;
use openpilot_ui_framework::{
    draw::{Draw, TextDraw},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{float, measure},
    Error,
};
use serde_json::Value;
use std::{cell::RefCell, io::Read, path::Path};

pub const PATH: &str = "/dev/shm/carrot-indicator-display.json";
const MAX_BYTES: u64 = 1024;
const MAX_AGE: f64 = 0.6;

fn origin(key: &str) -> Option<&'static str> {
    Some(match key {
        "turn" => "TBT 턴",
        "lane" => "ATC 차선변경",
        "tbt" => "250m 우측",
        "manual" => "운전자 차선접근",
        "manual_turn" => "운전자 수동 턴",
        "model_path" => "모델 경로",
        "model_desire" => "모델 차선변경",
        "model_turn" => "모델 턴",
        "bluetooth" => "BT 리모컨",
        _ => return None,
    })
}

fn reason(key: &str) -> Option<&'static str> {
    Some(match key {
        "confirming" => "조건 확인 중",
        "approach" => "턴 지점 접근",
        "turning" => "턴 진행 중",
        "boundary" => "차선 경계 접근 중",
        "waiting_conditions" => "차선·주변 조건 대기",
        "front" => "전방 거리·상대속도 조건 대기",
        "bsd" => "우측 BSD 감지",
        "admission" => "송신 조건 확인 중",
        "inputs" => "입력 상태 대기",
        "peer" => "수신 장치 확인 대기",
        "completed" => "완료",
        "returned" => "차선 안으로 복귀",
        "geometry" => "차선 위치 불확실",
        "relabel" => "차선 위치 변경",
        "driver_opposite" => "운전자 반대 조향",
        "driver" | "driver_override" => "운전자 조작",
        "route_or_distance" => "경로·거리 조건 종료",
        "route" => "경로 조건 종료",
        "timeout" => "유지 시간 종료",
        "cancelled" | "ended" => "요청 종료",
        "disabled" => "기능 꺼짐",
        "start" => "시작 조건 종료",
        "atc" => "다른 ATC 동작",
        "turn_started" => "턴 시작",
        "maneuver" => "차선변경 조건 종료",
        "interrupted" => "차선변경 중단",
        "route_jump" => "경로 거리 변경",
        "invalid" => "입력 무효",
        "active" => "유지 중",
        "waiting" => "시작 대기",
        "changing" => "차선변경 중",
        "finishing" => "차선변경 마무리",
        "trigger" => "조건 충족",
        _ => return None,
    })
}

fn state_label(key: &str) -> Option<(&'static str, (u8, u8, u8))> {
    Some(match key {
        "prepare" => ("준비", (255, 200, 80)),
        "request" => ("점등 요청", (120, 200, 255)),
        "confirmed" => ("점등 확인", (100, 240, 150)),
        "ended" => ("종료", (190, 190, 190)),
        _ => return None,
    })
}

/// A validated record: `(state, origin, direction, reason, lamp)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Indication {
    state: String,
    origin: String,
    direction: String,
    reason: String,
    lamp: String,
}

/// `indicator_display.clean`.
pub fn clean(value: &Value, now: f64) -> Option<Indication> {
    let stamp = value.get("stamp")?.as_f64()?;
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    let record = Indication {
        state: text("state")?,
        origin: text("origin")?,
        direction: text("direction")?,
        reason: text("reason")?,
        lamp: text("lamp")?,
    };
    (stamp.is_finite()
        && (0. ..MAX_AGE).contains(&(now - stamp))
        && value.get("version").and_then(Value::as_i64) == Some(1)
        && state_label(&record.state).is_some()
        && origin(&record.origin).is_some()
        && matches!(record.direction.as_str(), "left" | "right")
        && reason(&record.reason).is_some()
        && matches!(
            record.lamp.as_str(),
            "left" | "right" | "both" | "off" | "unknown"
        ))
    .then_some(record)
}

/// `indicator_display.lines`.
pub fn lines(value: &Indication) -> (String, String) {
    let direction = if value.direction == "left" {
        "좌"
    } else {
        "우"
    };
    let lamp = match value.lamp.as_str() {
        "left" => "좌 ON",
        "right" => "우 ON",
        "both" => "양쪽 ON",
        "off" => "OFF",
        _ => "미확인",
    };
    (
        format!(
            "{direction} · {} · {}",
            origin(&value.origin).unwrap_or_default(),
            state_label(&value.state).map_or("", |s| s.0)
        ),
        format!(
            "{} / 차량 램프 {lamp}",
            reason(&value.reason).unwrap_or_default()
        ),
    )
}

fn read_bounded(path: &Path, limit: u64) -> Option<Value> {
    let mut raw = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(limit + 1)
        .read_to_end(&mut raw)
        .ok()?;
    if raw.len() as u64 > limit {
        return None;
    }
    serde_json::from_slice(&raw).ok()
}

#[derive(Default)]
struct Reader {
    next_read: f64,
    value: Option<Value>,
}

thread_local! {
    static READER: RefCell<Reader> = RefCell::new(Reader::default());
    static BADGE: RefCell<(f64, Option<&'static str>)> = const { RefCell::new((0., None)) };
}

/// The current indication, re-read at most every 0.2 s.
pub fn current(now: f64) -> Option<Indication> {
    READER.with(|reader| {
        let mut reader = reader.borrow_mut();
        if now >= reader.next_read {
            reader.next_read = now + 0.2;
            reader.value = read_bounded(Path::new(PATH), MAX_BYTES);
        }
        reader.value.as_ref().and_then(|v| clean(v, now))
    })
}

/// Bounded two-line badge, the same on C3, C4 and the narrow USB panel.
pub fn draw(
    draw: &mut dyn Draw,
    now: f64,
    x: f64,
    y: f64,
    width: f64,
    size: f64,
) -> Result<(), Error> {
    let Some(value) = current(now) else {
        return Ok(());
    };
    let (r, g, b) = state_label(&value.state).map_or((255, 255, 255), |s| s.1);
    draw.rounded(
        Rect {
            x: float(x),
            y: float(y),
            width: float(width),
            height: float(size * 2.7),
        },
        0.,
        color(0, 0, 0, 180),
    )?;
    let (first, second) = lines(&value);
    for (i, text) in [first, second].iter().enumerate() {
        let measured = f64::from(measure(draw, Font::Display, text, size, 0.).x);
        let fitted = size.min(size * (width - 16.) / measured.max(1.));
        draw.text(TextDraw {
            font: Font::Display,
            text,
            position: Point {
                x: float(x + 8.),
                y: float(y + 4. + i as f64 * size * 1.25),
            },
            size: float(fitted),
            spacing: 0.,
            color: if i == 0 {
                color(r, g, b, 255)
            } else {
                color(255, 255, 255, 255)
            },
        })?;
    }
    Ok(())
}

const B1_ENROLLED: &str = "/data/carrot/b1-monitor.json";
const B1_STATUS: &str = "/dev/shm/carrot-b1-link.json";

fn b1_label(state: &str) -> Option<(&'static str, (u8, u8, u8))> {
    Some(match state {
        "updating" => ("S3 · 업데이트", (96, 218, 243)),
        "offline" => ("S3 · 연결 대기", (170, 178, 188)),
        "auth" => ("S3 · 인증 중", (255, 197, 89)),
        "missing" => ("S3 · CAN 대기", (202, 147, 255)),
        "ready" | "waiting" => ("S3 · CAN 연결", (96, 232, 162)),
        "sending" => ("S3 · 송신", (96, 218, 243)),
        "driver_override" => ("S3 · 레버 우선", (255, 197, 89)),
        "error" => ("S3 · CAN 오류", (252, 128, 110)),
        _ => return None,
    })
}

const B1_STATES: [&str; 9] = [
    "updating",
    "offline",
    "auth",
    "missing",
    "ready",
    "waiting",
    "sending",
    "driver_override",
    "error",
];

/// `b1_badge.status`: `None` without an enrolled board.
fn b1_status(now: f64) -> Option<&'static str> {
    if !Path::new(B1_ENROLLED).is_file() {
        return None;
    }
    let record = read_bounded(Path::new(B1_STATUS), 256);
    let state = record.as_ref().and_then(|r| {
        let time = r.get("time")?.as_f64()?;
        let state = r.get("state")?.as_str()?;
        if !(0. ..2.).contains(&(now - time)) {
            return None;
        }
        B1_STATES.iter().copied().find(|s| *s == state)
    });
    Some(state.unwrap_or("offline"))
}

/// Persistent B1 link indicator on the home and driving screens.
pub fn draw_b1_badge(
    draw: &mut dyn Draw,
    now: f64,
    rect: Rect,
    top: Option<f64>,
    right_margin: Option<f64>,
) -> Result<(), Error> {
    let state = BADGE.with(|badge| {
        let mut badge = badge.borrow_mut();
        if now >= badge.0 {
            badge.1 = b1_status(now);
            badge.0 = now + 0.35;
        }
        badge.1
    });
    let Some((label, (r, g, b))) = state.and_then(b1_label) else {
        return Ok(());
    };
    let (rx, ry, rw) = (f64::from(rect.x), f64::from(rect.y), f64::from(rect.width));
    let size = if rw >= 1200. { 24. } else { 16. };
    let width = size * 8.;
    let mut x = rx + rw * 0.72 - width / 2.;
    let mut y = ry + size * 0.55;
    if let Some(top) = top {
        y = ry + top;
    }
    if let Some(margin) = right_margin {
        x = rx + rw - margin - width;
    }
    draw.rounded(
        Rect {
            x: float(x),
            y: float(y),
            width: float(width),
            height: float(size * 1.55),
        },
        0.7,
        color(10, 18, 25, 220),
    )?;
    draw.circle(
        Point {
            x: float(x + size * 0.62),
            y: float(y + size * 0.77),
        },
        float(size * 0.19),
        color(r, g, b, 255),
    )?;
    draw.text(TextDraw {
        font: Font::Display,
        text: label,
        position: Point {
            x: float(x + size * 1.05),
            y: float(y + size * 0.24),
        },
        size: float(size),
        spacing: 0.,
        color: color(255, 255, 255, 255),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clean_validates_and_lines_describe() {
        let record = json!({"version": 1, "stamp": 10.0, "state": "confirmed", "origin": "turn",
            "direction": "left", "reason": "turning", "lamp": "left"});
        let value = clean(&record, 10.1).expect("valid");
        assert_eq!(
            lines(&value),
            (
                "좌 · TBT 턴 · 점등 확인".into(),
                "턴 진행 중 / 차량 램프 좌 ON".into()
            )
        );
        assert!(clean(&record, 10.7).is_none(), "stale");
        let mut bad = record;
        bad["reason"] = json!("idle");
        assert!(clean(&bad, 10.1).is_none());
    }
}
