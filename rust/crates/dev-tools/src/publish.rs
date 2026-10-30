//! Publish only verified exact-source assets after protected dev push gates.
use crate::{
    package::{clean_source, disk_guard, validate_assets},
    seal::{Result, digest},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};
pub const REPOSITORY: &str = "bin9208/openpilot-rust";
const BASE: &str = "repos/bin9208/openpilot-rust";
const GATES: &[(&str, &[&str])] = &[
    ("rust.yml", &["rust checks", "rust aarch64 build"]),
    ("integration.yml", &["integration gate"]),
    ("user-docs.yaml", &["check mapped user docs"]),
];
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn string(v: &Value) -> Result<&str> {
    v.as_str().ok_or_else(|| "expected string".into())
}
fn integer(v: &Value) -> Result<u64> {
    v.as_u64()
        .ok_or_else(|| "expected nonnegative integer".into())
}
fn array(v: &Value) -> Result<&Vec<Value>> {
    v.as_array().ok_or_else(|| "expected array".into())
}
fn hex(v: &Value) -> Result<&str> {
    let s = string(v)?;
    require(
        s.len() == 40
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid commit identity",
    )?;
    Ok(s)
}
fn succeeded(v: &Value) -> bool {
    v["status"] == "completed" && v["conclusion"] == "success"
}
pub trait Github {
    fn api(
        &mut self,
        endpoint: &str,
        method: &str,
        body: Option<&Value>,
        missing: bool,
        paginate: bool,
    ) -> Result<Value>;
    fn upload(&mut self, tag: &str, path: &Path) -> Result<()>;
    fn download(&mut self, asset_id: u64, destination: &Path, max_bytes: u64) -> Result<()>;
}
pub struct GhGithub;
impl Github for GhGithub {
    fn api(
        &mut self,
        endpoint: &str,
        method: &str,
        body: Option<&Value>,
        missing: bool,
        paginate: bool,
    ) -> Result<Value> {
        require(
            endpoint == BASE || endpoint.starts_with(&format!("{BASE}/")),
            "wrong GitHub API repository",
        )?;
        let mut cmd = Command::new("gh");
        cmd.args([
            "api",
            "--hostname",
            "github.com",
            endpoint,
            "--method",
            method,
            "-H",
            "Accept: application/vnd.github+json",
        ]);
        if paginate {
            cmd.args(["--paginate", "--slurp"]);
        }
        if missing {
            cmd.arg("--include");
        }
        let input = body.map(serde_json::to_vec).transpose()?;
        if input.is_some() {
            cmd.args(["--input", "-"]);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(bytes) = input {
            if let Err(e) = child
                .stdin
                .take()
                .ok_or("missing GitHub stdin")?
                .write_all(&bytes)
            {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.into());
            }
        }
        drop(child.stdin.take());
        let output = child.wait_with_output()?;
        let text = String::from_utf8(output.stdout)?.replace("\r\n", "\n");
        let content = if missing {
            let (headers, content) = text
                .split_once("\n\n")
                .ok_or("missing HTTP response headers")?;
            let status = headers.lines().next().unwrap_or("");
            if status.starts_with("HTTP/") && status.split_whitespace().nth(1) == Some("404") {
                return Ok(Value::Null);
            }
            content
        } else {
            &text
        };
        require(
            output.status.success(),
            format!(
                "GitHub API failed: {endpoint}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        )?;
        Ok(serde_json::from_str(content)?)
    }
    fn upload(&mut self, tag: &str, path: &Path) -> Result<()> {
        require(
            Command::new("gh")
                .args(["release", "upload", tag])
                .arg(path.canonicalize()?)
                .args(["--repo", "https://github.com/bin9208/openpilot-rust"])
                .status()?
                .success(),
            "GitHub asset upload failed",
        )
    }
    fn download(&mut self, id: u64, destination: &Path, limit: u64) -> Result<()> {
        let mut target = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        let mut child = Command::new("gh")
            .args([
                "api",
                "--hostname",
                "github.com",
                &format!("{BASE}/releases/assets/{id}"),
                "-H",
                "Accept: application/octet-stream",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let transfer = (|| -> Result<()> {
            let mut input = child.stdout.take().ok_or("missing asset download pipe")?;
            let mut remaining = limit;
            let mut buffer = [0; 65536];
            loop {
                let n = input.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                require(
                    n as u64 <= remaining,
                    "download exceeds recorded asset size",
                )?;
                target.write_all(&buffer[..n])?;
                remaining -= n as u64;
            }
            require(remaining == 0, "truncated GitHub asset download")?;
            target.sync_all()?;
            Ok(())
        })();
        if let Err(e) = transfer {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        require(
            child.wait()?.success(),
            format!("GitHub asset download failed: {id}"),
        )
    }
}
fn rows(g: &mut impl Github, endpoint: &str, key: Option<&str>) -> Result<Vec<Value>> {
    let pages = g.api(endpoint, "GET", None, false, true)?;
    let mut out = Vec::new();
    for page in array(&pages)? {
        for value in array(key.map_or(page, |key| &page[key]))? {
            require(value.is_object(), "expected GitHub object")?;
            out.push(value.clone());
        }
    }
    Ok(out)
}
pub fn ci_evidence(g: &mut impl Github, commit: &str) -> Result<Value> {
    require(
        g.api(BASE, "GET", None, false, false)?["full_name"] == REPOSITORY,
        "wrong GitHub repository identity",
    )?;
    let policy = g.api(
        &format!("{BASE}/branches/dev/protection/required_status_checks"),
        "GET",
        None,
        false,
        false,
    )?;
    let mut required = BTreeSet::new();
    for value in array(&policy["contexts"])? {
        required.insert(string(value)?.to_owned());
    }
    let configured = array(&policy["checks"])?;
    for check in configured {
        required.insert(string(&check["context"])?.to_owned());
    }
    require(
        GATES
            .iter()
            .flat_map(|(_, names)| names.iter())
            .all(|s| required.contains(*s)),
        "required dev protection checks missing",
    )?;
    let runs = rows(
        g,
        &format!("{BASE}/actions/runs?event=push&branch=dev&head_sha={commit}&per_page=100"),
        Some("workflow_runs"),
    )?;
    let mut urls = Vec::new();
    for (workflow, names) in GATES {
        let path = format!(".github/workflows/{workflow}");
        let candidates = runs
            .iter()
            .filter(|v| v["path"] == path)
            .collect::<Vec<_>>();
        for c in &candidates {
            integer(&c["id"])?;
        }
        let run = candidates
            .into_iter()
            .max_by_key(|v| v["id"].as_u64())
            .ok_or_else(|| format!("missing exact-SHA dev push workflow: {path}"))?;
        require(
            run["head_sha"] == commit
                && run["head_branch"] == "dev"
                && run["event"] == "push"
                && run["repository"]["full_name"] == REPOSITORY
                && succeeded(run),
            format!("incomplete/failed dev push workflow: {path}"),
        )?;
        let jobs = rows(
            g,
            &format!(
                "{BASE}/actions/runs/{}/jobs?filter=latest&per_page=100",
                integer(&run["id"])?
            ),
            Some("jobs"),
        )?;
        for name in *names {
            let matching = jobs
                .iter()
                .filter(|v| v["name"] == *name)
                .collect::<Vec<_>>();
            require(
                matching.len() == 1 && succeeded(matching[0]) && matching[0]["head_sha"] == commit,
                format!("failed/missing exact-SHA gate: {name}"),
            )?;
        }
        urls.push(string(&run["html_url"])?.to_owned());
    }
    let checks = rows(
        g,
        &format!("{BASE}/commits/{commit}/check-runs?filter=latest&per_page=100"),
        Some("check_runs"),
    )?;
    let statuses = rows(
        g,
        &format!("{BASE}/commits/{commit}/status?per_page=100"),
        Some("statuses"),
    )?;
    for name in &required {
        let app_ids = configured
            .iter()
            .filter(|c| c["context"] == *name)
            .filter_map(|c| c["app_id"].as_i64())
            .filter(|id| *id != -1)
            .collect::<BTreeSet<_>>();
        let matching = checks
            .iter()
            .filter(|c| {
                c["name"] == *name
                    && (app_ids.is_empty()
                        || c["app"]["id"]
                            .as_i64()
                            .is_some_and(|id| app_ids.contains(&id)))
            })
            .collect::<Vec<_>>();
        for c in &matching {
            integer(&c["id"])?;
        }
        if let Some(check) = matching.into_iter().max_by_key(|v| v["id"].as_u64()) {
            require(succeeded(check), format!("required check failed: {name}"))?;
        } else {
            let matching = statuses
                .iter()
                .filter(|v| v["context"] == *name)
                .collect::<Vec<_>>();
            for s in &matching {
                integer(&s["id"])?;
            }
            require(
                app_ids.is_empty()
                    && matching
                        .into_iter()
                        .max_by_key(|v| v["id"].as_u64())
                        .is_some_and(|v| v["state"] == "success"),
                format!("required status missing/failed: {name}"),
            )?;
        }
    }
    Ok(json!({"required_checks":required,"dev_push_runs":urls}))
}
fn tag_commit(g: &mut impl Github, tag: &str) -> Result<Option<String>> {
    let reference = g.api(
        &format!("{BASE}/git/ref/tags/{tag}"),
        "GET",
        None,
        true,
        false,
    )?;
    if reference.is_null() {
        return Ok(None);
    }
    let mut target = reference["object"].clone();
    for _ in 0..4 {
        if target["type"] == "commit" {
            return Ok(Some(hex(&target["sha"])?.to_owned()));
        }
        require(target["type"] == "tag", "unexpected release tag target")?;
        target = g.api(
            &format!("{BASE}/git/tags/{}", hex(&target["sha"])?),
            "GET",
            None,
            false,
            false,
        )?["object"]
            .clone();
    }
    Err("release tag nesting exceeds limit".into())
}
fn verify_remote(
    g: &mut impl Github,
    id: u64,
    local: &BTreeMap<String, Value>,
    assets: &Path,
    complete: bool,
) -> Result<BTreeSet<String>> {
    let remote = rows(
        g,
        &format!("{BASE}/releases/{id}/assets?per_page=100"),
        None,
    )?;
    let mut seen = BTreeSet::new();
    for asset in remote {
        let name = string(&asset["name"])?;
        require(
            local.contains_key(name) && !seen.contains(name) && asset["state"] == "uploaded",
            format!("unexpected/duplicate/incomplete uploaded asset: {name}"),
        )?;
        let expected = &local[name];
        require(
            asset["size"] == expected["bytes"],
            format!("uploaded asset size mismatch: {name}"),
        )?;
        let hash = string(&expected["sha256"])?;
        require(
            asset["digest"].is_null()
                || asset["digest"] == ""
                || asset["digest"] == format!("sha256:{hash}"),
            format!("uploaded asset digest mismatch: {name}"),
        )?;
        let size = integer(&expected["bytes"])?;
        disk_guard(
            assets,
            size.checked_add(64 * 1024 * 1024)
                .ok_or("asset size overflow")?,
        )?;
        let temp = tempfile::Builder::new()
            .prefix(".runtime-download-")
            .tempdir_in(assets.parent().ok_or("missing asset parent")?)?;
        let path = temp.path().join(name);
        g.download(integer(&asset["id"])?, &path, size)?;
        require(
            path.metadata()?.len() == size && digest(&path)? == hash,
            format!("downloaded asset size/hash mismatch: {name}"),
        )?;
        seen.insert(name.to_owned());
    }
    require(
        !complete || seen.iter().eq(local.keys()),
        "missing uploaded release assets",
    )?;
    Ok(seen)
}
fn asset_identity(assets: &Path) -> Result<BTreeMap<String, Value>> {
    let mut local = BTreeMap::new();
    for entry in fs::read_dir(assets)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("invalid asset name")?
            .to_owned();
        let metadata = fs::symlink_metadata(&path)?;
        require(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() > 0
                && metadata.len() < 2 * 1024 * 1024 * 1024,
            "invalid release asset while freezing identity",
        )?;
        local.insert(
            name,
            json!({"sha256":digest(&path)?,"bytes":path.metadata()?.len()}),
        );
    }
    Ok(local)
}
fn validate_frozen_assets(
    root: &Path,
    assets: &Path,
    descriptor: &Value,
    identity: &BTreeMap<String, Value>,
) -> Result<()> {
    require(
        &asset_identity(assets)? == identity,
        "local assets changed from frozen publication identity",
    )?;
    require(
        &validate_assets(root, assets)? == descriptor,
        "release descriptor changed from frozen publication identity",
    )?;
    require(
        &asset_identity(assets)? == identity,
        "local assets changed during publication validation",
    )
}
pub fn publish(root: &Path, assets: &Path, g: &mut impl Github, check_only: bool) -> Result<Value> {
    let local = asset_identity(assets)?;
    let descriptor = validate_assets(root, assets)?;
    require(
        asset_identity(assets)? == local,
        "local assets changed during initial publication validation",
    )?;
    let commit = hex(&descriptor["source_commit"])?;
    let ci = ci_evidence(g, commit)?;
    let tag = format!("runtime-{commit}");
    require(
        tag_commit(g, &tag)?.is_none_or(|s| s == commit),
        "existing release tag points to different source",
    )?;
    let identity = json!({"repository":REPOSITORY,"source_commit":commit,"assets":local});
    let body = format!(
        "Exact-source runtime release identity:\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&identity)?
    );
    let releases = rows(g, &format!("{BASE}/releases?per_page=100"), None)?
        .into_iter()
        .filter(|v| v["tag_name"] == tag)
        .collect::<Vec<_>>();
    require(releases.len() <= 1, "duplicate release identity")?;
    let mut release = releases.into_iter().next();
    let existing = if let Some(r) = &release {
        require(
            r["draft"] == true,
            "refusing to overwrite published runtime release",
        )?;
        require(
            r["target_commitish"] == commit && r["prerelease"] == true && r["body"] == body,
            "existing draft identity/assets differ",
        )?;
        verify_remote(g, integer(&r["id"])?, &local, assets, false)?
    } else {
        BTreeSet::new()
    };
    validate_frozen_assets(root, assets, &descriptor, &local)?;
    if check_only {
        return Ok(json!({"source_commit":commit,"tag":tag,"ci":ci,"check_only":true}));
    }
    let size = local
        .values()
        .map(|v| integer(&v["bytes"]))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .max()
        .ok_or("no assets")?;
    disk_guard(
        assets,
        size.checked_add(64 * 1024 * 1024)
            .ok_or("asset size overflow")?,
    )?;
    require(
        clean_source(root)?.0 == commit,
        "source changed before publication",
    )?;
    if release.is_none() {
        let r=g.api(&format!("{BASE}/releases"),"POST",Some(&json!({"tag_name":tag,"target_commitish":commit,"name":tag,"body":body,"draft":true,"prerelease":true,"make_latest":"false"})),false,false)?;
        require(
            r["draft"] == true && r["tag_name"] == tag && r["target_commitish"] == commit,
            "created draft identity mismatch",
        )?;
        release = Some(r);
    }
    let id = integer(&release.ok_or("missing release")?["id"])?;
    for name in local.keys() {
        if !existing.contains(name) {
            g.upload(&tag, &assets.join(name))?;
        }
    }
    verify_remote(g, id, &local, assets, true)?;
    let current = g.api(&format!("{BASE}/releases/{id}"), "GET", None, false, false)?;
    require(
        current["draft"] == true
            && current["body"] == body
            && current["target_commitish"] == commit
            && current["tag_name"] == tag
            && current["prerelease"] == true,
        "draft changed before publication",
    )?;
    require(
        tag_commit(g, &tag)?.is_none_or(|s| s == commit),
        "release tag changed before publication",
    )?;
    require(
        clean_source(root)?.0 == commit,
        "source changed before publishing verified draft",
    )?;
    // Upload/download verification can outlive a check rerun or protection change.
    let ci = ci_evidence(g, commit)?;
    validate_frozen_assets(root, assets, &descriptor, &local)?;
    let published = g.api(
        &format!("{BASE}/releases/{id}"),
        "PATCH",
        Some(&json!({"draft":false,"prerelease":true,"make_latest":"false"})),
        false,
        false,
    )?;
    require(
        published["draft"] == false
            && published["prerelease"] == true
            && tag_commit(g, &tag)?.as_deref() == Some(commit),
        "published release identity mismatch",
    )?;
    verify_remote(g, id, &local, assets, true)?;
    Ok(
        json!({"source_commit":commit,"tag":tag,"release_url":published["html_url"],"ci":ci,"assets":local,"download_verified":true}),
    )
}
