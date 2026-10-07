mod ordered;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error("invalid quoted PO string: {0}")]
    Quoted(String),
    #[error("invalid PO plural index")]
    Index,
}
#[derive(Default, Debug, serde::Serialize)]
pub struct Catalog {
    pub translations: HashMap<String, String>,
    pub plurals: HashMap<String, Vec<String>>,
}
#[derive(Default)]
struct Entry {
    id: String,
    plural: String,
    translation: String,
    forms: BTreeMap<usize, String>,
}
impl Entry {
    fn finish(&mut self, catalog: &mut Catalog) -> Result<(), Error> {
        if !self.id.is_empty() {
            if !self.plural.is_empty() {
                let last = self.forms.keys().last().copied().unwrap_or(0);
                let mut forms = Vec::new();
                forms
                    .try_reserve(last.checked_add(1).ok_or(Error::Index)?)
                    .map_err(|_| Error::Index)?;
                for index in 0..=last {
                    forms.push(self.forms.remove(&index).unwrap_or_default());
                }
                catalog.plurals.insert(std::mem::take(&mut self.id), forms);
            } else {
                catalog.translations.insert(
                    std::mem::take(&mut self.id),
                    std::mem::take(&mut self.translation),
                );
            }
        }
        *self = Self::default();
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Field {
    Id,
    Plural,
    Translation,
    Form(usize),
}
pub fn parse_quoted(value: &str) -> Result<String, Error> {
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| Error::Quoted(value.to_owned()))?;
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    '"' => result.push('"'),
                    '\\' => result.push('\\'),
                    _ => {
                        result.push('\\');
                        result.push(next);
                    }
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }
    Ok(result)
}
pub fn parse(value: &str) -> Result<Catalog, Error> {
    let mut catalog = Catalog::default();
    let mut entry = Entry::default();
    let mut field = None;
    for raw in value.lines() {
        let line = raw.trim();
        if line.is_empty() {
            entry.finish(&mut catalog)?;
            field = None;
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("msgid_plural ") {
            entry.plural = parse_quoted(value)?;
            field = Some(Field::Plural);
            continue;
        }
        if let Some(value) = line.strip_prefix("msgid ") {
            entry.id = parse_quoted(value)?;
            field = Some(Field::Id);
            continue;
        }
        if let Some(value) = line.strip_prefix("msgstr[") {
            if let Some((index, value)) = value.split_once(']') {
                if !index.is_empty()
                    && index.bytes().all(|c| c.is_ascii_digit())
                    && value.starts_with(char::is_whitespace)
                {
                    let index = index.parse().map_err(|_| Error::Index)?;
                    entry.forms.insert(index, parse_quoted(value)?);
                    field = Some(Field::Form(index));
                    continue;
                }
            }
        }
        if let Some(value) = line.strip_prefix("msgstr ") {
            entry.translation = parse_quoted(value)?;
            field = Some(Field::Translation);
            continue;
        }
        if line.starts_with('"') {
            let value = parse_quoted(line)?;
            match field {
                Some(Field::Id) => entry.id.push_str(&value),
                Some(Field::Plural) => entry.plural.push_str(&value),
                Some(Field::Translation) => entry.translation.push_str(&value),
                Some(Field::Form(index)) => entry.forms.entry(index).or_default().push_str(&value),
                None => {}
            }
        }
    }
    entry.finish(&mut catalog)?;
    Ok(catalog)
}
pub fn plural_index(language: &str, n: i64) -> usize {
    match language {
        "en" | "de" | "es" | "tr" => usize::from(n != 1),
        "fr" | "pt-BR" => usize::from(n > 1),
        "uk" => {
            if n.rem_euclid(10) == 1 && n.rem_euclid(100) != 11 {
                0
            } else if (2..=4).contains(&n.rem_euclid(10)) && !(12..=14).contains(&n.rem_euclid(100))
            {
                1
            } else {
                2
            }
        }
        _ => 0,
    }
}
pub struct Multilang {
    root: PathBuf,
    pub languages: HashMap<String, String>,
    pub language_order: Vec<String>,
    pub codes: HashMap<String, String>,
    language: String,
    selector_language: String,
    catalog: Catalog,
}
impl Multilang {
    pub fn new(root: &Path, saved: Option<&str>) -> Result<Self, Error> {
        let ordered: ordered::Languages =
            serde_json::from_slice(&std::fs::read(root.join("languages.json"))?)?;
        let language_order = ordered.0.iter().map(|(name, _)| name.clone()).collect();
        let languages: HashMap<String, String> = ordered.0.into_iter().collect();
        let codes: HashMap<_, _> = languages
            .iter()
            .map(|(name, code)| (code.clone(), name.clone()))
            .collect();
        let saved = saved
            .unwrap_or("None")
            .strip_prefix("main_")
            .unwrap_or(saved.unwrap_or("None"));
        let language = if codes.contains_key(saved) {
            saved
        } else {
            "en"
        };
        let mut result = Self {
            root: root.to_owned(),
            languages,
            language_order,
            codes,
            language: language.to_owned(),
            selector_language: "en".to_owned(),
            catalog: Catalog::default(),
        };
        result.setup()?;
        Ok(result)
    }
    pub fn from_params(root: &Path, params: &openpilot_params::Params) -> Result<Self, Error> {
        let value = params.get("LanguageSetting")?;
        let value = value.map(|value| String::from_utf8_lossy(&value).into_owned());
        Self::new(root, value.as_deref())
    }
    pub fn language(&self) -> &str {
        &self.language
    }
    pub fn requires_unifont(&self) -> bool {
        matches!(
            self.language.as_str(),
            "th" | "zh-CHT" | "zh-CHS" | "ko" | "ja"
        )
    }
    pub fn setup(&mut self) -> Result<(), Error> {
        match std::fs::read_to_string(self.root.join(format!("app_{}.po", self.language))) {
            Ok(value) => {
                self.catalog = parse(&value)?;
                self.selector_language = self.language.clone();
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!(
                    "No translation file found for language: {}, using default.",
                    self.language
                );
                self.catalog = Catalog::default();
            }
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }
    pub fn change_language(
        &mut self,
        language: &str,
        params: &openpilot_params::Params,
    ) -> Result<(), Error> {
        params.put("LanguageSetting", language.as_bytes())?;
        self.language = language.to_owned();
        self.setup()
    }
    pub fn tr<'a>(&'a self, text: &'a str) -> &'a str {
        self.catalog
            .translations
            .get(text)
            .filter(|value| !value.is_empty())
            .map_or(text, String::as_str)
    }
    pub fn trn<'a>(&'a self, singular: &'a str, plural: &'a str, n: i64) -> &'a str {
        self.catalog
            .plurals
            .get(singular)
            .and_then(|forms| forms.get(plural_index(&self.selector_language, n)))
            .filter(|form| !form.is_empty())
            .map_or(if n == 1 { singular } else { plural }, String::as_str)
    }
}
