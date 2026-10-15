//! Final source recovery layout; all geometry is rounded at the raylib ABI.
use crate::{text::whitespace, Error};
pub trait Measure {
    fn width(&self, text: &str, size: f64) -> Result<f64, Error>;
}
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Clear([u8; 4]),
    Text {
        text: String,
        position: [f32; 2],
        size: f32,
        color: [u8; 4],
    },
    Rectangle {
        rect: [f32; 4],
        roundness: f32,
        segments: i32,
        color: [u8; 4],
    },
}
pub const LABELS: [(&str, &str, &str); 7] = [
    (
        "idle",
        "시작 실패 · 업데이트 대기",
        "Startup failed · waiting for update",
    ),
    (
        "waiting",
        "새 수정본을 기다리고 있습니다",
        "Waiting for a new fix",
    ),
    (
        "updating",
        "업데이트 중 · 전원을 유지하세요",
        "Updating · keep power connected",
    ),
    (
        "failed",
        "복구 작업 실패 · 다시 시도하세요",
        "Recovery action failed · tap to retry",
    ),
    (
        "rebooting",
        "업데이트 완료 · 재부팅 중",
        "Update complete · rebooting",
    ),
    (
        "cleaning",
        "빌드 정리 중 · 전원을 유지하세요",
        "Cleaning build · keep power connected",
    ),
    (
        "rebuild_rebooting",
        "재부팅 후 다시 빌드합니다",
        "Rebooting to rebuild",
    ),
];
pub const AUTO_KO: &str = "30초마다 수정본 확인 · 리빌드는 재부팅 후 수 분 소요";
pub const BUTTON_KO: &str = "Git pull 후 재부팅";
pub const REBUILD_KO: &str = "리빌드 · 재부팅";
pub const ERROR_KO: &str = "시작 오류 · 누르면 다음 내용";
pub fn labels(native_repair: bool) -> [(&'static str, &'static str, &'static str); 7] {
    let mut labels = LABELS;
    if native_repair {
        labels[5] = (
            "cleaning",
            "복구 중 · 전원을 유지하세요",
            "Repairing · keep power connected",
        );
        labels[6] = (
            "rebuild_rebooting",
            "복구 완료 · 재부팅 중",
            "Repair complete · rebooting",
        );
    }
    labels
}
pub fn automatic_label(native_repair: bool) -> &'static str {
    if native_repair {
        "30초마다 수정본 확인 · 복구 중 전원을 유지하세요"
    } else {
        AUTO_KO
    }
}
pub fn repair_label(native_repair: bool) -> &'static str {
    if native_repair {
        "복구 · 재부팅"
    } else {
        REBUILD_KO
    }
}
fn fit(text: &str, size: f64, width: f64, measure: &impl Measure) -> Result<String, Error> {
    let mut text = text
        .split(whitespace)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if measure.width(&text, size)? <= width {
        return Ok(text);
    }
    while !text.is_empty() && measure.width(&format!("{text}..."), size)? > width {
        text.pop();
    }
    Ok(format!("{text}..."))
}
pub fn pages(text: &str, scale: f64, measure: &impl Measure) -> Result<Vec<Vec<String>>, Error> {
    let mut lines = Vec::new();
    for line in text.split([
        '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
        '\u{2029}',
    ]) {
        let mut expanded = String::new();
        let mut column = 0;
        for c in line.chars() {
            if c == '\t' {
                let n = 2 - column % 2;
                expanded.extend(std::iter::repeat_n(' ', n));
                column += n;
            } else {
                expanded.push(c);
                column += 1;
            }
        }
        let mut chars: Vec<_> = expanded.chars().collect();
        while !chars.is_empty() {
            let (mut low, mut high) = (1, chars.len());
            while low < high {
                let mid = (low + high).div_ceil(2);
                let text = chars[..mid].iter().collect::<String>();
                if measure.width(&text, 12.0 * scale)? <= 504.0 * scale {
                    low = mid;
                } else {
                    high = mid - 1;
                }
            }
            lines.push(chars.drain(..low).collect::<String>());
        }
    }
    Ok(if lines.is_empty() {
        vec![vec!["No error output was captured.".into()]]
    } else {
        lines.chunks(4).map(<[String]>::to_vec).collect()
    })
}
pub struct Screen<'a> {
    pub native_repair: bool,
    pub dimensions: (i32, i32),
    pub status: &'a str,
    pub detail: &'a str,
    pub reason: &'a str,
    pub address: &'a str,
    pub pages: &'a [Vec<String>],
    pub page: usize,
}
pub struct Frame {
    pub commands: Vec<Paint>,
    pub areas: [[f32; 4]; 3],
}
pub fn frame(screen: Screen<'_>, measure: &impl Measure) -> Result<Frame, Error> {
    let (w, h) = (
        f64::from(screen.dimensions.0),
        f64::from(screen.dimensions.1),
    );
    let scale = (w / 536.0).min(h / 240.0);
    let x = (w - 536.0 * scale) / 2.0;
    let y = (h - 240.0 * scale) / 2.0;
    let labels = labels(screen.native_repair);
    let (_, ko, en) = labels
        .iter()
        .find(|(state, _, _)| *state == screen.status)
        .unwrap_or(&labels[3]);
    let mut commands = vec![Paint::Clear([17, 22, 29, 255])];
    let mut line = |text: &str, top: f64, size: f64, color: [u8; 4]| -> Result<(), Error> {
        commands.push(Paint::Text {
            text: fit(text, size * scale, 504.0 * scale, measure)?,
            position: [(x + 16.0 * scale) as f32, (y + top * scale) as f32],
            size: (size * scale) as f32,
            color,
        });
        Ok(())
    };
    line(ko, 7.0, 21.0, [255; 4])?;
    line(en, 31.0, 12.0, [200, 200, 200, 255])?;
    line(
        &format!("{} · {}", screen.address, screen.reason),
        47.0,
        12.0,
        [120, 195, 255, 255],
    )?;
    line(
        &format!("{ERROR_KO} ({}/{})", screen.page + 1, screen.pages.len()),
        65.0,
        12.0,
        [255, 190, 120, 255],
    )?;
    for (i, text) in screen.pages[screen.page].iter().enumerate() {
        line(text, 81.0 + i as f64 * 15.0, 12.0, [255; 4])?;
    }
    line(
        if screen.native_repair && screen.status == "rebuild_rebooting" {
            "Repair ready. Rebooting."
        } else if screen.detail.is_empty() {
            en
        } else {
            screen.detail
        },
        145.0,
        11.0,
        [200, 200, 200, 255],
    )?;
    line(
        automatic_label(screen.native_repair),
        160.0,
        11.0,
        [200, 200, 200, 255],
    )?;
    line(
        "Auto-check every 30s · reboot only after a new update",
        173.0,
        10.0,
        [200, 200, 200, 255],
    )?;
    let busy = matches!(
        screen.status,
        "updating" | "cleaning" | "rebooting" | "rebuild_rebooting"
    );
    let mut areas = [[0.0; 4]; 3];
    for (i, (left, ko, en)) in [
        (
            16.0,
            repair_label(screen.native_repair),
            if screen.native_repair {
                "Repair & reboot"
            } else {
                "Clean build & reboot"
            },
        ),
        (274.0, BUTTON_KO, "Git pull & reboot"),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = [
            (x + left * scale) as f32,
            (y + 188.0 * scale) as f32,
            (246.0 * scale) as f32,
            (44.0 * scale) as f32,
        ];
        areas[i] = rect;
        commands.push(Paint::Rectangle {
            rect,
            roundness: 0.15,
            segments: 8,
            color: if busy {
                [80, 80, 80, 255]
            } else {
                [33, 104, 200, 255]
            },
        });
        for (text, top, size) in [(ko, 192.0, 18.0), (en, 215.0, 11.0)] {
            let size = size * scale;
            let tw = measure.width(text, size)?;
            commands.push(Paint::Text {
                text: text.into(),
                position: [
                    (f64::from(rect[0]) + (f64::from(rect[2]) - tw) / 2.0) as f32,
                    (y + top * scale) as f32,
                ],
                size: size as f32,
                color: [255; 4],
            });
        }
    }
    areas[2] = [
        (x + 16.0 * scale) as f32,
        (y + 63.0 * scale) as f32,
        (504.0 * scale) as f32,
        (79.0 * scale) as f32,
    ];
    Ok(Frame { commands, areas })
}
#[cfg(test)]
mod tests;
