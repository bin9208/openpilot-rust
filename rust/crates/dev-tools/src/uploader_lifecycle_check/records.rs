//! Fault and typed Params controllers using immutable source-only observations.
use super::{corpus, file, marked, require, Namespace, Result};
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    uploader_logging_check::{
        ipc::{ownership::Permission, Peer},
        normalized,
    },
    uploader_reference_check::http,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    time::Duration,
};
pub const FAULT_IDS: [&str; 15] = [
    "normal",
    "eagain",
    "persistent-debug",
    "backoff",
    "start",
    "debug",
    "success",
    "ignored",
    "failed",
    "large",
    "metadata",
    "listdir",
    "getsize",
    "clear",
    "mark",
];
pub const PARAM_IDS: [&str; 7] = [
    "missing-id",
    "empty-id",
    "invalid-id",
    "valid-id",
    "empty-routes",
    "valid-routes",
    "invalid-routes",
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub collector: PathBuf,
    pub worker: PathBuf,
    pub work: PathBuf,
    pub work_identity: (u64, u64),
    pub output: PathBuf,
    pub prefix: String,
    pub fault: bool,
    pub expected: Value,
}
fn fault_settings(name: &str) -> Result<(&'static str, Value)> {
    Ok(match name {
        "normal" => ("", json!({})),
        "eagain" => ("eagain", json!({})),
        "persistent-debug" => ("debug_url", json!({"persistent":true})),
        "backoff" => ("upload backoff 0.1", json!({"put_status":500})),
        "start" => ("upload_start", json!({})),
        "debug" => ("debug_url", json!({})),
        "success" => ("upload_success", json!({})),
        "ignored" => ("upload_ignored", json!({"api_status":412})),
        "failed" => ("upload_failed", json!({"put_status":500})),
        "large" => ("uploader_too_large", json!({"size":25000001})),
        "metadata" => ("uploader_getxattr_failed", json!({"broken":true})),
        "listdir" => (
            "listdir_by_creation failed",
            json!({"no_listing":true,"action":"direct_step"}),
        ),
        "getsize" => (
            "upload: getsize failed",
            json!({"missing":true,"action":"upload"}),
        ),
        "clear" => (
            "clear_locks failed",
            json!({"clear_error":true,"action":"clear"}),
        ),
        "mark" => ("uploader_setxattr_failed", json!({"fail_mark":true})),
        _ => return Err("unknown original fault case".into()),
    })
}
fn source_directory(records: &[Value]) -> Result<PathBuf> {
    for record in records {
        if let Some(path) = record["msg"]["fn"].as_str() {
            let path = Path::new(path);
            require(
                path.ends_with("route--0/qlog"),
                "original log fixture path differs",
            )?;
            return Ok(path
                .parent()
                .and_then(Path::parent)
                .ok_or("original log root absent")?
                .into());
        }
    }
    Ok(PathBuf::from("/"))
}
fn http_projection(rows: &Value) -> Result<Value> {
    Ok(json!(rows
        .as_array()
        .ok_or("HTTP rows absent")?
        .iter()
        .map(|v| json!({"method":v["method"],"path":v["path"],"body_hex":v["body_hex"]}))
        .collect::<Vec<_>>()))
}
fn configure(request: &Request, settings: &Value) -> Result<()> {
    fs::create_dir_all(&request.work)?;
    let path = request.work.join("route--0/qlog");
    if settings["missing"] != true {
        let size = settings["size"].as_u64().unwrap_or(128);
        if size > 100000 {
            fs::create_dir_all(path.parent().ok_or("fixture parent absent")?)?;
            fs::File::create(&path)?.set_len(size)?;
        } else {
            file(&path, usize::try_from(size)?)?;
        }
    }
    if settings["broken"] == true {
        fs::remove_file(&path)?;
        symlink("missing", &path)?;
    }
    if settings["clear_error"] == true {
        fs::write(request.work.join("plain"), b"fixture")?;
    }
    Ok(())
}
fn native_records(records: &[Value], root: &Path) -> Result<()> {
    for row in records {
        require(
            row["process"].as_u64().is_some_and(|p| p > 0)
                && row["lineno"].as_u64().is_some_and(|p| p > 0)
                && row["funcName"].as_str().is_some_and(|v| !v.is_empty()),
            "native log callsite missing",
        )?;
        let pathname = Path::new(
            row["pathname"]
                .as_str()
                .ok_or("native log pathname missing")?,
        );
        require(
            !pathname.is_absolute()
                && pathname
                    .components()
                    .all(|p| matches!(p, std::path::Component::Normal(_))),
            "native callsite escaped retained source",
        )?;
        let content =
            String::from_utf8(corpus::bytes(&root.join("rust").join(pathname), 4 << 20)?)?;
        let line = usize::try_from(
            row["lineno"]
                .as_u64()
                .ok_or("native line absent")?
                .checked_sub(1)
                .ok_or("native line zero")?,
        )?;
        require(
            content
                .lines()
                .nth(line)
                .is_some_and(|s| s.contains("log_site!")),
            "native log_site line absent from retained Rust source",
        )?;
        if let Some(exc) = row.get("exc_info") {
            require(
                exc.as_str().is_some_and(|v| {
                    v.starts_with("Rust error:")
                        && v.contains("Rust backtrace captured at the reporting site:")
                }),
                "native log exception lost Rust traceback",
            )?;
        }
    }
    Ok(())
}
pub fn worker(path: &Path) -> Result<Value> {
    let request: Request = serde_json::from_slice(&corpus::bytes(path, 8 << 20)?)?;
    require(
        request.prefix.starts_with("rust-probe-uploader-life-")
            && request.prefix.len() < 90
            && request
                .prefix
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-'),
        "unsafe sibling IPC basename",
    )?;
    let name = request.expected[if request.fault { "name" } else { "case" }]
        .as_str()
        .ok_or("source case absent")?;
    let (target, settings) = if request.fault {
        fault_settings(name)?
    } else {
        ("", json!({}))
    };
    let permission = Permission::admitted(&request.work, request.work_identity)?;
    configure(&request, &settings)?;
    if settings["no_listing"] == true {
        permission.mode(0o111)?;
    }
    let _permission = permission;
    let server = http::Server::start(&settings)?;
    let home = request
        .work
        .parent()
        .ok_or("private work parent absent")?
        .join("home");
    let persist = home.join(format!(".comma{}/persist", request.prefix));
    http::write_ecdsa(&persist)?;
    let params = request
        .work
        .parent()
        .ok_or("private work parent absent")?
        .join("params")
        .join(&request.prefix);
    fs::create_dir_all(&params)?;
    let identity: Option<&[u8]> = match name {
        "missing-id" => None,
        "empty-id" => Some(b""),
        "invalid-id" => Some(b"one'\xff"),
        _ => Some(b"0000000000000000"),
    };
    if let Some(value) = identity {
        fs::write(params.join("DongleId"), value)?;
    }
    fs::write(params.join("IsOffroad"), b"0")?;
    let routes: Option<&[u8]> = match name {
        "empty-routes" => Some(b""),
        "valid-routes" => Some(b"dongle|route--0"),
        "invalid-routes" => Some(b"\xff'\"\n"),
        _ => None,
    };
    if let Some(value) = routes {
        fs::write(params.join("AthenadRecentlyViewedRoutes"), value)?;
    }
    let source_collector = request.expected["original_collector"]
        .as_bool()
        .ok_or("collector lane absent")?;
    let mut peer = Peer::start(
        &request.collector,
        &request.worker,
        &request.output,
        source_collector,
        &request.prefix,
    )?;
    peer.synchronize()?;
    let mut command = CommandSpec::new_os(
        if request.fault {
            vec![request.binary.as_os_str().to_owned()]
        } else {
            vec![
                request.binary.as_os_str().to_owned(),
                "--cycles".into(),
                "1".into(),
            ]
        },
        Duration::from_secs(30),
    );
    command.cwd = Some(request.root.clone());
    command.env.insert(
        "ROUTE_UPLOAD_QA_IPC_ROOT".into(),
        peer.ipc_root.to_string_lossy().into_owned(),
    );
    for (key, value) in [
        ("HOME", home.as_os_str()),
        (
            "PARAMS_ROOT",
            params.parent().ok_or("Params parent absent")?.as_os_str(),
        ),
        ("LOG_ROOT", request.work.as_os_str()),
        ("OPENPILOT_PREFIX", std::ffi::OsStr::new(&request.prefix)),
        ("API_HOST", std::ffi::OsStr::new(&server.url)),
        ("UPLOADER_SLEEP", std::ffi::OsStr::new("0")),
        ("FORCEWIFI", std::ffi::OsStr::new("")),
    ] {
        command
            .env
            .insert(key.into(), value.to_string_lossy().into_owned());
    }
    command.env_remove = [
        "FAKEUPLOAD",
        "LOGPRINT",
        "OPENPILOT_LOG_CONTEXT",
        "MANAGER_DAEMON",
        "LD_PRELOAD",
        "ZMQ",
        "CEREAL_FAKE",
    ]
    .map(Into::into)
    .to_vec();
    if request.fault {
        command.input = Some(serde_json::to_vec(
            &json!({"root":request.work,"endpoint":peer.endpoint,"target":target,"api":server.url,"persist":persist,"version":request.root.join("openpilot/common/version.h"),"action":settings["action"].as_str().unwrap_or("step"),"fail_mark":settings["fail_mark"]==true,"persistent":settings["persistent"]==true}),
        )?);
    }
    let output = NativeRunner.run_in_current_group(&command)?;
    fs::write(request.output.join("uploader.stdout"), &output.stdout)?;
    fs::write(request.output.join("uploader.stderr"), &output.stderr)?;
    _permission.mode(0o755)?;
    let (records, errors, disks, collector_code) = peer.finish()?;
    let url = server.url.clone();
    let received = server.finish()?;
    let public = fs::read_to_string(persist.join("comma/id_ecdsa.pub"))?;
    for row in &received {
        if serde_json::to_value(row)?["method"] == "GET" {
            http::verify_jwt(row, &public, "id_ecdsa", true)?;
        }
    }
    fs::write(request.output.join("signer-public.pem"), public)?;
    let http = serde_json::to_value(received)?;
    fs::write(
        request.output.join("http.json"),
        serde_json::to_vec_pretty(&http)?,
    )?;
    fs::write(
        request.output.join("records.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    require(
        collector_code == if source_collector { -2 } else { 0 },
        "sibling collector shutdown changed",
    )?;
    require(
        errors
            == records
                .iter()
                .filter(|r| r["levelnum"].as_u64().unwrap_or(0) >= 40)
                .cloned()
                .collect::<Vec<_>>(),
        "sibling error packet routing changed",
    )?;
    let original = request.expected["records"]
        .as_array()
        .ok_or("original source records missing")?;
    let expected = normalized(original, &source_directory(original)?, None)?;
    let actual = normalized(&records, &request.work, Some(&url))?;
    fs::write(
        request.output.join("comparison.json"),
        serde_json::to_vec_pretty(&json!({"source":expected,"native":actual}))?,
    )?;
    require(
        actual == expected,
        "sibling log records differ from immutable original observations",
    )?;
    require(
        disks as u64
            == request.expected["disk_records"]
                .as_u64()
                .ok_or("source disk count absent")?,
        "sibling disk projection count changed",
    )?;
    native_records(&records, &request.root)?;
    let path = request.work.join("route--0/qlog");
    if request.fault {
        let mut observed: Value = serde_json::from_slice(&output.stdout)?;
        require(
            output.code == i32::from(observed["status"] == "Error")
                && json!(output.code) == request.expected["exit_code"],
            "fault exit status changed",
        )?;
        if !observed["error"].is_null() {
            require(
                observed["error"].as_str().is_some_and(|v| {
                    v.contains("Logging(Transport(Socket operation on non-socket))")
                }),
                "fault exception category changed",
            )?;
        }
        if let Some(last) = observed["last"].as_str().filter(|s| !s.is_empty()) {
            observed["last"] = json!(Path::new(last).strip_prefix(&request.work)?);
        }
        for key in ["status", "delay", "last", "attempts", "fired", "saturated"] {
            require(
                observed[key] == request.expected["observation"][key],
                &format!("fault observation changed: {key}"),
            )?;
        }
        require(
            json!(marked(&path)) == request.expected["marked"],
            "fault xattr boundary changed",
        )?;
        require(
            http_projection(&http)? == http_projection(&request.expected["http"])?,
            "fault full HTTP method/path/body changed",
        )?;
        if name == "debug" {
            let failure = records
                .iter()
                .find(|v| v["msg"]["event"] == "upload_failed")
                .ok_or("debug failure event absent")?;
            require(
                failure["msg"]["stat"].is_null()
                    && failure["msg"]["exc"][0].as_str().is_some_and(|v| {
                        v.starts_with("Logging(Transport(")
                            && v.contains("Socket operation on non-socket")
                    }),
                "debug fault meaning changed",
            )?;
        }
    } else {
        require(
            json!(output.code) == request.expected["exit"],
            "typed Params uploader exit changed",
        )?;
        let warnings = records
            .iter()
            .filter(|r| r["levelnum"] == 30)
            .collect::<Vec<_>>();
        require(
            json!(warnings.len()) == request.expected["warnings"] && errors.is_empty(),
            "typed Params warning/error routing changed",
        )?;
        if let Some(warning) = warnings.first() {
            require(
                warning["funcName"] == "openpilot_params_typed::get_string"
                    && warning["filename"] == "lib.rs"
                    && actual[0]["level"] == "WARNING",
                "typed Params warning origin/order changed",
            )?;
            if output.code != 0 {
                require(
                    actual[1]["msg"] == "uploader missing dongle_id",
                    "typed Params missing-id ordering changed",
                )?;
            }
        }
        require(
            json!(http.as_array().ok_or("HTTP rows missing")?.len())
                == request.expected["http_requests"],
            "typed Params HTTP request count changed",
        )?;
        if output.code == 0 {
            require(marked(&path), "typed Params upload not marked")?;
        }
    }
    Ok(
        json!({"passed":true,"case":name,"scope":if request.fault{"fault"}else{"params"},"source_collector":source_collector,"disk_records":disks,"log_packets":records.len(),"error_packets":errors.len(),"http_requests":http.as_array().ok_or("HTTP rows missing")?.len(),"exit_code":output.code,"full_original_comparison":true}),
    )
}
pub fn run(
    root: &Path,
    binary: &Path,
    collector: &Path,
    worker: &Path,
    output: &Path,
    rows: &[Value],
    fault: bool,
) -> Result<Value> {
    let mut results = Vec::new();
    for expected in rows {
        let name = expected[if fault { "name" } else { "case" }]
            .as_str()
            .ok_or("source case absent")?;
        let lane = if expected["original_collector"] == true {
            "source-policy-native-collector"
        } else {
            "rust-collector"
        };
        let output = output.join(name).join(lane);
        fs::create_dir_all(&output)?;
        let temp = tempfile::tempdir()?;
        let token = temp
            .path()
            .file_name()
            .ok_or("private token absent")?
            .to_string_lossy()
            .replace('.', "-");
        let prefix = format!("rust-probe-uploader-life-{}-{token}", std::process::id());
        let namespace_owner = Namespace::new(&prefix)?;
        let work = temp.path().join("logs");
        fs::create_dir(&work)?;
        let permission = Permission::created(&work)?;
        let work_identity = permission.identity()?;
        let _permission = permission;
        let request = Request {
            root: root.into(),
            binary: binary.into(),
            collector: collector.into(),
            worker: worker.into(),
            work,
            work_identity,
            output: output.clone(),
            prefix: prefix.clone(),
            fault,
            expected: expected.clone(),
        };
        let input = output.join("request.json");
        fs::write(&input, serde_json::to_vec(&request)?)?;
        let mut command = CommandSpec::new_os(
            vec![
                worker.as_os_str().to_owned(),
                "--uploader-records-worker".into(),
                input.into_os_string(),
            ],
            Duration::from_secs(75),
        );
        command.cwd = Some(root.into());
        for (key, value) in namespace_owner.environment()? {
            command.env.insert(key, value);
        }
        command.env.insert("OPENPILOT_PREFIX".into(), prefix);
        command.env_remove = ["ZMQ", "CEREAL_FAKE", "LD_PRELOAD"]
            .map(Into::into)
            .to_vec();
        let captured = NativeRunner.run(&command);
        let captured = captured?;
        fs::write(output.join("worker.stdout"), &captured.stdout)?;
        fs::write(output.join("worker.stderr"), &captured.stderr)?;
        require(
            captured.code == 0,
            "sibling record worker failed; inspect worker.stderr",
        )?;
        results.push(serde_json::from_slice::<Value>(&captured.stdout)?);
    }
    Ok(
        json!({"passed":true,"native_executions":results.len(),"scenarios":results,"expected_origin":"immutable captured Python runtime observations; native daemon is never an expectation source"}),
    )
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    #[test]
    fn permission_restore_targets_held_original_directory_not_foreign_replacement() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        for link in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("work");
            fs::create_dir(&path).unwrap();
            let permission = Permission::created(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o111)).unwrap();
            let original = root.path().join("original");
            fs::rename(&path, &original).unwrap();
            let foreign = root.path().join("foreign");
            fs::create_dir(&foreign).unwrap();
            fs::set_permissions(&foreign, fs::Permissions::from_mode(0o700)).unwrap();
            if link {
                symlink(&foreign, &path).unwrap();
            } else {
                fs::create_dir(&path).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            }
            assert!(Permission::admitted(&path, permission.identity().unwrap()).is_err());
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o700
            );
            drop(permission);
            assert_eq!(
                fs::metadata(&original).unwrap().permissions().mode() & 0o777,
                0o755
            );
            assert_eq!(
                fs::metadata(&foreign).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }
}
