use crate::{
    draw::{Draw, WHITE},
    geometry::Point,
    text::Font,
    text_layout::{self as text, float},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use regex::Regex;
use std::{collections::HashMap, sync::OnceLock};
mod style;
pub use style::{Element, ElementType, Style};
pub struct HtmlRenderer {
    pub state: WidgetState,
    pub elements: Vec<Element>,
    pub styles: HashMap<ElementType, Style>,
    pub color: u32,
    pub center: bool,
    indent: i32,
    cached: Option<(f64, f64)>,
}
impl HtmlRenderer {
    pub fn new(value: &str, base_size: f64) -> Result<Self, Error> {
        let mut styles = HashMap::new();
        for (kind, multiplier, top, bottom) in [
            (ElementType::H1, 2.0, 20.0, 16.0),
            (ElementType::H2, 1.5, 24.0, 12.0),
            (ElementType::H3, 1.17, 20.0, 10.0),
            (ElementType::H4, 1.0, 16.0, 8.0),
            (ElementType::H5, 0.83, 12.0, 6.0),
            (ElementType::H6, 0.67, 10.0, 4.0),
        ] {
            styles.insert(
                kind,
                Style {
                    size: (base_size.trunc() * multiplier).round_ties_even(),
                    font: Font::Bold,
                    top,
                    bottom,
                },
            );
        }
        for (kind, font, top, bottom) in [
            (ElementType::P, Font::Normal, 8.0, 12.0),
            (ElementType::B, Font::Bold, 8.0, 12.0),
            (ElementType::Li, Font::Normal, 6.0, 6.0),
        ] {
            styles.insert(
                kind,
                Style {
                    size: base_size.trunc(),
                    font,
                    top,
                    bottom,
                },
            );
        }
        styles.insert(
            ElementType::Br,
            Style {
                size: 0.0,
                font: Font::Normal,
                top: 0.0,
                bottom: 12.0,
            },
        );
        let mut result = Self {
            state: WidgetState::default(),
            elements: Vec::new(),
            styles,
            color: WHITE,
            center: false,
            indent: 0,
            cached: None,
        };
        result.parse(value)?;
        Ok(result)
    }
    fn add(&mut self, kind: ElementType, content: String) -> Result<(), Error> {
        let style = self
            .styles
            .get(&kind)
            .ok_or(Error::Contract("HTML element has no source style"))?;
        self.elements.push(Element {
            kind,
            content,
            size: style.size,
            font: style.font,
            top: style.top,
            bottom: style.bottom,
            line_height: 0.9,
            indent: self.indent,
        });
        Ok(())
    }
    fn close(
        &mut self,
        tag: &mut Option<ElementType>,
        words: &mut Vec<String>,
    ) -> Result<(), Error> {
        let kind = *tag.get_or_insert(ElementType::P);
        let mut value = words
            .join(" ")
            .trim_matches(crate::text::whitespace)
            .to_owned();
        words.clear();
        if !value.is_empty() {
            if kind == ElementType::Li {
                value = format!("• {value}");
            }
            self.add(kind, value)?;
        }
        Ok(())
    }
    pub fn parse(&mut self, value: &str) -> Result<(), Error> {
        static PATTERNS: OnceLock<Result<[Regex; 4], regex::Error>> = OnceLock::new();
        let patterns = PATTERNS
            .get_or_init(|| {
                Ok([
                    Regex::new("(?s)<!--.*?-->")?,
                    Regex::new("<!DOCTYPE[^>]*>")?,
                    Regex::new("</?(?:html|head|body)[^>]*>")?,
                    Regex::new(r"</[^>]+>|<[^>]+>|[^<\s\x1c-\x1f]+")?,
                ])
            })
            .as_ref()
            .map_err(|_| Error::Contract("invalid static HTML token patterns"))?;
        self.elements.clear();
        self.cached = None;
        let value = patterns[0].replace_all(value, "");
        let value = patterns[1].replace_all(&value, "");
        let value = patterns[2].replace_all(&value, "");
        let mut tag = None;
        let mut words = Vec::new();
        for token in patterns[3].find_iter(&value).map(|found| found.as_str()) {
            let (name, end) = if let Some(name) = token
                .strip_prefix("</")
                .and_then(|value| value.strip_suffix('>'))
            {
                (Some(name), true)
            } else {
                (
                    token
                        .strip_prefix('<')
                        .and_then(|value| value.strip_suffix('>')),
                    false,
                )
            };
            if let Some(kind) = name.and_then(ElementType::parse) {
                if kind == ElementType::Br {
                    self.close(&mut tag, &mut words)?;
                    self.add(ElementType::Br, String::new())?;
                } else {
                    self.close(&mut tag, &mut words)?;
                    tag = if end { None } else { Some(kind) };
                }
                if kind == ElementType::Ul {
                    self.indent = if end {
                        (self.indent - 1).max(0)
                    } else {
                        self.indent
                            .checked_add(1)
                            .ok_or(Error::Contract("HTML indent overflow"))?
                    };
                }
            } else {
                words.push(token.to_owned());
            }
        }
        if !words.is_empty() {
            self.close(&mut tag, &mut words)?;
        }
        Ok(())
    }
    pub fn total_height(&mut self, draw: &dyn Draw, width: f64) -> f64 {
        if let Some((cached, height)) = self.cached {
            if cached == width {
                return height;
            }
        }
        let mut height = 0.0;
        for element in &self.elements {
            if element.kind == ElementType::Br {
                height += element.bottom;
                continue;
            }
            height += element.top;
            if !element.content.is_empty() {
                for _ in text::wrap(
                    draw,
                    element.font,
                    &element.content,
                    element.size,
                    0.0,
                    (width - 40.0).trunc(),
                ) {
                    height += element.size * draw.font_scale() * element.line_height;
                }
            }
            height += element.bottom;
        }
        self.cached = Some((width, height));
        height
    }
}

mod render;
