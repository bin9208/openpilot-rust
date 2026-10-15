use super::*;
use serde_json::{json, Value};
struct Font;
impl Measure for Font {
    fn width(&self, text: &str, size: f64) -> Result<f64, crate::Error> {
        Ok(f64::from(
            (text.chars().count() as f64 * f64::from(size as f32) * 0.5) as f32,
        ))
    }
}
fn numbers<const N: usize>(values: [f32; N]) -> Vec<f64> {
    values.into_iter().map(f64::from).collect()
}
#[test]
fn source_recovery_commands_pages_and_hit_areas() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/startup-recovery-layout-final.json"
    ))
    .unwrap();
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let width = row["width"].as_i64().unwrap() as i32;
        let height = row["height"].as_i64().unwrap() as i32;
        let scale = (f64::from(width) / 536.0).min(f64::from(height) / 240.0);
        let pages = pages(row["error"].as_str().unwrap(), scale, &Font).unwrap();
        assert_eq!(json!(pages), row["pages"], "pages {i}");
        let result = frame(
            Screen {
                native_repair: false,
                dimensions: (width, height),
                status: row["status"].as_str().unwrap(),
                detail: row["detail"].as_str().unwrap(),
                reason: row["reason"].as_str().unwrap(),
                address: row["address"].as_str().unwrap(),
                pages: &pages,
                page: row["page"].as_u64().unwrap() as usize,
            },
            &Font,
        )
        .unwrap();
        let commands: Vec<_> = result
            .commands
            .into_iter()
            .map(|paint| match paint {
                Paint::Clear(color) => json!(["clear", color]),
                Paint::Text {
                    text,
                    position,
                    size,
                    color,
                } => json!(["text", text, numbers(position), f64::from(size), 0.0, color]),
                Paint::Rectangle {
                    rect,
                    roundness,
                    segments,
                    color,
                } => json!(["rect", numbers(rect), f64::from(roundness), segments, color]),
            })
            .collect();
        assert_eq!(json!(commands), row["commands"], "commands {i}");
        assert_eq!(json!(result.areas.map(numbers)), row["areas"], "areas {i}");
    }
}
