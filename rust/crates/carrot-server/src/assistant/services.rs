//! Voice setting edits, one-shot driving requests, Tools actions and
//! connection moves. Port of carrot's `server/services/assistant_*.py`.
use super::permissions::{digest, identity, invalid, token, Fault, Permissions};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

pub const ROOT: &str = "/dev/shm/carrot-assistant-lane";
const DENIED: [&str; 3] = ["DriverMonitoringEnabled", "DriverMonitoringSessionDisabled", "JetsonLegacyUpdatePending"];
const TERMINAL: [&str; 6] = ["completed", "rejected", "expired", "interrupted", "unverified", "cancelled"];

pub fn monotonic() -> f64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    t.tv_sec as f64 + t.tv_nsec as f64 * 1e-9
}

fn read_json(path: &Path) -> Value {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null)
}

fn atomic_json(path: &Path, value: &Value) -> Result<(), Fault> {
    let io = |e: std::io::Error| Fault::Invalid(e.to_string());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, value.to_string()).map_err(io)?;
    std::fs::rename(&temporary, path).map_err(io)
}

fn number(value: Option<&Value>) -> Result<f64, Fault> {
    match value {
        Some(Value::Number(n)) => n.as_f64().filter(|v| v.is_finite()).ok_or_else(|| invalid("A finite numeric value is required")),
        _ => Err(invalid("A finite numeric value is required")),
    }
}

/// `Decimal.quantize(1, ROUND_HALF_UP)`.
fn round_half_up(value: f64) -> f64 {
    value.signum() * (value.abs() + 0.5).floor()
}

/// The environment a setting edit runs in.
pub trait Device {
    fn catalog(&self) -> Result<Map<String, Value>, Fault>;
    fn read(&self, name: &str) -> Result<Option<i64>, Fault>;
    fn write(&self, name: &str, value: i64) -> Result<(), Fault>;
    fn record(&self, name: &str, before: i64, after: i64);
}

pub struct Settings {
    pub permissions: Permissions,
    proposals: HashMap<String, Value>,
    results: HashMap<(String, String), (f64, Value)>,
}

impl Settings {
    pub fn new(path: &Path) -> Result<Self, Fault> {
        Ok(Self { permissions: Permissions::load(path)?, proposals: HashMap::new(), results: HashMap::new() })
    }

    pub fn revoke(&mut self, token: &str, origin: &str) -> Result<(), Fault> {
        let owner = self.permissions.revoke(token, origin)?;
        self.proposals.retain(|_, v| v["owner"] != json!(owner));
        self.results.retain(|k, _| k.0 != owner);
        Ok(())
    }

    fn snapshot(device: &dyn Device, name: &str) -> Result<Value, Fault> {
        let catalog = device.catalog()?;
        let meta = catalog.get(name).filter(|_| !DENIED.contains(&name)).ok_or_else(|| invalid("This setting is not available for voice access"))?;
        let low = number(meta.get("min"))?;
        let high = number(meta.get("max"))?;
        let step = number(Some(meta.get("unit").unwrap_or(&json!(1))))?;
        if high < low || step <= 0. || [low, high, step].iter().any(|v| v.trunc() != *v) {
            return Err(invalid("Only bounded integer catalog settings are supported"));
        }
        let current = device.read(name)?.ok_or_else(|| invalid("No current device value; a catalog default is not a live value"))?;
        let title: String = meta.get("title").and_then(Value::as_str).unwrap_or(name).chars().take(160).collect();
        Ok(json!({"name": name, "title": title, "current": current, "min": low as i64, "max": high as i64, "step": step as i64,
            "unit_note": "Values are the integers displayed/stored by Carrot Web; honor any scale in the setting title.",
            "step_note": "step is the web button increment, not a required multiple or an offset from min. Any in-range integer may be specified.",
            "stationary_required": false}))
    }

