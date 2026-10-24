use openpilot_dev_tools::ci_policy::parse;
use std::{fs, path::Path};

#[test]
fn egl_reference_is_explicitly_built_and_tested_in_the_native_ui_job() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow =
        parse(&fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap()).unwrap();
    let mut matches = 0;
    for job in workflow["jobs"].as_object().unwrap().values() {
        let Some(steps) = job["steps"].as_array() else {
            continue;
        };
        for step in steps {
            if step["name"] != "Build native UI validation checkers" {
                continue;
            }
            matches += 1;
            let run = step["run"].as_str().unwrap();
            let build = run
                .lines()
                .find(|line| line.contains("cargo build "))
                .unwrap();
            let args = build.split_whitespace().collect::<Vec<_>>();
            assert!(args.contains(&"--locked"));
            assert!(args.windows(2).any(|pair| pair
                == [
                    "--features",
                    "egl-reference,polygon-reference,ui-application-lifecycle-native"
                ]));
            for binary in [
                "check-ui-egl",
                "ui-egl-reference",
                "check-ui-polygon",
                "ui-polygon-reference",
                "check-ui-diagnostics",
                "ui-diagnostic-reference",
                "build-ui-diagnostic-reference",
                "check-ui-emoji",
                "check-ui-forms",
                "ui-forms-reference",
            ] {
                assert!(
                    args.windows(2).any(|pair| pair == ["--bin", binary]),
                    "missing {binary}"
                );
            }
            assert!(run.contains(
                "--features egl-reference --test ui_egl_reference --test ui_egl_check --locked"
            ));
            assert!(steps.iter().any(|step| step["run"]
                .as_str()
                .is_some_and(|run| run.contains("STARTUP_UI_RAYLIB_ROOT="))));
        }
    }
    assert_eq!(matches, 1);
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("rust/crates/dev-tools/Cargo.toml")).unwrap())
            .unwrap();
    assert!(manifest["features"].get("default").is_none());
    assert_eq!(
        manifest["dependencies"]["openpilot-startup-ui"]["optional"].as_bool(),
        Some(true)
    );
    for kind in ["bin", "test"] {
        for entry in manifest[kind].as_array().unwrap() {
            if matches!(
                entry["name"].as_str(),
                Some("check-ui-egl" | "ui-egl-reference" | "ui_egl_reference" | "ui_egl_check")
            ) {
                assert_eq!(
                    entry["required-features"].as_array().unwrap(),
                    &[toml::Value::String("egl-reference".into())]
                );
            }
        }
    }
}
