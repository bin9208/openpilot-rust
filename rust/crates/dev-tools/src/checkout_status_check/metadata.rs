//! All original metadata admission and filesystem cases, generated independently.
use super::{peer::Peer, require, Result};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
};
fn check(peer: &mut Peer, rows: &mut Vec<Value>, name: &str, expected: Option<&str>) -> Result<()> {
    let actual = peer.op("read", 0.0)?;
    require(
        actual["commit"].as_str() == expected,
        format!("metadata {name}: {actual}, expected {expected:?}"),
    )?;
    rows.push(json!({"name":name,"commit":actual["commit"]}));
    Ok(())
}
pub struct MetadataInput {
    pub name: &'static str,
    pub raw: Vec<u8>,
    pub expected: Option<String>,
}
pub fn inputs() -> Result<Vec<MetadataInput>> {
    let a = "a".repeat(40);
    let b = "BC09".repeat(16);
    let mut rows = Vec::new();
    let values = vec![
        ("sha40", json!(a).to_string(), Some(a.clone())),
        (
            "sha64-uppercase",
            json!(b).to_string(),
            Some(b.to_lowercase()),
        ),
        ("empty", "\"\"".into(), None),
        ("short39", json!("a".repeat(39)).to_string(), None),
        ("long41", json!("a".repeat(41)).to_string(), None),
        ("short63", json!("b".repeat(63)).to_string(), None),
        ("long65", json!("b".repeat(65)).to_string(), None),
        ("nonhex", json!("g".repeat(40)).to_string(), None),
        ("leading-space", json!(format!(" {a}")).to_string(), None),
        (
            "trailing-newline",
            json!(format!("{a}\n")).to_string(),
            None,
        ),
        (
            "nul",
            json!(format!("{}\0", "a".repeat(39))).to_string(),
            None,
        ),
        ("fullwidth", json!("Ａ".repeat(40)).to_string(), None),
        ("arabic-digits", json!("١".repeat(40)).to_string(), None),
        ("surrogate", format!("\"{}\\ud800\"", "a".repeat(39)), None),
        ("null", "null".into(), None),
        ("bool", "true".into(), None),
        ("integer", "123".into(), None),
        ("float", "1.5".into(), None),
        ("array", json!([a]).to_string(), None),
        ("object", json!({"value":a}).to_string(), None),
        ("nan-value", "NaN".into(), None),
        ("inf-value", "Infinity".into(), None),
    ];
    for (name, value, expected) in values {
        let raw = format!("{{\"openpilot\":{{\"git_commit\":{value}}}}}");
        let raw = openpilot_log_format::JsonValue::parse(&raw)?
            .to_json()?
            .into_bytes();
        rows.push(MetadataInput {
            name,
            raw,
            expected,
        });
    }
    let raw=vec![
        ("unrelated-nonfinite-surrogate",format!("{{\"other\":[NaN,Infinity,-Infinity,\"\\ud800\"],\"openpilot\":{{\"git_commit\":\"{a}\"}}}}"),Some(a.as_str())),
        ("unrelated-infinite-exponent",format!("{{\"other\":1e9999,\"openpilot\":{{\"git_commit\":\"{a}\"}}}}"),Some(a.as_str())),
        ("unrelated-wide-integer",format!("{{\"other\":{},\"openpilot\":{{\"git_commit\":\"{a}\"}}}}","9".repeat(4300)),Some(a.as_str())),
        ("integer-limit",format!("{{\"other\":{},\"openpilot\":{{\"git_commit\":\"{a}\"}}}}","9".repeat(4301)),None),
        ("last-commit-wins",format!("{{\"openpilot\":{{\"git_commit\":null,\"git_commit\":\"{a}\"}}}}"),Some(a.as_str())),
        ("last-invalid-wins",format!("{{\"openpilot\":{{\"git_commit\":\"{a}\",\"git_commit\":null}}}}"),None),
        ("last-openpilot-wins",format!("{{\"openpilot\":null,\"openpilot\":{{\"git_commit\":\"{a}\"}}}}"),Some(a.as_str())),
        ("escaped-key-and-commit",format!("{{\"open\\u0070ilot\":{{\"git_commit\":\"{}\"}}}}","\\u0061".repeat(40)),Some(a.as_str())),
        ("surrogate-key",format!("{{\"\\ud800\":true,\"openpilot\":{{\"git_commit\":\"{a}\"}}}}"),Some(a.as_str())),
        ("trailing-json",format!("{{\"openpilot\":{{\"git_commit\":\"{a}\"}}}} null"),None),
        ("utf8-bom",format!("\u{feff}{{\"openpilot\":{{\"git_commit\":\"{a}\"}}}}"),None),
        ("missing-openpilot","{}".into(),None),("missing-commit","{\"openpilot\":{}}".into(),None),("null-openpilot","{\"openpilot\":null}".into(),None),
        ("list-openpilot","{\"openpilot\":[]}".into(),None),("null-root","null".into(),None),("list-root","[]".into(),None),("string-root","\"openpilot\"".into(),None),
        ("malformed-json","{".into(),None),("empty-file","".into(),None),
    ];
    for (name, raw, expected) in raw {
        rows.push(MetadataInput {
            name,
            raw: raw.into_bytes(),
            expected: expected.map(str::to_owned),
        });
    }
    let mut invalid = b"{\"other\":\"\xff\",\"openpilot\":{\"git_commit\":\"".to_vec();
    invalid.extend(a.as_bytes());
    invalid.extend(b"\"}}");
    rows.push(MetadataInput {
        name: "invalid-utf8",
        raw: invalid,
        expected: None,
    });
    require(rows.len() == 43, "metadata byte input coverage changed")?;
    Ok(rows)
}
pub fn run(peer: &mut Peer) -> Result<Value> {
    let a = "a".repeat(40);
    let mut rows = Vec::new();
    check(peer, &mut rows, "missing-build", None)?;
    for case in inputs()? {
        peer.write(&case.raw)?;
        check(peer, &mut rows, case.name, case.expected.as_deref())?;
    }
    peer.metadata(&a)?;
    let build = peer.repo.join("build.json");
    fs::set_permissions(&build, fs::Permissions::from_mode(0o0))?;
    peer.record(json!({"fixture_fs":"build.json mode000"}))?;
    let result = check(peer, &mut rows, "unreadable-build", None);
    fs::set_permissions(&build, fs::Permissions::from_mode(0o600))?;
    result?;
    fs::remove_file(&build)?;
    fs::create_dir(&build)?;
    peer.record(json!({"fixture_fs":"build.json directory"}))?;
    check(peer, &mut rows, "build-is-directory", None)?;
    fs::remove_dir(&build)?;
    peer.metadata(&a)?;
    let git = peer.repo.join(".git");
    for (target, name, description) in [
        (".git", "git-loop-falls-back", ".git symlink loop"),
        (
            "absent",
            "git-missing-target-falls-back",
            ".git dangling symlink",
        ),
    ] {
        symlink(target, &git)?;
        peer.record(json!({"fixture_fs":description}))?;
        check(peer, &mut rows, name, Some(&a))?;
        fs::remove_file(&git)?;
    }
    fs::write(&git, "invalid git metadata\n")?;
    peer.record(json!({"fixture_fs":".git regular invalid file"}))?;
    check(peer, &mut rows, "git-file-prevents-build-fallback", None)?;
    fs::remove_file(&git)?;
    fs::create_dir(&git)?;
    peer.record(json!({"fixture_fs":".git empty directory"}))?;
    check(
        peer,
        &mut rows,
        "git-directory-prevents-build-fallback",
        None,
    )?;
    require(rows.len() == 50, "metadata scenario coverage changed")?;
    Ok(json!(rows))
}
