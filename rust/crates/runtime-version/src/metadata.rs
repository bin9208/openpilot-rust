use crate::{git, Error};
use openpilot_logmessaged::{JsonValue, JsonView};

pub const RELEASE_BRANCHES: &[&str] = &[
    "release-tizi-staging",
    "release-mici-staging",
    "release-tizi",
    "release-mici",
    "nightly",
];
pub const TESTED_BRANCHES: &[&str] = &[
    "release-tizi-staging",
    "release-mici-staging",
    "release-tizi",
    "release-mici",
    "nightly",
    "devel-staging",
    "nightly-dev",
];
pub const TRAINING_VERSION: &str = "0.2.0";
pub const TERMS_VERSION: &str = "2";
pub const BUILD_METADATA_FILENAME: &str = "build.json";

/// Source dataclass fields are intentionally untyped at runtime: preserve JSON values.
#[derive(Clone)]
pub struct OpenpilotMetadata {
    pub version: JsonValue,
    pub release_notes: JsonValue,
    pub git_commit: JsonValue,
    pub git_origin: JsonValue,
    pub git_commit_date: JsonValue,
    pub build_style: JsonValue,
    pub is_dirty: bool,
}
#[derive(Clone)]
pub struct BuildMetadata {
    pub channel: JsonValue,
    pub openpilot: OpenpilotMetadata,
}
impl OpenpilotMetadata {
    pub fn short_version(&self) -> Result<JsonValue, Error> {
        let JsonView::Text(points) = self.version.view() else {
            return Err(Error::Attribute("version.split"));
        };
        crate::text(
            points
                .iter()
                .take_while(|&&point| point != u32::from('-'))
                .copied()
                .collect(),
        )
    }
    pub fn git_normalized_origin(&self) -> Result<JsonValue, Error> {
        let JsonView::Text(points) = self.git_origin.view() else {
            return Err(Error::Attribute("git_origin.replace"));
        };
        let mut points = points.to_vec();
        for (from, to) in [("git@", ""), (".git", ""), ("https://", ""), (":", "/")] {
            let from: Vec<_> = from.chars().map(u32::from).collect();
            if let Some(index) = points.windows(from.len()).position(|window| window == from) {
                points.splice(index..index + from.len(), to.chars().map(u32::from));
            }
        }
        crate::text(points)
    }
    pub fn comma_remote(&self) -> Result<bool, Error> {
        Ok(self
            .git_normalized_origin()?
            .text_eq("github.com/commaai/openpilot"))
    }
}
impl BuildMetadata {
    pub fn tested_channel(&self) -> bool {
        TESTED_BRANCHES
            .iter()
            .any(|channel| self.channel.text_eq(channel))
    }
    pub fn release_channel(&self) -> bool {
        RELEASE_BRANCHES
            .iter()
            .any(|channel| self.channel.text_eq(channel))
    }
    pub fn canonical(&self) -> Result<JsonValue, Error> {
        crate::text(crate::python::join(
            &[
                &self.openpilot.version,
                &self.openpilot.git_commit,
                &self.openpilot.build_style,
            ],
            "-",
        )?)
    }
    pub fn ui_description(&self) -> Result<JsonValue, Error> {
        let commit = match self.openpilot.git_commit.view() {
            JsonView::Text(points) => crate::text(points.iter().take(6).copied().collect())?,
            JsonView::Array(values) => {
                let json = values
                    .iter()
                    .take(6)
                    .map(JsonValue::to_json)
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ");
                JsonValue::parse(&format!("[{json}]"))?
            }
            JsonView::Object(_) => return Err(Error::Key),
            JsonView::Null | JsonView::Bool(_) | JsonView::Integer(_) | JsonView::Float(_) => {
                return Err(Error::Type("git_commit slice"))
            }
        };
        crate::text(crate::python::join(
            &[&self.openpilot.version, &commit, &self.channel],
            " / ",
        )?)
    }
}
/// Only the two objects are required; individual values retain source types and defaults.
pub fn build_metadata_from_dict(build: &JsonValue) -> Result<BuildMetadata, Error> {
    if !build.is_object() {
        return Err(Error::Attribute("build_metadata.get"));
    }
    let openpilot = build.get("openpilot");
    if openpilot.as_ref().is_some_and(|value| !value.is_object()) {
        return Err(Error::Attribute("openpilot.get"));
    }
    let field = |key| {
        openpilot
            .as_ref()
            .and_then(|value| value.get(key))
            .unwrap_or_else(|| JsonValue::text("unknown"))
    };
    Ok(BuildMetadata {
        channel: build
            .get("channel")
            .unwrap_or_else(|| JsonValue::text("unknown")),
        openpilot: OpenpilotMetadata {
            version: field("version"),
            release_notes: field("release_notes"),
            git_commit: field("git_commit"),
            git_origin: field("git_origin"),
            git_commit_date: field("git_commit_date"),
            build_style: field("build_style"),
            is_dirty: false,
        },
    })
}

pub(crate) fn from_source(path: &std::path::Path) -> Result<BuildMetadata, Error> {
    Ok(BuildMetadata {
        channel: JsonValue::text(&git::get_short_branch(Some(path))?),
        openpilot: OpenpilotMetadata {
            version: JsonValue::text(&crate::get_version(path)?),
            release_notes: JsonValue::text(&crate::get_release_notes(path)?),
            git_commit: JsonValue::text(&git::get_head(Some(path))?),
            git_origin: JsonValue::text(&git::get_origin(Some(path))?),
            git_commit_date: JsonValue::text(&git::get_head_date(Some(path))?),
            build_style: JsonValue::text("unknown"),
            is_dirty: crate::is_dirty(path)?,
        },
    })
}
