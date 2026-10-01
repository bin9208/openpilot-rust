//! Source layouts from system/ui/widgets/keyboard.py.
use crate::{geometry::Rect, text_layout::float};
use num_traits::ToPrimitive;
pub const BACKSPACE: &str = "<-";
pub const ENTER: &str = "->";
pub const SHIFT_OFF: &str = "SHIFT_OFF";
pub const SHIFT_ON: &str = "SHIFT_ON";
pub const CAPS: &str = "CAPS";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Lowercase,
    Uppercase,
    Numbers,
    Specials,
}
impl Layout {
    pub fn rows(self) -> [&'static [&'static str]; 4] {
        match self {
            Self::Lowercase => [
                &["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"],
                &["a", "s", "d", "f", "g", "h", "j", "k", "l"],
                &[SHIFT_OFF, "z", "x", "c", "v", "b", "n", "m", BACKSPACE],
                &["123", "/", "-", " ", ".", ENTER],
            ],
            Self::Uppercase => [
                &["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P"],
                &["A", "S", "D", "F", "G", "H", "J", "K", "L"],
                &[SHIFT_ON, "Z", "X", "C", "V", "B", "N", "M", BACKSPACE],
                &["123", "/", "-", " ", ".", ENTER],
            ],
            Self::Numbers => [
                &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
                &["-", "/", ":", ";", "(", ")", "$", "&", "@", "\""],
                &["#+=", "_", ",", "?", "!", "`", BACKSPACE],
                &["ABC", " ", ".", ENTER],
            ],
            Self::Specials => [
                &["[", "]", "{", "}", "#", "%", "^", "*", "+", "="],
                &["_", "\\", "|", "~", "<", ">", "€", "£", "¥", "•"],
                &["123", "-", ",", "?", "!", "'", BACKSPACE],
                &["ABC", " ", ".", ENTER],
            ],
        }
    }
    pub fn rectangles(self, rect: Rect) -> Vec<(&'static str, Rect)> {
        let rows = self.rows();
        let count = rows[2].len().to_f64().unwrap_or(1.0);
        let max_width = (f64::from(rect.width) - (count - 1.0) * 15.0) / count;
        let height = (f64::from(rect.height) - 345.0) / 4.0;
        let mut result = Vec::new();
        for (row, keys) in rows.iter().enumerate() {
            let count = keys.len().to_f64().unwrap_or(1.0);
            let width = ((f64::from(rect.width)
                - if row == 1 { 180.0 } else { 0.0 }
                - 15.0 * (count - 1.0))
                / count)
                .min(max_width);
            let mut x = f64::from(rect.x) + if row == 1 { 90.0 } else { 0.0 };
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    x += 15.0;
                }
                let width = match *key {
                    " " => width * 3.0 + 30.0,
                    ENTER => width * 2.0 + 15.0,
                    _ => width,
                };
                result.push((
                    *key,
                    Rect {
                        x: float(x),
                        y: float(
                            f64::from(rect.y)
                                + 300.0
                                + row.to_f64().unwrap_or(0.0) * (height + 15.0),
                        ),
                        width: float(width),
                        height: float(height),
                    },
                ));
                x += width;
            }
        }
        result
    }
}
