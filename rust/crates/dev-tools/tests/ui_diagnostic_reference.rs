#[path = "ui_diagnostic_reference/source_binding.rs"]
mod source_binding;

use openpilot_dev_tools::{
    seal::Result,
    ui_diagnostic_reference::{render, Api},
};
use serde_json::{json, Value};
use std::{ffi::OsStr, path::Path};
#[derive(Default)]
struct Trace {
    events: Vec<Value>,
    fonts: Vec<String>,
}
fn text(s: &str) -> String {
    String::from_utf8(
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect(),
    )
    .unwrap()
}
fn color(s: &str) -> [u8; 4] {
    s.parse::<u32>().unwrap().to_le_bytes()
}
impl Api for Trace {
    fn call(&mut self, command: &str) -> Result<()> {
        let p = command.split(' ').collect::<Vec<_>>();
        let n = |at: usize| p[at].parse::<i64>().unwrap();
        let f = |at: usize| p[at].parse::<f64>().unwrap();
        let row = match p[0] {
            "font" => {
                self.fonts.push(command.into());
                return Ok(());
            }
            "flags" => json!(["flags", n(1)]),
            "window" => json!(["window", n(1), n(2), text(p[3])]),
            "target" => json!(["target", n(1), n(2)]),
            "shader" => json!(["shader", text(p[1]), text(p[2])]),
            "begin" => json!(["begin_drawing"]),
            "end" => json!(["end_drawing"]),
            "target-begin" => json!(["begin_texture_mode"]),
            "target-end" => json!(["end_texture_mode"]),
            "shader-begin" => json!(["begin_shader_mode"]),
            "shader-end" => json!(["end_shader_mode"]),
            "target-draw" => json!(["target-draw"]),
            "clear" => json!(["clear", color(p[1])]),
            "rect" => json!(["rect", f(1), f(2), f(3), f(4), color(p[5])]),
            "text" => json!(["text", text(p[1]), f(2), f(3), f(4), color(p[5])]),
            "line" => json!(["line", n(1), n(2), n(3), n(4), color(p[5])]),
            "circle" => json!(["circle", n(1), n(2), n(3), color(p[4])]),
            "export" => {
                self.events.push(json!(["rl_draw_render_batch_active"]));
                json!(["export"])
            }
            _ => panic!("unexpected raw command {command}"),
        };
        self.events.push(row);
        Ok(())
    }
}
#[test]
fn original_raylib_calls_shader_overlay_geometry_and_font_policy_are_independent() {
    let traces: Value =
        serde_json::from_str(include_str!("fixtures/ui-diagnostics/trace-original.json")).unwrap();
    for mode in ["burn-in", "grid", "touches", "unknown"] {
        let mut trace = Trace::default();
        render(
            Path::new("/fixed-root"),
            OsStr::new(mode),
            Path::new("/unused.png"),
            mode == "burn-in",
            false,
            &mut trace,
        )
        .unwrap();
        assert_eq!(
            trace.events,
            traces[mode].as_array().unwrap().clone(),
            "{mode}"
        );
        assert_eq!(trace.fonts.len(), 1);
        let font = &trace.fonts[0];
        let parts = font.split(' ').collect::<Vec<_>>();
        assert!(text(parts[1]).ends_with("/fonts/Inter-Medium.ttf"));
        assert_eq!(&parts[2..5], ["200", "2", "95"]);
        assert_eq!(
            parts[5..]
                .iter()
                .map(|p| p.parse::<u32>().unwrap())
                .collect::<Vec<_>>(),
            (32..127).collect::<Vec<_>>()
        );
    }
}
#[test]
fn raw_error_stops_scene_at_the_failing_call() {
    struct Stop {
        calls: usize,
        limit: usize,
    }
    impl Api for Stop {
        fn call(&mut self, _: &str) -> Result<()> {
            self.calls += 1;
            if self.calls == self.limit {
                Err("injected native failure".into())
            } else {
                Ok(())
            }
        }
    }
    for limit in 1..=13 {
        let mut stop = Stop { calls: 0, limit };
        assert!(render(
            Path::new("/root"),
            OsStr::new("burn-in"),
            Path::new("/out"),
            true,
            false,
            &mut stop
        )
        .is_err());
        assert_eq!(stop.calls, limit);
    }
}

