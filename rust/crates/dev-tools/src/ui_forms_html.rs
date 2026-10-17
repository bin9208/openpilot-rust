//! Original limited HTML token/parser/wrapping policy, independent of candidate.
use crate::{
    seal::Result,
    ui_forms_input::{Canvas, Rect},
};

#[derive(Clone)]
pub struct Element {
    pub kind: String,
    pub content: Vec<u32>,
    pub size: f64,
    pub top: f64,
    pub bottom: f64,
    pub indent: i64,
    pub bold: bool,
}
pub struct Html {
    pub elements: Vec<Element>,
    base: f64,
    indent: i64,
    pub center: bool,
}
fn points(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn begins(text: &[u32], prefix: &str) -> bool {
    text.iter()
        .copied()
        .zip(prefix.chars().map(u32::from))
        .all(|(a, b)| a == b)
        && text.len() >= prefix.chars().count()
}
fn remove_matches(text: &[u32], prefix: &str, suffix: &str) -> Vec<u32> {
    let mut result = Vec::new();
    let mut at = 0;
    while at < text.len() {
        if begins(&text[at..], prefix) {
            if let Some(end) =
                (at + prefix.len()..text.len()).find(|&end| begins(&text[end..], suffix))
            {
                at = end + suffix.len();
                continue;
            }
        }
        result.push(text[at]);
        at += 1;
    }
    result
}
impl Html {
    pub fn new_codepoints(text: &[u32], base: f64, center: bool) -> Result<Self> {
        let mut html = Self {
            elements: vec![],
            base: base.trunc(),
            indent: 0,
            center,
        };
        html.parse_codepoints(text)?;
        Ok(html)
    }
    fn add(&mut self, kind: &str, content: Vec<u32>) -> Result<()> {
        let (size, top, bottom, bold) = match kind {
            "h1" => ((self.base * 2.0).round_ties_even(), 20.0, 16.0, true),
            "h2" => ((self.base * 1.5).round_ties_even(), 24.0, 12.0, true),
            "h3" => ((self.base * 1.17).round_ties_even(), 20.0, 10.0, true),
            "h4" => (self.base, 16.0, 8.0, true),
            "h5" => ((self.base * 0.83).round_ties_even(), 12.0, 6.0, true),
            "h6" => ((self.base * 0.67).round_ties_even(), 10.0, 4.0, true),
            "p" => (self.base, 8.0, 12.0, false),
            "b" => (self.base, 8.0, 12.0, true),
            "li" => (self.base, 6.0, 6.0, false),
            "br" => (0.0, 0.0, 12.0, false),
            _ => return Err("original HTML style missing".into()),
        };
        self.elements.push(Element {
            kind: kind.into(),
            content,
            size,
            top,
            bottom,
            indent: self.indent,
            bold,
        });
        Ok(())
    }
    fn close(&mut self, tag: &mut Option<String>, content: &mut Vec<Vec<u32>>) -> Result<()> {
        let kind = tag.get_or_insert_with(|| "p".into()).clone();
        let mut text = Vec::new();
        for (index, part) in content.iter().enumerate() {
            if index > 0 {
                text.push(32);
            }
            text.extend(part);
        }
        content.clear();
        let start = text
            .iter()
            .position(|&p| !crate::ui_forms_input::python_space(p))
            .unwrap_or(text.len());
        let end = text
            .iter()
            .rposition(|&p| !crate::ui_forms_input::python_space(p))
            .map_or(start, |p| p + 1);
        let mut text = text[start..end].to_vec();
        if !text.is_empty() {
            if kind == "li" {
                let mut prefix = points("• ");
                prefix.append(&mut text);
                text = prefix;
            }
            self.add(&kind, text)?;
        }
        Ok(())
    }
    pub fn parse_codepoints(&mut self, text: &[u32]) -> Result<()> {
        self.elements.clear();
        let text = remove_matches(text, "<!--", "-->");
        let text = remove_matches(&text, "<!DOCTYPE", ">");
        let mut stripped = Vec::new();
        let mut at = 0;
        while at < text.len() {
            if text[at] == 60 {
                let after = at + 1 + usize::from(text.get(at + 1) == Some(&47));
                if ["html", "head", "body"]
                    .iter()
                    .any(|name| begins(&text[after..], name))
                {
                    if let Some(end) = text[after..].iter().position(|&p| p == 62) {
                        at = after + end + 1;
                        continue;
                    }
                }
            }
            stripped.push(text[at]);
            at += 1;
        }
        let mut tag = None;
        let mut content = Vec::new();
        let mut at = 0;
        while at < stripped.len() {
            if crate::ui_forms_input::python_space(stripped[at]) {
                at += 1;
                continue;
            }
            let start = at;
            if stripped[at] == 60 {
                let body = at + 1;
                if let Some(end) = stripped[body..]
                    .iter()
                    .position(|&p| p == 62)
                    .filter(|&v| v > 0)
                {
                    at = body + end + 1;
                } else {
                    at += 1;
                    continue;
                }
            } else {
                while at < stripped.len()
                    && stripped[at] != 60
                    && !crate::ui_forms_input::python_space(stripped[at])
                {
                    at += 1;
                }
            }
            let token = &stripped[start..at];
            let name = crate::ui_forms_input::Input::utf8(token).ok();
            let recognized = name
                .as_deref()
                .and_then(|token| token.strip_prefix('<'))
                .and_then(|token| token.strip_suffix('>'))
                .map(|token| token.strip_prefix('/').unwrap_or(token))
                .filter(|kind| {
                    matches!(
                        *kind,
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "b" | "ul" | "li" | "br"
                    )
                });
            if let Some(kind) = recognized {
                self.close(&mut tag, &mut content)?;
                let ending = token.get(1) == Some(&47);
                if kind == "br" {
                    self.add("br", Vec::new())?;
                } else {
                    tag = if ending { None } else { Some(kind.into()) };
                }
                if kind == "ul" {
                    if ending {
                        self.indent = (self.indent - 1).max(0);
                    } else {
                        self.indent += 1;
                    }
                }
            } else {
                content.push(token.to_vec());
            }
        }
        if !content.is_empty() {
            self.close(&mut tag, &mut content)?;
        }
        Ok(())
    }
    fn font(api: &mut dyn Canvas, bold: bool) -> Result<()> {
        api.call(if bold {
            "font-select 1"
        } else {
            "font-select 0"
        })
    }
    pub fn wrap_codepoints(
        api: &mut dyn Canvas,
        text: &[u32],
        size: f64,
        width: f64,
        scale: f64,
    ) -> Result<Vec<Vec<u32>>> {
        use crate::ui_forms_input::{python_space, text_width_codepoints};
        if text.is_empty() || width <= 0.0 {
            return Ok(Vec::new());
        }
        let mut all = Vec::new();
        for paragraph in text.split(|&point| point == 10) {
            let words = paragraph
                .split(|&point| python_space(point))
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>();
            if words.is_empty() {
                all.push(Vec::new());
                continue;
            }
            let mut current: Vec<u32> = Vec::new();
            for word in words {
                if text_width_codepoints(api, word, size, scale)? > width {
                    if !current.is_empty() {
                        all.push(std::mem::take(&mut current));
                    }
                    let mut word = word;
                    while !word.is_empty() {
                        if text_width_codepoints(api, word, size, scale)? <= width {
                            all.push(word.to_vec());
                            break;
                        }
                        let (mut low, mut high, mut best) = (1, word.len(), 1);
                        while low <= high {
                            let mid = (low + high) / 2;
                            if text_width_codepoints(api, &word[..mid], size, scale)? <= width {
                                best = mid;
                                low = mid + 1;
                            } else {
                                high = mid - 1;
                            }
                        }
                        all.push(word[..best].to_vec());
                        word = &word[best..];
                    }
                    continue;
                }
                let mut candidate = current.clone();
                if !candidate.is_empty() {
                    candidate.push(32);
                }
                candidate.extend(word);
                if text_width_codepoints(api, &candidate, size, scale)? <= width {
                    current = candidate;
                } else {
                    if !current.is_empty() {
                        all.push(std::mem::take(&mut current));
                    }
                    current = word.to_vec();
                }
            }
            if !current.is_empty() {
                all.push(current);
            }
        }
        Ok(all)
    }
    pub fn height(&self, api: &mut dyn Canvas, width: f64, scale: f64) -> Result<f64> {
        let mut height = 0.0;
        for e in &self.elements {
            if e.kind == "br" {
                height += e.bottom;
                continue;
            }
            height += e.top;
            if !e.content.is_empty() {
                Self::font(api, e.bold)?;
                for _ in
                    Self::wrap_codepoints(api, &e.content, e.size, (width - 40.0).trunc(), scale)?
                {
                    height += e.size * scale * 0.9;
                }
            }
            height += e.bottom;
        }
        Ok(height)
    }
    pub fn render(&self, api: &mut dyn Canvas, rect: Rect, scale: f64) -> Result<()> {
        self.render_color(api, rect, scale, 4294967295)
    }
    pub fn render_color(
        &self,
        api: &mut dyn Canvas,
        rect: Rect,
        scale: f64,
        rgba: u32,
    ) -> Result<()> {
        let mut y = rect.y;
        for e in &self.elements {
            if e.kind == "br" {
                y += e.bottom;
                continue;
            }
            y += e.top;
            if y > rect.y + rect.height {
                break;
            }
            if !e.content.is_empty() {
                Self::font(api, e.bold)?;
                for line in Self::wrap_codepoints(
                    api,
                    &e.content,
                    e.size,
                    (rect.width - 40.0).trunc(),
                    scale,
                )? {
                    if y < rect.y - e.size * scale {
                        y += e.size * scale * 0.9;
                        continue;
                    }
                    if y > rect.y + rect.height {
                        break;
                    }
                    let x = if self.center {
                        rect.x
                            + (rect.width
                                - crate::ui_forms_input::text_width_codepoints(
                                    api, &line, e.size, scale,
                                )?)
                                / 2.0
                    } else {
                        rect.x + ((e.indent - 1).max(0) as f64 * 40.0)
                    };
                    api.call(&format!(
                        "text {} {} {y} {} {rgba}",
                        crate::ui_forms_input::hex_codepoints(&line)?,
                        x + 20.0,
                        e.size * scale
                    ))?;
                    y += e.size * scale * 0.9;
                }
            }
            y += e.bottom;
        }
        super::controls::debug_rect(api, rect)
    }
    pub fn snapshot_json(
        &self,
        api: &mut dyn Canvas,
        width: f64,
        scale: f64,
    ) -> Result<openpilot_log_format::JsonValue> {
        use crate::ui_forms_input::{json_codepoints, snapshot_object};
        use openpilot_log_format::JsonValue;
        let height = self.height(api, width.trunc(), scale)?;
        let mut elements = vec![91];
        for (index, e) in self.elements.iter().enumerate() {
            if index > 0 {
                elements.push(44);
            }
            let number = |value: f64| -> Result<JsonValue> {
                Ok(JsonValue::parse(&serde_json::to_string(&value)?)?)
            };
            let value = snapshot_object(&[
                ("kind", JsonValue::text(&e.kind)),
                (
                    "content",
                    JsonValue::codepoints(e.content.clone()).ok_or("HTML invalid codepoint")?,
                ),
                ("size", number(e.size)?),
                ("top", number(e.top)?),
                ("bottom", number(e.bottom)?),
                ("line_height", number(0.9)?),
                ("indent", JsonValue::parse(&e.indent.to_string())?),
            ])?;
            elements.extend(json_codepoints(&value)?);
        }
        elements.push(93);
        snapshot_object(&[
            (
                "height",
                JsonValue::parse(&serde_json::to_string(&height)?)?,
            ),
            ("elements", JsonValue::parse_codepoints(&elements)?),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpilot_log_format::{JsonValue, JsonView};
    #[test]
    fn original_html_tokenizer_operates_on_python_codepoints() {
        let captures = JsonValue::parse(include_str!(
            "../tests/fixtures/ui-forms/html-parser-codepoints-original.json"
        ))
        .unwrap();
        let JsonView::Array(cases) = captures.view() else {
            panic!("original parser fixtures must list")
        };
        for (index, case) in cases.iter().enumerate() {
            let text = case.get("text").unwrap();
            let JsonView::Text(text) = text.view() else {
                panic!("original parser text missing")
            };
            let html = Html::new_codepoints(text, 48.0, false).unwrap();
            let elements = case.get("elements").unwrap();
            let JsonView::Array(elements) = elements.view() else {
                panic!("original elements missing")
            };
            assert_eq!(html.elements.len(), elements.len(), "case {index}");
            for (actual, expected) in html.elements.iter().zip(elements) {
                assert_eq!(
                    actual.kind,
                    expected.get("kind").unwrap().to_utf8().unwrap(),
                    "case {index}"
                );
                let content = expected.get("content").unwrap();
                let JsonView::Text(content) = content.view() else {
                    panic!("content missing")
                };
                assert_eq!(actual.content, content, "case {index}");
                for (name, actual) in [
                    ("size", actual.size),
                    ("top", actual.top),
                    ("bottom", actual.bottom),
                    ("indent", actual.indent as f64),
                ] {
                    assert_eq!(
                        actual,
                        expected
                            .get(name)
                            .unwrap()
                            .to_json()
                            .unwrap()
                            .parse::<f64>()
                            .unwrap(),
                        "case {index} {name}"
                    );
                }
            }
        }
    }
}
