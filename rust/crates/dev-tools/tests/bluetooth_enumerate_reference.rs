//! Sealed Python 3.12 original observation and two fresh native same-ELF lanes.
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer,
};
use sha2::{Digest, Sha256};
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct Receipt {
    nodes: usize,
    accepted: usize,
    source: Ordered,
    native: Ordered,
    source_sha256: String,
    binary_sha256: String,
}
struct Ordered(Vec<(String, String)>);
impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedVisitor;
        impl<'de> Visitor<'de> for OrderedVisitor {
            type Value = Ordered;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("ordered device map")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Ordered, M::Error> {
                let mut result = Vec::new();
                while let Some(row) = map.next_entry::<String, String>()? {
                    result.push(row);
                }
                Ok(Ordered(result))
            }
        }
        deserializer.deserialize_map(OrderedVisitor)
    }
}

fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("required live enumeration evidence missing: {name}"))
}
fn rows(value: &Ordered, output: &Path) -> Vec<(String, String)> {
    let root = fs::canonicalize(output).unwrap();
    let prefix = format!("{}/dev/", root.display());
    value
        .0
        .iter()
        .map(|(path, address)| {
            (
                path.strip_prefix(&prefix)
                    .expect("device path under run root")
                    .to_owned(),
                address.to_owned(),
            )
        })
        .collect()
}
fn original_rows(value: &Ordered) -> Vec<(String, String)> {
    const PREFIX: &str =
        "/home/runner/work/_temp/ui-connectivity/bluetooth-enumerate/original/dev/";
    value
        .0
        .iter()
        .map(|(path, address)| {
            (
                path.strip_prefix(PREFIX)
                    .expect("sealed original device path")
                    .to_owned(),
                address.clone(),
            )
        })
        .collect()
}
fn pairs(rows: &[(String, String)]) -> std::collections::BTreeMap<&str, &str> {
    let mapped = rows
        .iter()
        .map(|(path, address)| (path.as_str(), address.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(mapped.len(), rows.len(), "duplicate device path");
    mapped
}
fn files(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    fn visit(base: &Path, path: &Path, out: &mut Vec<(PathBuf, Option<Vec<u8>>)>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                out.push((entry.path().strip_prefix(base).unwrap().to_owned(), None));
                visit(base, &entry.path(), out);
            } else {
                out.push((
                    entry.path().strip_prefix(base).unwrap().to_owned(),
                    Some(fs::read(entry.path()).unwrap()),
                ));
            }
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
#[test]
fn sealed_original_and_fresh_native_match_full_sysfs_corpus() {
    let native = required("BLUETOOTH_ENUMERATE_NATIVE_OUTPUT");
    let absent = required("BLUETOOTH_ENUMERATE_ABSENT_OUTPUT");
    let absent_root = required("BLUETOOTH_ENUMERATE_ABSENT_ROOT");
    let checker = absent_root.join("rust/tools/check_bluetooth_enumerate.py");
    assert_eq!(
        fs::symlink_metadata(checker).unwrap_err().kind(),
        std::io::ErrorKind::NotFound,
        "historical checker must be absent from the copied source root"
    );
    assert_eq!(files(&native.join("sysfs")), files(&absent.join("sysfs")));
    let old_raw = include_bytes!("fixtures/bluetooth-enumerate/original-result.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(old_raw)),
        "36d25b8684cb814421dd22ada7bc75b3d9959c38433e4a5b75c70e02375ac18c"
    );
    let new_raw = fs::read(native.join("result.json")).unwrap();
    let absent_raw = fs::read(absent.join("result.json")).unwrap();
    let old: Receipt = serde_json::from_slice(&old_raw).unwrap();
    let new: Receipt = serde_json::from_slice(&new_raw).unwrap();
    let missing: Receipt = serde_json::from_slice(&absent_raw).unwrap();
    assert_eq!(old.nodes, 40);
    assert_eq!(new.nodes, old.nodes);
    assert_eq!(missing.nodes, old.nodes);
    assert_eq!(old.accepted, 31);
    assert_eq!(new.accepted, old.accepted);
    assert_eq!(missing.accepted, old.accepted);
    assert_eq!(new.source_sha256, old.source_sha256);
    assert_eq!(missing.source_sha256, old.source_sha256);
    assert_eq!(missing.binary_sha256, new.binary_sha256);
    assert_eq!(
        pairs(&original_rows(&old.source)),
        pairs(&rows(&new.source, &native)),
        "source pairs"
    );
    assert_eq!(
        pairs(&original_rows(&old.native)),
        pairs(&rows(&new.native, &native)),
        "native pairs"
    );
    assert_eq!(
        pairs(&original_rows(&old.source)),
        pairs(&rows(&missing.source, &absent)),
        "absent source pairs"
    );
    assert_eq!(
        pairs(&original_rows(&old.native)),
        pairs(&rows(&missing.native, &absent)),
        "absent native pairs"
    );
    assert_eq!(original_rows(&old.source), original_rows(&old.native));
    assert_eq!(rows(&new.source, &native), rows(&new.native, &native));
    assert_eq!(rows(&new.source, &native), rows(&missing.source, &absent));
    assert_eq!(rows(&new.native, &native), rows(&missing.native, &absent));
    assert_eq!(
        rows(&missing.source, &absent),
        rows(&missing.native, &absent)
    );
    let native_stdout = fs::read(required("BLUETOOTH_ENUMERATE_NATIVE_STDOUT")).unwrap();
    let absent_stdout = fs::read(required("BLUETOOTH_ENUMERATE_ABSENT_STDOUT")).unwrap();
    assert_eq!(native_stdout, absent_stdout);
    assert_eq!(
        native_stdout,
        b"PASS: 40 sysfs nodes, 31 accepted in source order, missing directory\n"
    );
    let native_stderr = fs::read(required("BLUETOOTH_ENUMERATE_NATIVE_STDERR")).unwrap();
    let absent_stderr = fs::read(required("BLUETOOTH_ENUMERATE_ABSENT_STDERR")).unwrap();
    assert_eq!(native_stderr, absent_stderr);
    assert!(native_stderr.is_empty(), "original stderr was empty");
    assert_eq!(
        fs::read(native.join("native.stderr")).unwrap(),
        fs::read(absent.join("native.stderr")).unwrap()
    );
    assert_eq!(
        fs::read(native.join("absent.stdout")).unwrap(),
        fs::read(absent.join("absent.stdout")).unwrap()
    );
    assert_eq!(
        fs::read(native.join("absent.stderr")).unwrap(),
        fs::read(absent.join("absent.stderr")).unwrap()
    );
    let present_fixture: Ordered =
        serde_json::from_slice(&fs::read(native.join("native.stdout")).unwrap()).unwrap();
    let absent_fixture: Ordered =
        serde_json::from_slice(&fs::read(absent.join("native.stdout")).unwrap()).unwrap();
    assert_eq!(
        rows(&present_fixture, &native),
        rows(&absent_fixture, &absent)
    );
}