#[test]
fn original_runtime_font_namespace_and_dependency_drift_fail_closed() {
    use openpilot_dev_tools::ui_diagnostic_reference::validate_source;
    use std::{fs, path::PathBuf};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let pins: Value =
        serde_json::from_str(include_str!("fixtures/ui-diagnostics/original.json")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    for name in pins["sources"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain(std::iter::once("uv.lock"))
    {
        let destination = temp.path().join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        if root.join(name).exists() {
            fs::copy(root.join(name), destination).unwrap();
        }
    }
    validate_source(temp.path()).unwrap();
    for name in pins["sources"].as_object().unwrap().keys() {
        let path = temp.path().join(name);
        let original = fs::read(&path).ok();
        fs::write(&path, b"changed original").unwrap();
        assert!(validate_source(temp.path()).is_err(), "changed {name}");
        fs::remove_file(&path).unwrap();
        let retired = matches!(
            name.as_str(),
            "rust/tools/check_ui_diagnostics.py" | "rust/tools/ui_qa/diagnostic_source.py"
        );
        assert_eq!(
            validate_source(temp.path()).is_ok(),
            retired,
            "absent {name}"
        );
        if let Some(original) = original {
            fs::write(&path, original).unwrap();
        }
    }
    for name in pins["absent"].as_array().unwrap() {
        let path = temp.path().join(name.as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"new namespace or font").unwrap();
        assert!(validate_source(temp.path()).is_err(), "added {name}");
        fs::remove_file(path).unwrap();
    }
    let lock = temp.path().join("uv.lock");
    let original = fs::read_to_string(&lock).unwrap();
    for package in ["numpy", "pillow", "comma-deps-raylib", "cffi", "pycparser"] {
        fs::write(
            &lock,
            original.replace(
                &format!("name = \"{package}\""),
                &format!("name = \"changed-{package}\""),
            ),
        )
        .unwrap();
        assert!(
            validate_source(temp.path()).is_err(),
            "dependency {package}"
        );
        fs::write(&lock, &original).unwrap();
    }
    fs::remove_file(&lock).unwrap();
    assert!(validate_source(temp.path()).is_err());
    fs::write(&lock, original).unwrap();
    for name in [
        "params_pyx.py",
        "params_pyx.cpython-312-x86_64-linux-gnu.so",
        "params_pyx",
    ] {
        let path = temp.path().join("openpilot/common").join(name);
        fs::write(&path, b"new importable Params implementation").unwrap();
        assert!(validate_source(temp.path()).is_err(), "{name}");
        fs::remove_file(path).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn owned_raw_child_is_reaped_on_malformed_reply_eof_and_command_failure() {
    use openpilot_dev_tools::ui_diagnostic_reference::Raw;
    use std::{fs, os::unix::fs::PermissionsExt};
    for (phase, reply) in [
        ("open", "invalid"),
        ("open", "ERR f"),
        ("open", "EOF"),
        ("command", "invalid"),
        ("command", "ERR ff"),
        ("command", "EOF"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let helper = temp.path().join("helper");
        let pid = temp.path().join("pid");
        let first = if phase == "command" {
            "printf 'OK\\n'; IFS= read -r command\n"
        } else {
            ""
        };
        let last = if reply == "EOF" {
            "exit 0\n".into()
        } else {
            format!("printf '%s\\n' '{reply}'\nwhile IFS= read -r command; do :; done\n")
        };
        fs::write(
            &helper,
            format!("#!/bin/sh\nprintf '%s' \"$$\" > \"$1\"\n{first}{last}"),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
        if phase == "open" {
            assert!(Raw::open(&helper, &pid).is_err(), "{phase}/{reply}");
        } else {
            let mut raw = Raw::open(&helper, &pid).unwrap();
            assert!(raw.call("flags 32").is_err(), "{phase}/{reply}");
            drop(raw);
        }
        let child = fs::read_to_string(pid).unwrap();
        assert!(
            !Path::new("/proc").join(child).exists(),
            "unreaped child {phase}/{reply}"
        );
    }
}

#[test]
fn inherited_invalid_numeric_environment_fails_before_rendering() {
    use std::{fs, process::Command};
    let temp = tempfile::tempdir().unwrap();
    // The original imports these integer globals even when the corresponding
    // overlay or recorder is disabled. BIG/SCALE/OFFSCREEN are overwritten.
    for variable in [
        "GRID",
        "PROFILE_RENDER",
        "PROFILE_STATS",
        "RECORD_QUALITY",
        "RECORD_SPEED",
    ] {
        let output = temp.path().join(format!("{variable}.png"));
        let reply = Command::new(env!("CARGO_BIN_EXE_ui-diagnostic-reference"))
            .args([OsStr::new("unknown"), output.as_os_str()])
            .env(variable, "invalid")
            .env("UI_DIAGNOSTIC_RAW_HELPER", "/missing-helper")
            .output()
            .unwrap();
        assert!(!reply.status.success());
        assert!(
            String::from_utf8_lossy(&reply.stderr).contains(variable),
            "{variable}: {:?}",
            reply.stderr
        );
        assert!(!fs::exists(output).unwrap());
    }
}

#[cfg(unix)]
#[test]
fn original_recording_launch_arguments_shader_failure_and_child_cleanup() {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};
    let pins: Value = serde_json::from_str(include_str!(
        "fixtures/ui-diagnostics/environment-original.json"
    ))
    .unwrap();
    for case in pins["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c.get("ffmpeg_argv").is_some())
    {
        let temp = tempfile::tempdir().unwrap();
        let raw = temp.path().join("raw");
        let marker = temp.path().join("raw-state");
        let ffmpeg = temp.path().join("ffmpeg");
        let output = temp.path().join("clip.test");
        fs::write(&raw, "#!/bin/sh\nprintf '%s' \"$$\" > \"$1.pid\"\nprintf 'OK\\n'\nwhile IFS= read -r command; do\n  printf '%s\\n' \"$command\" >> \"$1.calls\"\n  printf 'OK\\n'\n  [ \"$command\" = quit ] && exit 0\ndone\n").unwrap();
        fs::write(&ffmpeg, "#!/bin/sh\nfor last; do :; done\nprintf '%s' \"$$\" > \"$last.pid\"\nprintf '%s\\n' \"$@\" > \"$last.argv\"\n/bin/cat >/dev/null\n").unwrap();
        for helper in [&raw, &ffmpeg] {
            fs::set_permissions(helper, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_ui-diagnostic-reference"));
        command
            .args([
                OsStr::new(case["scenario"].as_str().unwrap()),
                temp.path().join("scene.png").as_os_str(),
            ])
            .env("UI_DIAGNOSTIC_RAW_HELPER", &raw)
            .env("STARTUP_UI_RAYLIB_LIBRARY", &marker)
            .env("PATH", temp.path())
            .env("RECORD", "1")
            .env("RECORD_OUTPUT", &output)
            .env("RECORD_QUALITY", "17")
            .env("RECORD_SPEED", "2")
            .env("RECORD_BITRATE", "123k")
            .env_remove("BURN_IN");
        let profile = case["environment"].get("PROFILE_STARTUP").is_some();
        if profile {
            command.env("PROFILE_STARTUP", "0");
        } else {
            command.env_remove("PROFILE_STARTUP");
        }
        let result = command.output().unwrap();
        assert_eq!(
            result.status.success(),
            case["exit"] == 0,
            "{:?}",
            result.stderr
        );
        let arguments = fs::read_to_string(output.with_extension("mp4.argv")).unwrap();
        let mut expected = case["ffmpeg_argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        *expected.last_mut().unwrap() = output.with_extension("mp4").to_str().unwrap().into();
        assert_eq!(arguments.lines().collect::<Vec<_>>(), expected);
        for pid in [
            marker.with_extension("pid"),
            output.with_extension("mp4.pid"),
        ] {
            let value = fs::read_to_string(pid).unwrap();
            assert!(!Path::new("/proc").join(value).exists());
        }
        let calls = fs::read_to_string(marker.with_extension("calls")).unwrap();
        assert!(calls.contains("target 536 240"));
        assert_eq!(calls.contains("export "), case["exit"] == 0 && !profile);
    }
}

#[cfg(unix)]
#[test]
fn startup_profile_preserves_early_exit_and_missing_recorder_preserves_failure() {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};
    let temp = tempfile::tempdir().unwrap();
    let raw = temp.path().join("raw");
    let marker = temp.path().join("calls");
    let image = temp.path().join("existing.png");
    fs::write(&image, b"existing requested image").unwrap();
    fs::write(
        &raw,
        r#"#!/bin/sh
printf 'OK\n'
while IFS= read -r command; do
  printf '%s\n' "$command" >> "$1"
  printf 'OK\n'
  [ "$command" = quit ] && exit 0
done
"#,
    )
    .unwrap();
    fs::set_permissions(&raw, fs::Permissions::from_mode(0o755)).unwrap();
    for profile in [true, false] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ui-diagnostic-reference"));
        command
            .args([OsStr::new("unknown"), image.as_os_str()])
            .env("UI_DIAGNOSTIC_RAW_HELPER", &raw)
            .env("STARTUP_UI_RAYLIB_LIBRARY", &marker)
            .env("PATH", temp.path());
        if profile {
            command.env("PROFILE_STARTUP", "0").env_remove("RECORD");
        } else {
            command.env("RECORD", "1").env_remove("PROFILE_STARTUP");
        }
        let result = command.output().unwrap();
        assert_eq!(result.status.success(), profile, "{:?}", result.stderr);
        let calls = fs::read_to_string(&marker).unwrap();
        assert!(!calls.contains("export "));
        assert_eq!(fs::read(&image).unwrap(), b"existing requested image");
        if profile {
            assert!(String::from_utf8_lossy(&result.stdout).contains("=== Startup profile ==="));
            assert!(calls.contains("font "));
            assert!(!calls.contains("begin\n"));
        } else {
            assert!(!calls.contains("font "));
        }
        fs::remove_file(&marker).unwrap();
    }
}
