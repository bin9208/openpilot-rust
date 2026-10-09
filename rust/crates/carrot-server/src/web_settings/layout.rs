use super::{coercion::lowered, Catalog};
use crate::{Error, Value};

impl Catalog {
    pub fn normalize_layout(&self, areas: [&Value; 2], source: &str) -> Result<[String; 2], Error> {
        let mut selected = Vec::with_capacity(2);
        for (value, slot, fallback) in [
            (areas[0], "primary", "vision"),
            (areas[1], "secondary", "navigation"),
        ] {
            let requested = lowered(value, false)?;
            let supported = |content: &&super::catalog::Content| {
                content.slots.iter().any(|item| item == slot)
                    && content.sources.iter().any(|item| item == source)
                    && !(content.singleton && selected.contains(&content.id))
            };
            let chosen = self
                .contents
                .iter()
                .filter(supported)
                .find(|content| requested.text_eq(&content.id))
                .or_else(|| {
                    self.contents
                        .iter()
                        .filter(supported)
                        .find(|content| content.id == fallback)
                })
                .or_else(|| self.contents.iter().find(supported));
            let Some(chosen) = chosen else {
                return Err(Error::Source(format!(
                    "NO_SUPPORTED_CONTENT: slot={slot} source={source}"
                )));
            };
            selected.push(chosen.id.clone());
        }
        Ok([selected.remove(0), selected.remove(0)])
    }
}
