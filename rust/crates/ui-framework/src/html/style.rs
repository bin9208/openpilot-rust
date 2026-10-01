use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ElementType {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    P,
    B,
    Ul,
    Li,
    Br,
}
impl ElementType {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "h1" => Some(Self::H1),
            "h2" => Some(Self::H2),
            "h3" => Some(Self::H3),
            "h4" => Some(Self::H4),
            "h5" => Some(Self::H5),
            "h6" => Some(Self::H6),
            "p" => Some(Self::P),
            "b" => Some(Self::B),
            "ul" => Some(Self::Ul),
            "li" => Some(Self::Li),
            "br" => Some(Self::Br),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub size: f64,
    pub font: Font,
    pub top: f64,
    pub bottom: f64,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Element {
    pub kind: ElementType,
    pub content: String,
    pub size: f64,
    #[serde(skip)]
    pub font: Font,
    pub top: f64,
    pub bottom: f64,
    pub line_height: f64,
    pub indent: i32,
}