    pub fn execute(&mut self, device: &dyn Device, token: &str, origin: &str, command: &Value) -> Result<Value, Fault> {
        let owner = self.permissions.authorize(token, origin)?;
        let now = monotonic();
        self.proposals.retain(|_, v| v["until"].as_f64().unwrap_or(0.) > now);
        self.results.retain(|_, v| v.0 > now);
        let request_id = command.get("id").and_then(Value::as_str).filter(|s| s.chars().count() == 43).ok_or_else(|| invalid("Invalid request identity"))?;
        let key = (owner.clone(), request_id.to_owned());
        if let Some((_, result)) = self.results.get(&key) {
            return Ok(result.clone());
        }
        if self.results.len() >= 1000 {
            return Err(invalid("Too many requests; try later"));
        }
        let operation = command.get("operation").and_then(Value::as_str).unwrap_or_default();
        let result = match operation {
            "commit" => {
                let proposal_id = command.get("proposal_id").and_then(Value::as_str).unwrap_or_default();
                let proposal = self.proposals.get(proposal_id).filter(|p| p["owner"] == json!(owner)).cloned()
                    .ok_or_else(|| invalid("Proposal expired or belongs to another connection; prepare it again"))?;
                // Consume before writing: retrying an uncertain result never applies twice.
                self.proposals.remove(proposal_id);
                let name = proposal["name"].as_str().unwrap_or_default().to_owned();
                let snapshot = Self::snapshot(device, &name)?;
                if ["current", "min", "max", "step"].iter().any(|k| snapshot[*k] != proposal[*k]) {
                    return Err(invalid("Setting value or definition changed; prepare and confirm again"));
                }
                self.results.insert(key.clone(), (now + 120., json!({"status": "failed", "name": name, "error": "Application not verified; read current value before retrying"})));
                let proposed = proposal["proposed"].as_i64().unwrap_or_default();
                device.write(&name, proposed)?;
                let actual = device.read(&name)?;
                if actual != Some(proposed) {
                    return Err(invalid("Device readback differs; application was not confirmed"));
                }
                let previous = proposal["current"].as_i64().unwrap_or_default();
                device.record(&name, previous, proposed);
                json!({"status": "applied", "name": name, "previous": previous, "current": proposed})
            }
            "read" | "prepare" => {
                let name = command.get("name").and_then(Value::as_str).unwrap_or_default();
                let snapshot = Self::snapshot(device, name)?;
                let mut result = snapshot.clone();
                result["status"] = json!("read");
                if operation == "prepare" {
                    let amount = number(command.get("value"))?;
                    let current = snapshot["current"].as_f64().unwrap_or_default();
                    let (low, high) = (snapshot["min"].as_f64().unwrap_or_default(), snapshot["max"].as_f64().unwrap_or_default());
                    let target = match command.get("mode").and_then(Value::as_str) {
                        Some("absolute") => {
                            if amount.trunc() != amount {
                                return Err(invalid("Specify an exact integer displayed/stored by Carrot Web"));
                            }
                            amount
                        }
                        Some(mode @ ("delta" | "percent")) => {
                            if !(low <= current && current <= high) {
                                return Err(invalid("Current value has a special meaning; choose an explicit absolute value"));
                            }
                            current + if mode == "delta" { amount } else { current.abs() * amount / 100. }
                        }
                        _ => return Err(invalid("Choose absolute, delta or percent")),
                    };
                    if !(low <= target && target <= high) {
                        return Err(invalid("Requested value is outside the allowed range; no value was changed"));
                    }
                    let proposed = round_half_up(target);
                    if !(low <= proposed && proposed <= high) || proposed == current {
                        return Err(invalid("No integer value change; choose another value"));
                    }
                    if self.proposals.len() >= 100 {
                        return Err(invalid("Too many pending proposals"));
                    }
                    // A correction replaces the previous proposal for this owner.
                    self.proposals.retain(|_, v| v["owner"] != json!(owner));
                    let proposal_id = super::permissions::token();
                    let mut proposal = snapshot.clone();
                    proposal["proposed"] = json!(proposed as i64);
                    proposal["owner"] = json!(owner);
                    proposal["until"] = json!(now + 60.);
                    self.proposals.insert(proposal_id.clone(), proposal);
                    result["status"] = json!("prepared");
                    result["proposed"] = json!(proposed as i64);
                    result["proposal_id"] = json!(proposal_id);
                    result["expires_in_seconds"] = json!(60);
                    result["confirmation_required"] = json!(true);
                }
                result
            }
            "cancel" => {
                self.proposals.retain(|_, v| v["owner"] != json!(owner));
                json!({"status": "cancelled"})
            }
            _ => return Err(invalid("Unknown setting operation")),
        };
        self.results.insert(key, (now + 120., result.clone()));
        Ok(result)
    }
}

const ACTIONS: [&str; 5] = ["lane_left", "lane_right", "speed_up", "speed_down", "speed_set"];

pub struct Drive {
    pub permissions: Permissions,
    root: PathBuf,
    requests: HashMap<String, Value>,
}

fn finished(status: &str) -> bool {
    TERMINAL.contains(&status) || matches!(status, "applied" | "unchanged")
}

