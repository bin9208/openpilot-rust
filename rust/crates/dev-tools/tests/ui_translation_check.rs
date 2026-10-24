use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_translation_check::{check, parse_po, selector},
};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs,
    io::Read,
    os::unix::{ffi::OsStringExt, fs::symlink},
    path::Path,
};
fn fixture(name: &str) -> Value {
    let mut bytes = vec![];
    flate2::read::GzDecoder::new(match name {
        "parser" => include_bytes!("fixtures/ui-translations/original-parser.json.gz").as_slice(),
        "cli" => include_bytes!("fixtures/ui-translations/original-cli.json.gz").as_slice(),
        _ => panic!(),
    })
    .read_to_end(&mut bytes)
    .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn decode(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect()
}
fn catalog_value(c: openpilot_dev_tools::ui_translation_check::Catalog) -> Value {
    json!({"translations":c.translations.into_iter().map(|(k,v)|(k,json!(v))).collect::<serde_json::Map<String,Value>>(),"plurals":c.plurals.into_iter().map(|(k,v)|(k,json!(v))).collect::<serde_json::Map<String,Value>>()})
}
#[test]
fn independent_original_po_mutations_and_all_5733_plural_values() {
    let f = fixture("parser");
    for row in f["cases"].as_array().unwrap() {
        let result = parse_po(&decode(row["po_hex"].as_str().unwrap()));
        assert_eq!(
            result.is_ok(),
            row["error"].is_null(),
            "{}: {result:?}",
            row["case"]
        );
        if let Ok(c) = result {
            assert_eq!(
                json!(c.translations.iter().map(|(k, _)| k).collect::<Vec<_>>()),
                row["translation_keys"],
                "{}",
                row["case"]
            );
            assert_eq!(
                json!(c.plurals.iter().map(|(k, _)| k).collect::<Vec<_>>()),
                row["plural_keys"],
                "{}",
                row["case"]
            );
            assert_eq!(catalog_value(c), row["expected"], "{}", row["case"]);
        }
    }
    for (lang, values) in f["selectors"].as_object().unwrap() {
        for (n, value) in (-220..=220).zip(values.as_array().unwrap()) {
            assert_eq!(
                u64::from(selector(lang, n)),
                value.as_u64().unwrap(),
                "{lang}/{n}"
            );
        }
    }
}
struct Peer<'a> {
    root: &'a Path,
    row: &'a Value,
    calls: usize,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        assert_eq!(
            spec.effective_argv(),
            vec![
                match self.row["case"].as_str().unwrap() {
                    "relative-binary" => OsString::from("peer"),
                    "normalized-binary" => self.root.join("peer").into_os_string(),
                    _ => OsString::from("candidate"),
                },
                self.root
                    .join("openpilot/selfdrive/ui/translations")
                    .into_os_string()
            ]
        );
        assert!(spec.cwd.is_none());
        assert!(spec.inherit_stdin);
        if self.row["case"] == "relative-binary" {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
        }
        Ok(Captured {
            code: self.row["status"].as_i64().unwrap() as i32,
            stdout: decode(self.row["reply_hex"].as_str().unwrap()),
            stderr: decode(self.row["stderr_hex"].as_str().unwrap()),
        })
    }
}
#[test]
fn complete_original_checker_import_gates_dynamic_catalogs_and_failure_files() {
    let f = fixture("cli");
    for row in f["cases"].as_array().unwrap() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        let source = root.join("openpilot/system/ui/lib/multilang.py");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(
            source,
            include_bytes!("../../../../openpilot/system/ui/lib/multilang.py"),
        )
        .unwrap();
        let catalogs = root.join("openpilot/selfdrive/ui/translations");
        fs::create_dir_all(&catalogs).unwrap();
        fs::write(
            catalogs.join("languages.json"),
            decode(row["languages_hex"].as_str().unwrap()),
        )
        .unwrap();
        for (name, body) in row["catalogs_hex"].as_object().unwrap() {
            fs::write(
                catalogs.join(OsString::from_vec(decode(name))),
                decode(body.as_str().unwrap()),
            )
            .unwrap();
        }
        match row["case"].as_str().unwrap() {
            "directory-po" => fs::create_dir(catalogs.join("directory.po")).unwrap(),
            "dangling-po" => symlink("absent", catalogs.join("dangling.po")).unwrap(),
            _ => {}
        }
        let output = root.join("output");
        let mut peer = Peer {
            root,
            row,
            calls: 0,
        };
        let binary = match row["case"].as_str().unwrap() {
            "relative-binary" => std::path::PathBuf::from("./peer"),
            "normalized-binary" => std::path::PathBuf::from(format!("{}//./peer", root.display())),
            _ => std::path::PathBuf::from("candidate"),
        };
        let result = check(root, &binary, &output, &mut peer);
        assert_eq!(
            result.is_ok(),
            row["exit"] == 0,
            "{}: {result:?}",
            row["case"]
        );
        assert_eq!(
            peer.calls,
            usize::from(row["called"].as_bool().unwrap() || row["case"] == "relative-binary"),
            "{}",
            row["case"]
        );
        assert!(output.is_dir());
        let actual = fs::read_dir(&output)
            .unwrap()
            .map(|p| {
                let p = p.unwrap().path();
                (
                    p.file_name().unwrap().to_str().unwrap().to_owned(),
                    json!(fs::read(p)
                        .unwrap()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()),
                )
            })
            .collect::<serde_json::Map<String, Value>>();
        let expected = row["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v["hex"].clone()))
            .collect::<serde_json::Map<String, Value>>();
        assert_eq!(actual, expected, "{}", row["case"]);
        if let Ok(message) = result {
            assert_eq!(message, row["stdout"].as_str().unwrap());
        }
    }
}
#[test]
fn explicit_per_entry_plural_allocation_policy_has_checked_inclusive_boundary() {
    let body = b"msgid \"one\"\nmsgid_plural \"many\"\nmsgstr[1048575] \"last\"";
    let c = parse_po(body).unwrap();
    assert_eq!(c.plurals[0].1.len(), 1048576);
    assert_eq!(c.plurals[0].1.last().unwrap(), "last");
    assert!(
        parse_po(b"msgid \"one\"\nmsgid_plural \"many\"\nmsgstr[1048576] \"too many\"").is_err()
    );
}
#[test]
fn changed_reference_source_fails_closed_before_candidate_and_receipts() {
    struct Never;
    impl Runner for Never {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("changed reference must not spawn candidate")
        }
    }
    let t = tempfile::tempdir().unwrap();
    let source = t.path().join("openpilot/system/ui/lib/multilang.py");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(source, b"changed reference semantics").unwrap();
    let out = t.path().join("out");
    assert!(check(t.path(), Path::new("candidate"), &out, &mut Never).is_err());
    assert!(out.is_dir());
    assert_eq!(fs::read_dir(out).unwrap().count(), 0);
}
