use openpilot_dev_tools::path_resolve;
use serde_json::json;
use std::fs;
#[test]
fn output_paths_match_original_resolve_with_missing_components_and_symlinks() {
    use std::os::unix::fs::symlink;
    let original: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/logger-identifier/original.json")).unwrap();
    let captured = &original["output_resolution"];
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for name in captured["directories"].as_array().unwrap() {
        fs::create_dir_all(root.join(name.as_str().unwrap())).unwrap();
    }
    for name in captured["files"].as_array().unwrap() {
        fs::write(root.join(name.as_str().unwrap()), b"file").unwrap();
    }
    for (name, target) in captured["links"].as_object().unwrap() {
        symlink(target.as_str().unwrap(), root.join(name)).unwrap();
    }
    for case in captured["cases"].as_array().unwrap() {
        let result = path_resolve::resolve_non_strict(&root.join(case["input"].as_str().unwrap()));
        if case.get("error").is_some() {
            assert!(result.is_err(), "{case}");
        } else {
            assert_eq!(
                json!(result
                    .unwrap()
                    .to_string_lossy()
                    .replace(root.to_str().unwrap(), "$ROOT")),
                case["resolved"],
                "{case}"
            );
        }
    }
}
