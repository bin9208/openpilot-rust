use openpilot_dev_tools::{
    python_fs,
    qa_process::{CaptureError, Captured, CommandSpec, Runner},
    seal::Result,
    startup_adapter_sanitizers::{self, Kind, Options},
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    ffi::{OsStr, OsString},
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{symlink, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::Duration,
};
fn decode(v: &str) -> Vec<u8> {
    v.as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
fn hex(v: &[u8]) -> String {
    v.iter().map(|b| format!("{b:02x}")).collect()
}
fn replace(mut input: Vec<u8>, old: &[u8], new: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    while let Some(at) = input.windows(old.len()).position(|v| v == old) {
        out.extend_from_slice(&input[..at]);
        out.extend_from_slice(new);
        input = input[at + old.len()..].to_vec();
    }
    out.extend(input);
    out
}
fn normalized(input: &[u8], root: &Path) -> Vec<u8> {
    let escaped = python_fs::json_string(root.as_os_str()).unwrap();
    let input = replace(input.to_vec(), root.as_os_str().as_bytes(), b"$ROOT");
    replace(input, &escaped.as_bytes()[1..escaped.len() - 1], b"$ROOT")
}
fn physical(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.into()
    } else {
        root.join(path)
    }
}
struct Peer {
    case: String,
    root: PathBuf,
    output: PathBuf,
    calls: Vec<Value>,
}
impl Runner for Peer {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert!(spec.merge_output);
        assert!(spec.inherit_stdin);
        assert_eq!(spec.timeout, Duration::from_secs(180));
        let n = self.calls.len();
        let args = spec.effective_argv();
        let environment = spec.effective_env();
        let names = [
            "asan-runtime-build",
            "asan-build",
            "asan",
            "params-asan-build",
            "params-asan",
        ];
        let prefix = fs::read(self.output.join(format!("{}.log", names[n])))?;
        let env = [
            "DISPLAY",
            "OFFSCREEN",
            "STARTUP_UI_RAYLIB_LIBRARY",
            "ASAN_OPTIONS",
            "UBSAN_OPTIONS",
            "SENTINEL",
        ]
        .into_iter()
        .map(|key| {
            let fallback = match key {
                "OFFSCREEN" | "ASAN_OPTIONS" | "UBSAN_OPTIONS" => Some(OsStr::new("old")),
                "SENTINEL" => Some(OsStr::new("preserved")),
                _ => None,
            };
            let value = environment
                .get(OsStr::new(key))
                .map(OsString::as_os_str)
                .or(fallback);
            (
                key.into(),
                value
                    .map(|v| json!(hex(&normalized(v.as_bytes(), &self.root))))
                    .unwrap_or(Value::Null),
            )
        })
        .collect::<serde_json::Map<_, _>>();
        self.calls.push(json!({"argv_hex":args.iter().map(|v|hex(&normalized(v.as_bytes(),&self.root))).collect::<Vec<_>>(),"cwd_hex":hex(&normalized(spec.cwd.as_ref().unwrap().as_os_str().as_bytes(),&self.root)),"timeout":180,"check":true,"stderr_merged":true,"prefix":hex(&normalized(&prefix,&self.root)),"env_hex":env}));
        if self.case == "spawn0" {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
        }
        let bytes = b"out\xff\nerr\xfe\r".to_vec();
        if self.case == "timeout2" && n == 2 {
            return Err(CaptureError::Timeout {
                timeout: Duration::from_secs(180),
                stdout: bytes,
                stderr: vec![],
            }
            .into());
        }
        let failed = matches!(
            (self.case.as_str(), n),
            ("build0-fails", 0)
                | ("build1-fails", 1)
                | ("run2-fails", 2)
                | ("params-build-fails", 3)
                | ("params-run-fails", 4)
        );
        if !failed {
            if let Some(at) = args.iter().position(|v| v == "-o") {
                fs::write(
                    physical(&self.root, Path::new(&args[at + 1])),
                    b"owned-binary\n",
                )?;
            }
            if n == 2 && self.case != "missing-capture" {
                fs::write(
                    physical(&self.root, Path::new(args.last().unwrap())),
                    b"controlled-PNG\0",
                )?;
            }
        }
        Ok(Captured {
            code: if failed { 7 } else { 0 },
            stdout: bytes,
            stderr: vec![],
        })
    }
}
#[test]
fn seventy_four_independent_original_cases_preserve_invocations_disks_and_partial_files() {
    assert_ne!(
        rustix::process::geteuid().as_raw(),
        0,
        "permission oracle requires a non-root user (original captures uid1000)"
    );
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/startup-adapter-sanitizers/original.json"
    ))
    .unwrap();
    for frozen in original["cases"].as_array().unwrap() {
        let mut row = frozen.clone();
        let temp = tempfile::tempdir().unwrap();
        let case = frozen["case"].as_str().unwrap();
        let mut root = temp.path().to_path_buf();
        if case == "raw-root" {
            root = root.join(OsString::from_vec(b"work-\xff".to_vec()));
            fs::create_dir(&root).unwrap();
        }
        let axis = |name: &str| {
            root.join(OsString::from_vec(
                [
                    name.as_bytes(),
                    if case == format!("raw-{name}") {
                        b"-\xff"
                    } else {
                        b""
                    },
                ]
                .concat(),
            ))
        };
        let target = axis("target");
        let output = axis("output");
        let raylib = axis("raylib");
        fs::create_dir_all(target.join("build")).unwrap();
        fs::create_dir(target.join("deps")).unwrap();
        let header=target.join("build/openpilot-startup-ui-one/out/cxxbridge/include/openpilot-startup-ui/src/bridge.rs.h");
        fs::create_dir_all(header.parent().unwrap()).unwrap();
        if case != "missing-header" {
            if case == "dangling-header" {
                symlink(root.join("missing"), &header).unwrap();
            } else {
                fs::write(&header, b"header").unwrap();
            }
        }
        if case == "non-directory-prefix" {
            fs::write(target.join("build/openpilot-startup-ui-file"), b"file").unwrap();
        }
        let cxx = target.join("deps/libcxx-one.rlib");
        if case != "missing-cxx" {
            if case == "directory-cxx" {
                fs::create_dir(&cxx).unwrap();
            } else {
                fs::write(&cxx, b"rlib").unwrap();
            }
        }
        if case == "equal-mtimes" {
            let second = target.join("deps/libcxx-two.rlib");
            fs::write(&second, b"two").unwrap();
            let time = std::time::UNIX_EPOCH + Duration::from_secs(100);
            for p in [&cxx, &second] {
                fs::File::open(p).unwrap().set_modified(time).unwrap();
            }
        }
        if case == "equal-mtimes" {
            let candidates = fs::read_dir(target.join("deps"))
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect::<Vec<_>>();
            let captured = row["cxx_candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                candidates
                    .iter()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>(),
                captured
            );
            let old = decode(row["calls"][0]["argv_hex"][8].as_str().unwrap());
            assert!(old.starts_with(b"cxx=$ROOT/target/deps/"));
            let oldname = old.rsplit(|b| *b == b'/').next().unwrap();
            let chosen = candidates.first().unwrap().as_bytes();
            row["calls"][0]["argv_hex"][8] = json!(hex(&replace(old.clone(), oldname, chosen)));
            let prefix = decode(row["calls"][0]["prefix"].as_str().unwrap());
            let prefix_json: Value =
                serde_json::from_slice(&prefix[b"Invocation: ".len()..]).unwrap();
            assert_eq!(prefix_json[8].as_str().unwrap().as_bytes(), old);
            row["calls"][0]["prefix"] = json!(hex(&replace(prefix, oldname, chosen)));
            let log = decode(row["files"]["asan-runtime-build.log"].as_str().unwrap());
            row["files"]["asan-runtime-build.log"] = json!(hex(&replace(log, oldname, chosen)));
        }
        let params=target.join("build/openpilot-ui-application-one/out/cxxbridge/include/openpilot-ui-application/src/params/numeric/bridge.rs.h");
        if case != "missing-params-header" {
            fs::create_dir_all(params.parent().unwrap()).unwrap();
            fs::write(&params, b"params").unwrap();
        }
        let denied = row["permission_dir"].as_str().map(|p| root.join(p));
        if let Some(denied) = &denied {
            let selected = if case.starts_with("params-") {
                &params
            } else {
                &header
            };
            let alternate =
                PathBuf::from(selected.to_str().unwrap().replace("-one/", "-alternate/"));
            fs::create_dir_all(alternate.parent().unwrap()).unwrap();
            fs::write(&alternate, b"alternate").unwrap();
            for (path, seconds) in [(selected, 300), (&alternate, 100)] {
                fs::File::open(path)
                    .unwrap()
                    .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(seconds))
                    .unwrap();
            }
            fs::set_permissions(
                denied,
                fs::Permissions::from_mode(row["permission_mode"].as_u64().unwrap() as u32),
            )
            .unwrap();
        }
        let relative = |path: PathBuf| {
            if case == "relative-args" {
                path.strip_prefix(&root).unwrap().into()
            } else {
                path
            }
        };
        let mut options = Options {
            root: root.clone(),
            cwd: root.clone(),
            target: relative(target),
            output: relative(output.clone()),
            raylib: relative(raylib.clone()),
            raylib_library: Some(relative(raylib.join("lib/alias.so"))),
            display: ":owned".into(),
        };
        if case.starts_with("resolve-") {
            let loop_path = root.join("loop");
            symlink(&loop_path, &loop_path).unwrap();
            match case {
                "resolve-output-loop" => options.output = loop_path,
                "resolve-library-loop" => options.raylib_library = Some(loop_path),
                "resolve-target-loop" => options.target = loop_path,
                "resolve-raylib-loop" => options.raylib = loop_path,
                _ => unreachable!(),
            }
        }
        let mut peer = Peer {
            case: case.into(),
            root: root.clone(),
            output: output.clone(),
            calls: vec![],
        };
        let disks = RefCell::new(vec![]);
        let mut free = |path: &Path| {
            let n = disks.borrow().len();
            let value = match case {
                "disk-initial-equal" => 25 * 1024u64.pow(3) + 256 * 1024 * 1024,
                "disk-initial-low" => 25 * 1024u64.pow(3) + 256 * 1024 * 1024 - 1,
                "disk-first-low" => 26 * 1024u64.pow(3) - 1,
                "disk-second-low" if n == 1 => 26 * 1024u64.pow(3) - 1,
                "disk-equal" => 26 * 1024u64.pow(3),
                _ => 40 * 1024u64.pow(3),
            };
            disks.borrow_mut().push(json!({"path_hex":hex(&normalized(path.as_os_str().as_bytes(),&root)),"free":value}));
            Ok(value)
        };
        let mut printed = vec![];
        let kind = if row["family"] == "safety" {
            Kind::Safety
        } else {
            Kind::Resources
        };
        let result =
            startup_adapter_sanitizers::check(&options, kind, &mut peer, &mut free, &mut printed);
        if let Some(denied) = &denied {
            fs::set_permissions(denied, fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "{} {case}: {result:?}",
            row["family"]
        );
        assert_eq!(json!(peer.calls), row["calls"], "{} {case}", row["family"]);
        assert_eq!(json!(disks.into_inner()), row["disks"], "{case}");
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            row["printed"].as_str().unwrap(),
            "{case}"
        );
        assert_eq!(
            output.exists(),
            row["output_exists"].as_bool().unwrap(),
            "{case}"
        );
        let mut actual = serde_json::Map::new();
        if output.is_dir() {
            for entry in fs::read_dir(&output).unwrap() {
                let path = entry.unwrap().path();
                actual.insert(
                    path.file_name().unwrap().to_str().unwrap().into(),
                    json!(hex(&normalized(&fs::read(path).unwrap(), &root))),
                );
            }
        }
        assert_eq!(
            Value::Object(actual),
            row["files"],
            "{} {case}",
            row["family"]
        );
    }
}
#[test]
fn additional_safety_policy_guards_every_compiler_before_its_log_or_child() {
    for failing in [Some(0), Some(1), Some(2), None] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let target = root.join("target");
        let output = root.join("output");
        for relative in [
            "build/openpilot-startup-ui-one/out/cxxbridge/include/openpilot-startup-ui/src/bridge.rs.h",
            "build/openpilot-ui-application-one/out/cxxbridge/include/openpilot-ui-application/src/params/numeric/bridge.rs.h",
            "deps/libcxx-one.rlib",
        ] {
            let path = target.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"fixture").unwrap();
        }
        let options = Options {
            root: root.into(),
            cwd: root.into(),
            target,
            output: output.clone(),
            raylib: root.join("raylib"),
            raylib_library: Some(root.join("raylib/lib/alias.so")),
            display: ":owned".into(),
        };
        let mut peer = Peer {
            case: "success".into(),
            root: root.into(),
            output: output.clone(),
            calls: vec![],
        };
        let mut checks = 0;
        let mut build_free = |path: &Path| {
            assert_eq!(path, output);
            let current = checks;
            checks += 1;
            Ok(26 * 1024u64.pow(3) - u64::from(failing == Some(current)))
        };
        let mut printed = vec![];
        let result = startup_adapter_sanitizers::check_guarded(
            &options,
            Kind::Safety,
            &mut peer,
            &mut |_| Ok(40 * 1024u64.pow(3)),
            &mut build_free,
            &mut printed,
        );
        assert_eq!(result.is_ok(), failing.is_none(), "{failing:?}: {result:?}");
        assert_eq!(
            checks,
            match failing {
                Some(0) => 1,
                Some(1) => 2,
                _ => 3,
            }
        );
        assert_eq!(
            peer.calls.len(),
            match failing {
                Some(0) => 0,
                Some(1) => 1,
                Some(2) => 3,
                _ => 5,
            }
        );
        if let Some(index) = failing {
            let name = ["asan-runtime-build", "asan-build", "params-asan-build"][index];
            assert!(!output.join(format!("{name}.log")).exists());
            assert!(!output.join("result.json").exists());
            assert!(printed.is_empty());
        }
    }
}
