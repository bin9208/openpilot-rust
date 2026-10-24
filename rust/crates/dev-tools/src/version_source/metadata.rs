//! Independent OpenpilotMetadata/BuildMetadata data and property policy.
use super::{value as v, Result};
use openpilot_log_format::JsonValue;

const RELEASE: &[&str] = &[
    "release-tizi-staging",
    "release-mici-staging",
    "release-tizi",
    "release-mici",
    "nightly",
];
const TESTED: &[&str] = &["devel-staging", "nightly-dev"];
pub(super) fn from_dict(value: &JsonValue) -> Result<JsonValue> {
    let channel = v::default(value, "channel", JsonValue::text("unknown"))?;
    let original = v::default(value, "openpilot", v::object(&[]))?;
    let mut fields = Vec::new();
    for key in [
        "version",
        "release_notes",
        "git_commit",
        "git_origin",
        "git_commit_date",
        "build_style",
    ] {
        fields.push((key, v::default(&original, key, JsonValue::text("unknown"))?));
    }
    fields.push(("is_dirty", v::boolean(false)));
    Ok(build(channel, v::object(&fields)))
}
pub(super) fn build(channel: JsonValue, openpilot: JsonValue) -> JsonValue {
    let version = openpilot.get("version").unwrap();
    let origin = openpilot.get("git_origin").unwrap();
    let commit = openpilot.get("git_commit").unwrap();
    let style = openpilot.get("build_style").unwrap();
    let properties = v::object(&[
        ("short_version", v::capture(v::short_version(&version))),
        (
            "git_normalized_origin",
            v::capture(v::normalized_origin(&origin)),
        ),
        (
            "comma_remote",
            v::capture(
                v::normalized_origin(&origin)
                    .map(|value| v::boolean(value.text_eq("github.com/commaai/openpilot"))),
            ),
        ),
        (
            "tested_channel",
            v::capture(Ok(v::boolean(
                RELEASE
                    .iter()
                    .chain(TESTED)
                    .any(|name| channel.text_eq(name)),
            ))),
        ),
        (
            "release_channel",
            v::capture(Ok(v::boolean(
                RELEASE.iter().any(|name| channel.text_eq(name)),
            ))),
        ),
        (
            "canonical",
            v::capture(Ok(v::format_parts(
                &[version.clone(), commit.clone(), style],
                "-",
            ))),
        ),
        (
            "ui_description",
            v::capture(
                v::slice_six(&commit)
                    .map(|short| v::format_parts(&[version, short, channel.clone()], " / ")),
            ),
        ),
    ]);
    v::object(&[
        ("channel", channel),
        ("openpilot", openpilot),
        ("properties", properties),
    ])
}
