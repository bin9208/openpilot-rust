use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Default,
    Eco,
    Apply,
    Vehicle,
    External,
}
impl ColorMode {
    pub const fn code(self) -> u8 {
        match self {
            Self::Default => 0,
            Self::Eco => 1,
            Self::Apply => 2,
            Self::Vehicle => 3,
            Self::External => 4,
        }
    }
}
fn strip(value: &str) -> &str {
    value.trim_matches(|ch: char| ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch))
}
fn label(source: &str) -> String {
    match source {
        "atc" | "atc2" => "turn".into(),
        "hda" => "cam".into(),
        "hda_section" => "section".into(),
        "hda_bump" => "bump".into(),
        "cam" | "section" | "bump" | "police" | "waze" | "road" | "route" | "school" | "gas"
        | "vturn" | "model" | "turn" => source.into(),
        _ => source.chars().take(8).collect(),
    }
}
pub fn reason(source: &str, provider: &str) -> (String, ColorMode) {
    let source = strip(source).to_lowercase();
    let (mut text, mode) = if source.is_empty() {
        ("apply".into(), ColorMode::Apply)
    } else if matches!(
        source.as_str(),
        "hda" | "hda_section" | "hda_bump" | "school" | "cam:v" | "bump:v" | "school:v"
    ) {
        (label(&source), ColorMode::Vehicle)
    } else if matches!(
        source.as_str(),
        "cam" | "section" | "bump" | "police" | "waze" | "road" | "atc" | "atc2" | "route"
    ) || source.ends_with(":n")
    {
        (
            label(source.strip_suffix(":n").unwrap_or(&source)),
            ColorMode::External,
        )
    } else if let Some(base) = source.strip_suffix(":v") {
        (
            label(base),
            if matches!(base, "cam" | "section" | "bump" | "school") {
                ColorMode::Vehicle
            } else {
                ColorMode::Apply
            },
        )
    } else if let Some(base) = source.strip_suffix(":c") {
        (label(base), ColorMode::Apply)
    } else {
        (label(&source), ColorMode::Apply)
    };
    let prefix = match provider {
        "hda" => Some("HDA"),
        "naver_v1" => Some("N"),
        "tmap_legacy" => Some("T"),
        _ => None,
    };
    if let Some(prefix) = prefix {
        text = format!("{prefix} {text}");
    }
    (text, mode)
}
pub struct Navigation<'a> {
    pub vehicle_available: bool,
    pub external_active: bool,
    pub owner: &'a str,
    pub lifecycle: &'a str,
}
impl Navigation<'_> {
    pub fn status(&self) -> Option<(&'static str, ColorMode)> {
        if !self.lifecycle.is_empty() {
            if self.lifecycle == "guiding" && !self.owner.is_empty() {
                return Some((
                    match self.owner {
                        "naver_v1" => "NAVER",
                        "tmap_legacy" => "TMAP",
                        _ => "NAVI",
                    },
                    ColorMode::External,
                ));
            }
            return self
                .vehicle_available
                .then_some(("vNAVI", ColorMode::Vehicle));
        }
        if self.external_active {
            Some(("NAVI", ColorMode::External))
        } else {
            self.vehicle_available
                .then_some(("vNAVI", ColorMode::Vehicle))
        }
    }
}
pub fn external_connected(remote: &str, connected: bool) -> bool {
    !strip(remote).is_empty() || connected
}
#[derive(Serialize)]
pub struct Override {
    pub active: bool,
    pub speed_kph: f64,
    pub label: String,
    pub speed_color_mode: u8,
    pub force_persist: bool,
}
pub struct SetSpeed<'a> {
    pub cruise_target: Option<f64>,
    pub desired_speed: Option<f64>,
    pub source: &'a str,
    pub provider: &'a str,
    pub set_speed_kph: f64,
    pub max_label: &'a str,
}
impl SetSpeed<'_> {
    pub fn compute(&self) -> Override {
        if let Some(target) = self
            .cruise_target
            .filter(|target| *target > self.set_speed_kph + 0.5)
        {
            return Override {
                active: true,
                speed_kph: target,
                label: "eco".into(),
                speed_color_mode: ColorMode::Eco.code(),
                force_persist: true,
            };
        }
        if let Some(speed) = self
            .desired_speed
            .filter(|speed| *speed > 0.0 && *speed < 200.0 && *speed < self.set_speed_kph)
        {
            let (label, mode) = reason(self.source, self.provider);
            return Override {
                active: true,
                speed_kph: speed,
                label,
                speed_color_mode: mode.code(),
                force_persist: true,
            };
        }
        Override {
            active: false,
            speed_kph: self.set_speed_kph,
            label: self.max_label.into(),
            speed_color_mode: ColorMode::Default.code(),
            force_persist: false,
        }
    }
}
