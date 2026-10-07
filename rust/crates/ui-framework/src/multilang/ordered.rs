use serde::de::{MapAccess, Visitor};
use std::fmt;
pub(super) struct Languages(pub Vec<(String, String)>);
impl<'de> serde::Deserialize<'de> for Languages {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct LanguageVisitor;
        impl<'de> Visitor<'de> for LanguageVisitor {
            type Value = Languages;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("language-name to code object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut values: Vec<(String, String)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    if let Some((_, old)) = values.iter_mut().find(|(name, _)| *name == key) {
                        *old = value;
                    } else {
                        values.push((key, value));
                    }
                }
                Ok(Languages(values))
            }
        }
        deserializer.deserialize_map(LanguageVisitor)
    }
}
