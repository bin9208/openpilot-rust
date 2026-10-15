use std::{fs, process::Command};
#[test]
fn original_help_and_parse_error_exit_codes_precede_filesystem_work() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/panda-cli/five-interfaces.json")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let mut count = 0;
    for row in rows.as_array().unwrap().iter().filter(|row| {
        [
            "shorthelp",
            "cluster-equals",
            "explicithelp",
            "unknown-help",
            "missing-help",
        ]
        .contains(&row["case"].as_str().unwrap())
    }) {
        let binary = match row["family"].as_str().unwrap() {
            "bootlog" => env!("CARGO_BIN_EXE_build-bootlog-source"),
            "loggerd" => env!("CARGO_BIN_EXE_build-loggerd-source"),
            "runtime" => env!("CARGO_BIN_EXE_build-pandad-runtime-source"),
            "bridge" => env!("CARGO_BIN_EXE_build-bridge-source"),
            _ => env!("CARGO_BIN_EXE_check-jpeg-sanitizers"),
        };
        let args = row["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        let output = Command::new(binary)
            .args(args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code().unwrap() as i64,
            row["original_exit"].as_i64().unwrap(),
            "{row}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0, "{row}");
        count += 1;
    }
    assert_eq!(count, 25);
}