impl Drive {
    pub fn new(path: &Path) -> Result<Self, Fault> {
        Ok(Self { permissions: Permissions::load(path)?, root: ROOT.into(), requests: HashMap::new() })
    }

    /// Runtime ownership survives a web-server restart during preparation.
    pub fn cancel_owned(root: &Path, owner: &str) {
        for path in [root.to_owned(), root.join("speed")] {
            let request = read_json(&path.join("request.json"));
            if request.get("owner").and_then(Value::as_str) == Some(owner)
                && request.get("id").and_then(Value::as_str).is_some_and(identity)
            {
                let _ = atomic_json(&path.join("cancel.json"), &json!({"id": request["id"]}));
            }
        }
    }

    pub fn revoke(&mut self, token: &str, origin: &str) -> Result<(), Fault> {
        let owner = self.permissions.revoke(token, origin)?;
        Self::cancel_owned(&self.root, &owner);
        Ok(())
    }

    pub fn execute(&mut self, token: &str, origin: &str, command: &Value) -> Result<Value, Fault> {
        let owner = self.permissions.authorize(token, origin)?;
        let now = monotonic();
        self.requests.retain(|_, v| now - v["time"].as_f64().unwrap_or(0.) < 120.);
        let id = command.get("id").and_then(Value::as_str).filter(|s| identity(s)).ok_or_else(|| invalid("Invalid request identity"))?;
        if let Some(existing) = self.requests.get(id) {
            if existing["owner"] != json!(owner) {
                return Err(Fault::Permission("Request belongs to another connection".into()));
            }
            return Ok(self.status(existing));
        }
        if command.get("operation").and_then(Value::as_str) != Some("execute") {
            return Err(invalid("Request is no longer available; do not repeat it automatically"));
        }
        let action = command.get("action").and_then(Value::as_str).filter(|a| ACTIONS.contains(a)).ok_or_else(|| invalid("Unsupported driving command"))?;
        let value = command.get("value").cloned().unwrap_or(Value::Null);
        if matches!(action, "speed_down" | "speed_set") {
            if !value.as_i64().is_some_and(|v| (1..=199).contains(&v)) || value.is_f64() {
                return Err(invalid("An explicit integer speed or decrease from 1 to 199 km/h is required"));
            }
        } else if !value.is_null() {
            return Err(invalid("This action does not accept a speed"));
        }
        if self.requests.len() >= 1000 {
            return Err(invalid("Too many requests"));
        }
        // One maneuver across all approved web origins; never queue the next one.
        for path in [self.root.clone(), self.root.join("speed")] {
            let pending = read_json(&path.join("request.json"));
            let result = read_json(&path.join("result.json"));
            if pending.get("time").and_then(Value::as_f64).is_some_and(|t| (0. ..20.).contains(&(now - t)))
                && (result.get("id") != pending.get("id")
                    || !result.get("status").and_then(Value::as_str).is_some_and(finished))
            {
                return Err(invalid("A driving request is already pending; check its result"));
            }
        }
        let lane = action.starts_with("lane_");
        let root = if lane { self.root.clone() } else { self.root.join("speed") };
        let request = json!({"id": id, "owner": owner, "action": action, "value": value, "time": now, "root": root});
        // Consume before publishing: uncertain writes are never retried.
        self.requests.insert(id.to_owned(), request.clone());
        atomic_json(&root.join("request.json"), &json!({"id": id, "owner": owner, "time": now, "action": action, "value": value,
            "direction": if lane { json!(action.trim_start_matches("lane_")) } else { Value::Null }}))?;
        Ok(self.status(&request))
    }

    pub fn status(&self, request: &Value) -> Value {
        let now = monotonic();
        let root = PathBuf::from(request["root"].as_str().unwrap_or(ROOT));
        let result = read_json(&root.join("result.json"));
        let time = request["time"].as_f64().unwrap_or(0.);
        if result.get("id") == request.get("id") {
            if let (Some(status), Some(stamp)) = (result.get("status").and_then(Value::as_str), result.get("time").and_then(Value::as_f64)) {
                if (finished(status) || matches!(status, "waiting" | "running")) && time <= stamp && stamp <= now && (finished(status) || now - stamp < 1.) {
                    let mut out = Map::new();
                    for key in ["id", "status", "direction", "reason", "previous", "current"] {
                        if let Some(v) = result.get(key) {
                            out.insert(key.into(), v.clone());
                        }
                    }
                    return Value::Object(out);
                }
            }
        }
        let waiting = now - time <= 1.;
        json!({"id": request["id"], "status": if waiting { "waiting" } else { "unverified" },
            "reason": if waiting { "awaiting_vehicle" } else { "vehicle_feedback_unavailable" }})
    }
}

