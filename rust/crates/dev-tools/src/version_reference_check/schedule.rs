//! Request order and literals translated from unchanged check_version_reference.py.
//! This module never reads captured responses or candidate outputs.
use super::{json::object, Result};
use openpilot_log_format::JsonValue as J;
use std::path::Path;

pub(super) const NAMES: &[&str] = &[
    "clean",
    "dirty",
    "staged",
    "untracked",
    "prebuilt",
    "nontracking",
    "missingorigin",
    "detached",
    "trackingremote",
    "dirty-process-cwd",
    "dirty-cwd-missing-origin",
    "cache-and-cwd",
    "empty-default-cache",
    "dirty-exception-cache",
    "boundaries",
    "build-json",
    "missing-executable",
];
fn path(root: &Path, name: &str) -> J {
    crate::python_fs::json_value(root.join(name).as_os_str())
}
fn call(op: &str, fields: &[(&str, J)]) -> Result<J> {
    let mut all = vec![("op", J::text(op))];
    all.extend_from_slice(fields);
    object(&all)
}
fn at(op: &str, p: J) -> Result<J> {
    call(op, &[("path", p)])
}
fn git(method: &str, p: Option<J>) -> Result<J> {
    let mut fields = vec![("method", J::text(method))];
    if let Some(p) = p {
        fields.push(("cwd", p));
    }
    call("git", &fields)
}
fn revision(p: J, revision: &str) -> Result<J> {
    call(
        "git",
        &[
            ("method", J::text("commit")),
            ("cwd", p),
            ("revision", J::text(revision)),
        ],
    )
}
fn from_json(source: &str) -> Result<J> {
    call("from_json", &[("source", J::text(source))])
}

pub(super) fn requests(root: &Path, name: &str) -> Result<Vec<J>> {
    let p = |name| path(root, name);
    let mut out = Vec::new();
    match name {
        "dirty-process-cwd" => out.extend([
            at("dirty", p("prebuilt-without-git"))?,
            at("chdir", p("prebuilt-without-git"))?,
            at("dirty", p("cwd-oddity"))?,
        ]),
        "dirty-cwd-missing-origin" => out.push(at("dirty", p("cwd-oddity"))?),
        "cache-and-cwd" => {
            for _ in 0..2 {
                for op in ["metadata", "dirty", "prebuilt"] {
                    out.push(at(op, p("cache"))?);
                }
                out.push(revision(p("cache"), "missing")?);
            }
            out.extend([
                git("commit", Some(p("cache")))?,
                git("commit_date", Some(p("cache")))?,
                at("chdir", p("elsewhere"))?,
                at("dirty", p("elsewhere"))?,
                git("origin", None)?,
                git("normalized_origin", None)?,
            ]);
        }
        "empty-default-cache" => out.extend([
            git("origin", Some(p("empty-cache")))?,
            git("origin", Some(p("empty-cache")))?,
            git(
                "origin",
                Some(
                    crate::python_fs::json_value(root.join("empty-cache").as_os_str())
                        .to_utf8()
                        .map(|s| J::text(&(s + "/.")))
                        .ok_or("fixture path is not Unicode")?,
                ),
            )?,
        ]),
        "dirty-exception-cache" => out.extend([
            at("dirty", p("dirty-exception"))?,
            at("dirty", p("dirty-exception"))?,
        ]),
        "boundaries" => {
            out.extend([
                at("metadata", p("empty"))?,
                at("dirty", p("empty"))?,
                git("origin", Some(p("empty")))?,
                git("commit", Some(p("not-yet-present")))?,
                git("commit", Some(p("not-yet-present")))?,
                git("origin", Some(p("boundaries")))?,
                at("dirty", p("boundaries"))?,
                git("origin", Some(p("boundaries")))?,
                at("dirty", p("boundaries"))?,
                at("release_notes", p("boundaries"))?,
            ]);
            for _ in 0..3 {
                out.push(at("version", p("boundaries"))?);
            }
            for name in ["gitfile", "unreadable", "empty", "empty"] {
                out.push(at("metadata", p(name))?);
            }
        }
        "build-json" => {
            for _ in 0..5 {
                out.push(at("metadata", p("json"))?);
            }
            for source in [
                "{}",
                "[]",
                "null",
                "{\"openpilot\":null}",
                "{\"channel\":null}",
                "{\"channel\":42}",
                "{\"channel\":[\"nightly\"]}",
                "{\"openpilot\":{\"is_dirty\":true}}",
            ] {
                out.push(from_json(source)?);
            }
            for channel in [
                "release-tizi-staging",
                "release-mici-staging",
                "release-tizi",
                "release-mici",
                "nightly",
                "devel-staging",
                "nightly-dev",
                "nightly-dev-x",
                "unknown",
            ] {
                out.push(from_json(
                    &object(&[("channel", J::text(channel))])?.to_json()?,
                )?);
            }
            for origin in [
                r#""git@github.com:commaai/openpilot.git""#,
                r#""https://github.com/commaai/openpilot.git""#,
                r#""git@github.com:fork/openpilot.git""#,
                r#""xgit@git@host.git.githttps://https://:tail""#,
                r#""http://github.com/commaai/openpilot""#,
                r#""ssh://git@github.com/commaai/openpilot.git""#,
                r#""github.com/commaai/openpilot/""#,
                r#""\ud800:abc.git""#,
            ] {
                out.push(from_json(
                    &object(&[("openpilot", object(&[("git_origin", J::parse(origin)?)])?)])?
                        .to_json()?,
                )?);
            }
            let negative = format!("-1{}", "0".repeat(100));
            let values = [
                "null",
                "true",
                "false",
                "42",
                negative.as_str(),
                "1.234",
                "-0.0",
                "NaN",
                "Infinity",
                r#""a-b-c""#,
                r#""한글😀abcd""#,
                r#""\ud800abc""#,
                "[]",
                r#"["a", "\ud800", "\u001c", "\u200b", "\ue000", "\u0378", "'\""]"#,
                r#"{"z": 1, "a": "value"}"#,
            ];
            for value in values {
                let value = J::parse(value)?;
                let fields = [
                    "version",
                    "git_commit",
                    "git_origin",
                    "release_notes",
                    "git_commit_date",
                    "build_style",
                ]
                .map(|key| (key, value.clone()));
                out.push(from_json(
                    &object(&[("channel", value), ("openpilot", object(&fields)?)])?.to_json()?,
                )?);
            }
            out.push(from_json(
                r#"{"openpilot":{"version":"old","version":"new"}}"#,
            )?);
            out.push(from_json(r#"{"openpilot":{"version":1e999}}"#)?);
            out.push(from_json(&format!(
                "{{\"openpilot\":{{\"version\":{}}}}}",
                "1".repeat(4301)
            ))?);
        }
        "missing-executable" => {
            out.extend([git("origin", Some(p(name)))?, git("origin", Some(p(name)))?])
        }
        _ => {
            out.push(at("metadata", p(name))?);
            for method in [
                "commit",
                "commit_date",
                "short_branch",
                "branch",
                "origin",
                "normalized_origin",
            ] {
                out.push(git(method, Some(p(name)))?);
            }
            out.extend([at("dirty", p(name))?, at("prebuilt", p(name))?]);
        }
    }
    Ok(out)
}
