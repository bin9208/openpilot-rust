use super::{Error, Result};
use openpilot_sdl2::gui::{Context, Font};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
const MONO: &[&str] = &[
    "monospace",
    "misc-fixed",
    "courier",
    "couriernew",
    "console",
    "fixed",
    "mono",
    "freemono",
    "bitstreamverasansmono",
    "verasansmono",
    "monotype",
    "lucidaconsole",
    "consolas",
    "dejavusansmono",
    "liberationmono",
];
const SANS: &[&str] = &[
    "sans",
    "arial",
    "helvetica",
    "swiss",
    "freesans",
    "bitstreamverasans",
    "verasans",
    "verdana",
    "tahoma",
    "calibri",
    "gillsans",
    "segoeui",
    "trebuchetms",
    "ubuntu",
    "dejavusans",
    "liberationsans",
];
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FontConfig {
    pub mono: Option<PathBuf>,
    pub title: Option<PathBuf>,
    pub title_fake_bold: bool,
    pub fallback: PathBuf,
}
impl FontConfig {
    #[must_use]
    pub fn installed(bin: &Path) -> Self {
        Self {
            mono: None,
            title: None,
            title_fake_bold: true,
            fallback: bin.join("cluster-replay/freesansbold.ttf"),
        }
    }
}
pub struct Fonts {
    pub normal: Font,
    pub small: Font,
    pub title: Font,
}
impl Fonts {
    /// # Errors
    /// Returns a missing font asset or external TTF error.
    pub fn load(context: &Context, config: &FontConfig) -> Result<Self> {
        let catalog = system_catalog();
        let mono = choose(&catalog, MONO, "consolas", false);
        let title = choose(&catalog, SANS, "segoeui", true);
        let mono_path = config
            .mono
            .as_deref()
            .or(mono.as_ref().map(|p| p.0.as_path()))
            .unwrap_or(&config.fallback);
        let title_path = config
            .title
            .as_deref()
            .or(title.as_ref().map(|p| p.0.as_path()))
            .unwrap_or(&config.fallback);
        let fallback_mono = config.mono.is_none() && mono.is_none();
        let fallback_title = config.title.is_none() && title.is_none();
        let bold = if config.title.is_some() {
            config.title_fake_bold
        } else {
            title.as_ref().is_none_or(|p| p.1)
        };
        if !mono_path.is_file() || !title_path.is_file() {
            return Err(Error::Contract("replay font asset missing"));
        }
        Ok(Self {
            normal: context.font(mono_path, if fallback_mono { 11 } else { 16 }, false)?,
            small: context.font(mono_path, if fallback_mono { 9 } else { 14 }, false)?,
            title: context.font(title_path, if fallback_title { 13 } else { 19 }, bold)?,
        })
    }
}
type Catalog = BTreeMap<String, Vec<((bool, bool), PathBuf)>>;
fn system_catalog() -> Catalog {
    let mut result = Catalog::new();
    let output = match super::font_inventory::collect() {
        Ok(output) => output,
        Err(error) => {
            eprintln!("replay font inventory: {error}");
            return result;
        }
    };
    let ascii: String = output
        .into_iter()
        .filter(u8::is_ascii)
        .map(char::from)
        .collect();
    for line in ascii.lines() {
        let mut parts = line.splitn(3, ':');
        let Some(path) = parts.next() else {
            continue;
        };
        let Some(family) = parts.next() else {
            continue;
        };
        let Some(style) = parts.next() else {
            continue;
        };
        let path = PathBuf::from(path);
        if !path.extension().is_some_and(|ext| {
            ext.eq_ignore_ascii_case("ttf")
                || ext.eq_ignore_ascii_case("ttc")
                || ext.eq_ignore_ascii_case("otf")
        }) {
            continue;
        }
        let name = family
            .split(',')
            .next()
            .unwrap_or("")
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .flat_map(char::to_lowercase)
            .collect::<String>();
        let key = (
            style.contains("Bold"),
            style.contains("Italic") || style.contains("Oblique"),
        );
        let styles = result.entry(name).or_default();
        if let Some(entry) = styles.iter_mut().find(|p| p.0 == key) {
            entry.1 = path;
        } else {
            styles.push((key, path));
        }
    }
    result
}
fn choose(catalog: &Catalog, aliases: &[&str], name: &str, bold: bool) -> Option<(PathBuf, bool)> {
    let styles = catalog
        .get(name)
        .or_else(|| aliases.iter().find_map(|n| catalog.get(*n)))?;
    let selected = styles
        .iter()
        .find(|p| p.0 == (bold, false))
        .or_else(|| styles.iter().find(|p| p.0 == (false, false)))
        .or_else(|| styles.first())?;
    let plain = styles.iter().find(|p| p.0 == (false, false));
    let requested = styles.iter().find(|p| p.0 == (bold, false));
    let got_bold = if requested.is_some() {
        plain.is_none_or(|p| p.1 != selected.1)
    } else if plain.is_some() {
        false
    } else {
        selected.0 .0
    };
    Some((selected.1.clone(), bold && !got_bold))
}
