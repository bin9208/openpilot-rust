#[path = "common/release.rs"]
mod release;
use openpilot_dev_tools::{
    publish::{publish, Github},
    seal::{digest, Result},
};
use release::{git, Fixture};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};
const REPO: &str = "bin9208/openpilot-rust";
const GATES: &[(&str, &[&str])] = &[
    ("rust.yml", &["rust checks", "rust aarch64 build"]),
    ("fast-checks.yml", &["fast checks"]),
    ("integration.yml", &["integration gate"]),
    ("user-docs.yaml", &["check mapped user docs"]),
];
struct OwnedGithub {
    commit: String,
    store: tempfile::TempDir,
    release: Value,
    mutations: Vec<String>,
    runs: Vec<Value>,
    jobs: BTreeMap<u64, Vec<Value>>,
    checks: Vec<Value>,
    tag: Option<String>,
    full_name: String,
    bad_download: bool,
    omit_digest: bool,
    mutate_asset: Option<(String, std::path::PathBuf, Vec<u8>)>,
    fail_ci_at_draft_verification: bool,
}
impl OwnedGithub {
    fn new(root: &Path) -> Self {
        let commit = git(root, &["rev-parse", "HEAD"]);
        let mut runs = Vec::new();
        let mut jobs = BTreeMap::new();
        for (i, (path, names)) in GATES.iter().enumerate() {
            let id = (i + 1) as u64;
            runs.push(json!({"id":id,"path":format!(".github/workflows/{path}"),"head_sha":commit,"head_branch":"dev","event":"push","status":"completed","conclusion":"success","repository":{"full_name":REPO},"html_url":format!("https://github.com/{REPO}/actions/runs/{id}")}));
            jobs.insert(id, names.iter().enumerate().map(|(j,name)| json!({"id":id*10+j as u64,"name":name,"status":"completed","conclusion":"success","head_sha":commit,"app":{"id":1}})).collect());
        }
        let checks = jobs.values().flatten().cloned().collect();
        Self {
            commit,
            store: tempfile::tempdir().unwrap(),
            release: Value::Null,
            mutations: Vec::new(),
            runs,
            jobs,
            checks,
            tag: None,
            full_name: REPO.into(),
            bad_download: false,
            omit_digest: false,
            mutate_asset: None,
            fail_ci_at_draft_verification: false,
        }
    }
}
impl Github for OwnedGithub {
    fn api(
        &mut self,
        endpoint: &str,
        method: &str,
        body: Option<&Value>,
        _missing: bool,
        paginate: bool,
    ) -> Result<Value> {
        if self
            .mutate_asset
            .as_ref()
            .is_some_and(|(trigger, _, _)| method == "GET" && endpoint == trigger)
        {
            let (_, path, bytes) = self.mutate_asset.take().unwrap();
            fs::write(path, bytes)?;
        }
        if self.fail_ci_at_draft_verification
            && method == "GET"
            && endpoint == format!("repos/{REPO}/releases/10")
        {
            self.checks[0]["conclusion"] = "failure".into();
        }
        let prefix = format!("repos/{REPO}/");
        let suffix = endpoint.strip_prefix(&prefix).unwrap_or(endpoint);
        if method != "GET" {
            self.mutations.push(format!("{method} {suffix}"));
            if method == "POST" && suffix == "releases" {
                self.release = body.unwrap().clone();
                self.release["id"] = 10.into();
                self.release["assets"] = json!([]);
                self.release["html_url"] = format!(
                    "https://github.com/{REPO}/releases/tag/runtime-{}",
                    self.commit
                )
                .into();
                return Ok(self.release.clone());
            }
            if method == "PATCH" && suffix == "releases/10" {
                for (k, v) in body.unwrap().as_object().unwrap() {
                    self.release[k] = v.clone();
                }
                self.tag = Some(self.commit.clone());
                return Ok(self.release.clone());
            }
            panic!("unexpected mutation {method} {suffix}");
        }
        let result = if endpoint == format!("repos/{REPO}") {
            json!({"full_name":self.full_name})
        } else if suffix == "branches/dev/protection/required_status_checks" {
            json!({"contexts":GATES.iter().flat_map(|(_,n)|n.iter()).collect::<Vec<_>>(),"checks":[]})
        } else if suffix.starts_with("commits/") && suffix.contains("check-runs") {
            assert!(paginate);
            json!([{"check_runs":self.checks}])
        } else if suffix.starts_with("commits/") && suffix.contains("/status") {
            assert!(paginate);
            json!([{"statuses":[]}])
        } else if suffix.starts_with("actions/runs?") {
            assert!(paginate);
            json!([{"workflow_runs":self.runs}])
        } else if suffix.starts_with("actions/runs/") && suffix.contains("/jobs") {
            let id = suffix.split('/').nth(2).unwrap().parse::<u64>().unwrap();
            json!([{"jobs":self.jobs[&id]}])
        } else if suffix.starts_with("git/ref/tags/") {
            self.tag
                .as_ref()
                .map_or(Value::Null, |s| json!({"object":{"type":"commit","sha":s}}))
        } else if suffix.starts_with("releases?") {
            json!([if self.release.is_null() {
                vec![]
            } else {
                vec![self.release.clone()]
            }])
        } else if suffix == "releases/10" {
            self.release.clone()
        } else if suffix.starts_with("releases/10/assets") {
            json!([self.release["assets"]])
        } else {
            panic!("unexpected endpoint {endpoint}")
        };
        Ok(result)
    }
    fn upload(&mut self, _tag: &str, path: &Path) -> Result<()> {
        let name = path.file_name().unwrap().to_str().unwrap();
        self.mutations.push(format!("UPLOAD {name}"));
        fs::copy(path, self.store.path().join(name))?;
        let id = 100 + self.release["assets"].as_array().unwrap().len();
        let hash = if self.omit_digest {
            Value::Null
        } else {
            format!("sha256:{}", digest(path)?).into()
        };
        self.release["assets"].as_array_mut().unwrap().push(json!({"id":id,"name":name,"size":path.metadata()?.len(),"state":"uploaded","digest":hash}));
        Ok(())
    }
    fn download(&mut self, id: u64, destination: &Path, _limit: u64) -> Result<()> {
        let asset = self.release["assets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap();
        fs::copy(
            self.store.path().join(asset["name"].as_str().unwrap()),
            destination,
        )?;
        if self.bad_download {
            fs::write(destination, b"truncated")?;
        }
        Ok(())
    }
}
fn prepared() -> (Fixture, OwnedGithub) {
    let f = Fixture::new();
    f.run().unwrap();
    let g = OwnedGithub::new(&f.root);
    (f, g)
}
#[test]
fn draft_then_publish_and_download_every_byte() {
    let (f, mut g) = prepared();
    let r = publish(&f.root, &f.output, &mut g, false).unwrap();
    assert_eq!(r["source_commit"], g.commit);
    assert_eq!(r["download_verified"], true);
    assert_eq!(g.release["draft"], false);
    assert_eq!(g.release["prerelease"], true);
    assert_eq!(g.release["make_latest"], "false");
    assert_eq!(g.release["assets"].as_array().unwrap().len(), 6);
    assert_eq!(g.mutations.first().unwrap(), "POST releases");
    assert_eq!(g.mutations.last().unwrap(), "PATCH releases/10");
}
#[test]
fn failures_prevent_remote_writes() {
    for fault in [
        "wrong-repo",
        "wrong-head",
        "branch",
        "event",
        "run-failed",
        "job-skipped",
        "required-failed",
        "missing-workflow",
        "wrong-tag",
        "dirty",
        "published",
        "newer-failed",
        "missing-protection",
        "job-head",
    ] {
        let (f, mut g) = prepared();
        match fault {
            "wrong-repo" => g.full_name = "bin9208/openpilot".into(),
            "wrong-head" => g.runs[0]["head_sha"] = "f".repeat(40).into(),
            "branch" => g.runs[0]["head_branch"] = "main".into(),
            "event" => g.runs[0]["event"] = "pull_request".into(),
            "run-failed" => g.runs[0]["conclusion"] = "failure".into(),
            "job-skipped" => g.jobs.get_mut(&1).unwrap()[0]["conclusion"] = "skipped".into(),
            "required-failed" => g.checks[0]["conclusion"] = "failure".into(),
            "missing-workflow" => {
                g.runs.pop();
            }
            "wrong-tag" => g.tag = Some("f".repeat(40)),
            "dirty" => fs::write(f.root.join("source.txt"), "dirty").unwrap(),
            "published" => {
                g.release = json!({"id":10,"tag_name":format!("runtime-{}",g.commit),"draft":false})
            }
            "newer-failed" => {
                let mut r = g.runs[0].clone();
                r["id"] = 99.into();
                r["conclusion"] = "failure".into();
                g.runs.push(r);
            }
            "missing-protection" => g.checks.clear(),
            "job-head" => g.jobs.get_mut(&1).unwrap()[0]["head_sha"] = "f".repeat(40).into(),
            _ => unreachable!(),
        };
        assert!(
            publish(&f.root, &f.output, &mut g, false).is_err(),
            "{fault}"
        );
        assert!(g.mutations.is_empty(), "{fault}");
    }
}
#[test]
fn check_only_never_creates_a_release() {
    let (f, mut g) = prepared();
    let r = publish(&f.root, &f.output, &mut g, true).unwrap();
    assert_eq!(r["check_only"], true);
    assert!(g.release.is_null());
    assert!(g.mutations.is_empty());
}
#[test]
fn interrupted_draft_resumes_but_never_overwrites_mismatch() {
    let (f, mut g) = prepared();
    g.bad_download = true;
    assert!(publish(&f.root, &f.output, &mut g, false).is_err());
    assert_eq!(g.release["draft"], true);
    assert!(!g.mutations.iter().any(|s| s.starts_with("PATCH")));
    g.bad_download = false;
    let n = g.mutations.len();
    g.release["assets"][0]["digest"] = format!("sha256:{}", "f".repeat(64)).into();
    assert!(publish(&f.root, &f.output, &mut g, false).is_err());
    assert_eq!(g.mutations.len(), n);
    g.release["assets"][0]["digest"] = Value::Null;
    assert_eq!(
        publish(&f.root, &f.output, &mut g, false).unwrap()["download_verified"],
        true
    );
    assert_eq!(
        g.mutations
            .iter()
            .filter(|s| s.starts_with("UPLOAD"))
            .count(),
        6
    );
}
#[test]
fn omitted_server_digest_still_checks_download() {
    let (f, mut g) = prepared();
    g.omit_digest = true;
    assert_eq!(
        publish(&f.root, &f.output, &mut g, false).unwrap()["download_verified"],
        true
    );
}

#[test]
fn asset_change_during_first_api_never_becomes_trusted_identity() {
    for check_only in [false, true] {
        let (f, mut g) = prepared();
        g.mutate_asset = Some((
            format!("repos/{REPO}"),
            f.output.join("openpilot-runtime-install-aarch64"),
            b"changed after initial semantic validation".to_vec(),
        ));
        assert!(publish(&f.root, &f.output, &mut g, check_only).is_err());
        assert!(
            g.mutations.is_empty(),
            "asset changed before publication but remote writes occurred"
        );
    }
}
#[test]
fn extra_asset_during_draft_verification_prevents_publication() {
    let (f, mut g) = prepared();
    g.mutate_asset = Some((
        format!("repos/{REPO}/releases/10"),
        f.output.join("unreviewed-asset"),
        b"changed during draft verification".to_vec(),
    ));
    assert!(publish(&f.root, &f.output, &mut g, false).is_err());
    assert_eq!(g.release["draft"], true);
    assert!(!g.mutations.iter().any(|s| s.starts_with("PATCH")));
}

#[test]
fn changed_exact_sha_ci_during_upload_prevents_publication() {
    let (f, mut g) = prepared();
    g.fail_ci_at_draft_verification = true;
    assert!(publish(&f.root, &f.output, &mut g, false).is_err());
    assert_eq!(g.release["draft"], true);
    assert!(!g.mutations.iter().any(|s| s.starts_with("PATCH")));
}