pub struct Job {
    pub title: &'static str,
    pub effect: &'static str,
    pub body: Value,
}

pub fn job(action: &str) -> Option<Job> {
    let (title, effect, body) = match action {
        "git_pull" => ("GitPull", "현재 브랜치를 업데이트합니다. 재시작은 별도 요청입니다.", json!({"action": "git_pull"})),
        "git_sync" => ("Git Sync", "현재 브랜치를 동기화합니다. 로컬 변경을 잃을 수 있습니다.", json!({"action": "git_sync"})),
        "git_reset_hard" => ("Git Reset Hard", "HEAD 기준으로 로컬 파일 변경을 버립니다.", json!({"action": "git_reset", "mode": "hard", "target": "HEAD"})),
        "git_reset_mixed" => ("Git Reset Mixed", "HEAD 기준으로 스테이징을 해제하고 파일 변경은 남깁니다.", json!({"action": "git_reset", "mode": "mixed", "target": "HEAD"})),
        "git_reset_soft" => ("Git Reset Soft", "HEAD 기준 soft reset을 실행합니다.", json!({"action": "git_reset", "mode": "soft", "target": "HEAD"})),
        "reboot" => ("기기 재시작", "기기를 재시작하며 연결이 잠시 끊깁니다.", json!({"action": "reboot"})),
        "rebuild_all" => ("전체 재빌드", "재시작 후 전체 빌드를 진행합니다. 완료까지 기기를 사용할 수 없습니다.", json!({"action": "rebuild_all"})),
        "reset_calib" => ("캘리브레이션 초기화", "보정값을 지우고 자동 재시작합니다. 다시 보정해야 합니다.", json!({"action": "reset_calib"})),
        "delete_all_videos" => ("모든 화면 녹화 삭제", "기기의 화면 녹화 파일 전체를 삭제합니다. 되돌릴 수 없습니다.", json!({"action": "delete_all_videos"})),
        "delete_all_logs" => ("모든 주행 로그 삭제", "기기의 주행 로그 전체를 삭제합니다. 되돌릴 수 없습니다.", json!({"action": "delete_all_logs"})),
        "send_tmux_log" => ("진단 로그 저장", "tmux 진단 로그를 기기에 저장합니다. 서버 업로드와는 다릅니다.", json!({"action": "send_tmux_log"})),
        "server_tmux_log" => ("진단 로그 서버 전송", "tmux 진단 로그를 기존 당근 업로드 서버로 전송 요청합니다. 차량 식별정보와 진단 내용이 포함될 수 있습니다.", json!({"action": "server_tmux_log"})),
        "backup_settings" => ("설정 백업", "기기에 설정 백업 파일을 만듭니다. 폰 다운로드는 도구 화면에서 진행합니다.", json!({"action": "backup_settings"})),
        _ => return None,
    };
    Some(Job { title, effect, body })
}

const PANELS: [&str; 17] = ["tools", "car_select", "language", "web_settings", "device_info", "bluetooth", "assistant",
    "git_branch", "git_log", "git_reset_repo", "restore_settings", "qr_backup_settings", "qr_restore_settings",
    "copy_settings", "view_settings", "egpu", "naver_map"];
const REQUESTED: [&str; 4] = ["reboot", "rebuild_all", "reset_calib", "server_tmux_log"];

/// The Tools environment.
pub trait Jobs {
    fn running(&self) -> bool;
    fn start(&self, body: Value) -> Result<String, Fault>;
    fn snapshot(&self, id: &str) -> Option<Value>;
    fn record(&self, enabled: bool) -> Result<(), Fault>;
}

pub struct Tools {
    pub permissions: Permissions,
    proposals: HashMap<String, Value>,
    results: HashMap<(String, String), (f64, Value)>,
    owned: BTreeMap<String, (String, String)>,
}

impl Tools {
    pub fn new(path: &Path) -> Result<Self, Fault> {
        Ok(Self { permissions: Permissions::load(path)?, proposals: HashMap::new(), results: HashMap::new(), owned: BTreeMap::new() })
    }

    pub fn revoke(&mut self, token: &str, origin: &str) -> Result<(), Fault> {
        let owner = self.permissions.revoke(token, origin)?;
        self.proposals.retain(|_, v| v["owner"] != json!(owner));
        Ok(())
    }

    fn guard(&self, token: &str, origin: &str, action: &str, stationary: bool) -> Result<(), Fault> {
        self.permissions.authorize(token, origin)?;
        if job(action).is_some() && !stationary {
            return Err(invalid("정차하고 주행 보조를 해제한 뒤 도구 작업을 요청하세요."));
        }
        Ok(())
    }

    pub fn execute(&mut self, jobs: &dyn Jobs, stationary: bool, token: &str, origin: &str, command: &Value) -> Result<Value, Fault> {
        let owner = self.permissions.authorize(token, origin)?;
        let now = monotonic();
        self.proposals.retain(|_, v| v["until"].as_f64().unwrap_or(0.) > now);
        self.results.retain(|_, v| v.0 > now);
        let id = command.get("id").and_then(Value::as_str).filter(|s| identity(s)).ok_or_else(|| invalid("Invalid request identity"))?;
        let key = (owner.clone(), id.to_owned());
        if let Some((_, result)) = self.results.get(&key) {
            return Ok(result.clone());
        }
        if self.results.len() >= 1000 {
            return Err(invalid("Too many requests"));
        }
        let action = command.get("action").and_then(Value::as_str).unwrap_or_default().to_owned();
        let record = match action.as_str() {
            "record_start" => Some(true),
            "record_stop" => Some(false),
            _ => None,
        };
        let result = match command.get("operation").and_then(Value::as_str).unwrap_or_default() {
            "open" => {
                if !PANELS.contains(&action.as_str()) {
                    return Err(invalid("Unknown Tools panel"));
                }
                json!({"status": "open_requested", "action": action})
            }
            "prepare" => {
                if job(&action).is_none() && record.is_none() {
                    return Err(invalid("Unsupported Tools action; choose a listed action"));
                }
                self.guard(token, origin, &action, stationary)?;
                self.proposals.retain(|_, v| v["owner"] != json!(owner));
                let proposal_id = token_value();
                self.proposals.insert(proposal_id.clone(), json!({"owner": owner, "action": action, "until": now + 60.}));
                let (title, effect) = match job(&action) {
                    Some(j) => (j.title, j.effect),
                    None => (
                        if record == Some(true) { "화면 녹화 켜기" } else { "화면 녹화 끄기" },
                        "화면 녹화 설정을 변경합니다. 주행 로그 기록과는 별개입니다.",
                    ),
                };
                json!({"status": "prepared", "action": action, "title": title, "effect": effect,
                    "proposal_id": proposal_id, "confirmation_required": true, "expires_in_seconds": 60})
            }
            "commit" => {
                let proposal_id = command.get("proposal_id").and_then(Value::as_str).unwrap_or_default();
                let proposal = self.proposals.get(proposal_id).filter(|p| p["owner"] == json!(owner)).cloned()
                    .ok_or_else(|| invalid("제안이 만료됐습니다. 다시 내용을 확인한 뒤 요청하세요."))?;
                let action = proposal["action"].as_str().unwrap_or_default().to_owned();
                self.guard(token, origin, &action, stationary)?;
                self.proposals.remove(proposal_id);
                // Consume before side effects; an uncertain delivery is never replayed.
                self.results.insert(key.clone(), (now + 120., json!({"status": "unknown", "action": action})));
                match action.as_str() {
                    "record_start" | "record_stop" => {
                        jobs.record(action == "record_start")?;
                        json!({"status": "requested", "action": action})
                    }
                    _ => {
                        self.owned.retain(|k, _| jobs.snapshot(k).is_some());
                        if self.owned.len() >= 1000 {
                            return Err(invalid("Too many jobs"));
                        }
                        if jobs.running() {
                            return Err(invalid("다른 도구 작업이 진행 중입니다. 결과를 확인한 뒤 요청하세요."));
                        }
                        let body = job(&action).map(|j| j.body).unwrap_or(Value::Null);
                        let job_id = jobs.start(body)?;
                        self.owned.insert(job_id.clone(), (owner.clone(), action.clone()));
                        json!({"status": "running", "action": action, "job_id": job_id})
                    }
                }
            }
            "status" => {
                let job_id = command.get("job_id").and_then(Value::as_str).unwrap_or_default();
                let (job_owner, action) = self.owned.get(job_id).cloned()
                    .filter(|(o, _)| *o == owner)
                    .ok_or_else(|| invalid("이 연결에서 시작한 작업을 확인할 수 없습니다. 도구 작업 이력을 확인하고 자동 재실행하지 마세요."))?;
                let _ = job_owner;
                let state = match jobs.snapshot(job_id) {
                    Some(job) => match job.get("status").and_then(Value::as_str) {
                        Some("running") => "running",
                        Some("failed") => "failed",
                        _ if job.get("result").and_then(|r| r.get("ok")) != Some(&Value::Bool(true)) => "failed",
                        Some("done") => if REQUESTED.contains(&action.as_str()) { "requested" } else { "completed" },
                        _ => "unknown",
                    },
                    None => "unknown",
                };
                // Never forward shell output, file contents, device IDs or logs.
                json!({"status": state, "action": action, "job_id": job_id})
            }
            "cancel" => {
                self.proposals.retain(|_, v| v["owner"] != json!(owner));
                json!({"status": "cancelled"})
            }
            _ => return Err(invalid("Unknown Tools operation")),
        };
        self.results.insert(key, (now + 120., result.clone()));
        Ok(result)
    }
}

fn token_value() -> String {
    token()
}

/// `local_origin`: `http://<private IPv4>:7000`, not loopback.
pub fn local_origin(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("http://") else {
        return false;
    };
    let Some(host) = rest.strip_suffix(":7000") else {
        return false;
    };
    host.parse::<std::net::Ipv4Addr>().is_ok_and(|a| a.is_private() && !a.is_loopback() && a.to_string() == host)
}

pub fn proof_id(digest: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(digest.as_bytes()))
}

fn hex64(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `AssistantConnection.proof`.
pub fn proof(stores: &BTreeMap<&str, &Permissions>, origin: &str, previous: &Value, nonce: &Value, identifiers: &Value) -> Result<Value, Fault> {
    use hmac::{Hmac, Mac};
    let previous = previous.as_str().unwrap_or_default();
    let nonce = nonce.as_str().filter(|n| identity(n)).ok_or_else(|| invalid("Invalid device proof request"))?;
    if !local_origin(origin) || !local_origin(previous) {
        return Err(invalid("Invalid device proof request"));
    }
    let identifiers = identifiers.as_object().filter(|m| m.len() <= 4).ok_or_else(|| invalid("Invalid proof identifiers"))?;
    let mut replies = Map::new();
    for (role, id) in identifiers {
        let (Some(store), Some(id)) = (stores.get(role.as_str()), id.as_str().filter(|s| hex64(s))) else {
            continue;
        };
        for (digest, bound) in &store.grants {
            if (bound == previous || bound == origin) && proof_id(digest) == id {
                // Only the original phone and this device know SHA256(token).
                let message = format!("{nonce}\n{previous}\n{origin}");
                let Ok(mut mac) = Hmac::<sha2::Sha256>::new_from_slice(digest.as_bytes()) else { continue };
                mac.update(message.as_bytes());
                replies.insert(role.clone(), json!(hex::encode(mac.finalize().into_bytes())));
            }
        }
    }
    Ok(json!({"proofs": replies}))
}

/// `AssistantConnection.move`: validate every credential before changing any store.
pub fn move_grants(stores: &mut BTreeMap<&str, &mut Permissions>, origin: &str, previous: &Value, credentials: &Value) -> Result<(Vec<String>, Vec<(String, String)>), Fault> {
    let previous = previous.as_str().unwrap_or_default();
    let credentials = credentials.as_object().filter(|m| (1..=4).contains(&m.len()));
    let (true, Some(credentials)) = (local_origin(origin) && local_origin(previous), credentials) else {
        return Err(invalid("Invalid connection move"));
    };
    let mut matched = Vec::new();
    for (role, token) in credentials {
        let (Some(store), Some(token)) = (stores.get(role.as_str()), token.as_str().filter(|t| identity(t))) else {
            return Err(Fault::Permission("Invalid existing permission".into()));
        };
        let d = digest(token);
        let bound = store.grants.get(&d).cloned();
        if !bound.as_deref().is_some_and(|b| b == previous || b == origin) {
            return Err(Fault::Permission("Permission belongs to another device or was revoked".into()));
        }
        matched.push((role.clone(), d, bound.unwrap_or_default()));
    }
    let mut cancelled = Vec::new();
    for (role, d, bound) in &matched {
        cancelled.push((role.clone(), d.clone()));
        if bound != origin {
            if let Some(store) = stores.get_mut(role.as_str()) {
                let mut grants = store.grants.clone();
                grants.insert(d.clone(), origin.to_owned());
                store.save(grants)?;
            }
        }
    }
    Ok((matched.into_iter().map(|m| m.0).collect(), cancelled))
}
