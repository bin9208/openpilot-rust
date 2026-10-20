//! Requires a real Python 3.12 original run and native run on the same fresh ELF.
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer,
};
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
fn original_python_312_and_native_controller_match_full_sysfs_corpus() {
    let original = required("BLUETOOTH_ENUMERATE_ORIGINAL_OUTPUT");
    let native = required("BLUETOOTH_ENUMERATE_NATIVE_OUTPUT");
    assert_eq!(files(&original.join("sysfs")), files(&native.join("sysfs")));
    let old_raw = fs::read(original.join("result.json")).unwrap();
    let new_raw = fs::read(native.join("result.json")).unwrap();
    let old: Receipt = serde_json::from_slice(&old_raw).unwrap();
    let new: Receipt = serde_json::from_slice(&new_raw).unwrap();
    assert_eq!(old.nodes, 40);
    assert_eq!(new.nodes, old.nodes);
    assert_eq!(old.accepted, 31);
    assert_eq!(new.accepted, old.accepted);
    assert_eq!(new.source_sha256, old.source_sha256);
    assert_eq!(new.binary_sha256, old.binary_sha256);
    assert_eq!(
        rows(&old.source, &original),
        rows(&new.source, &native),
        "source"
    );
    assert_eq!(
        rows(&old.native, &original),
        rows(&new.native, &native),
        "native"
    );
    assert_eq!(rows(&old.source, &original), rows(&old.native, &original));
    assert_eq!(rows(&new.source, &native), rows(&new.native, &native));
    let original_stdout = fs::read(required("BLUETOOTH_ENUMERATE_ORIGINAL_STDOUT")).unwrap();
    let native_stdout = fs::read(required("BLUETOOTH_ENUMERATE_NATIVE_STDOUT")).unwrap();
    assert_eq!(original_stdout, native_stdout);
    assert_eq!(
        original_stdout,
        b"PASS: 40 sysfs nodes, 31 accepted in source order, missing directory\n"
    );
    let original_stderr = fs::read(required("BLUETOOTH_ENUMERATE_ORIGINAL_STDERR")).unwrap();
    let native_stderr = fs::read(required("BLUETOOTH_ENUMERATE_NATIVE_STDERR")).unwrap();
    assert_eq!(original_stderr, native_stderr);
    let _native_fixture_stdout = fs::read(native.join("native.stdout")).unwrap();
    let _native_fixture_stderr = fs::read(native.join("native.stderr")).unwrap();
    let _native_absent_stdout = fs::read(native.join("absent.stdout")).unwrap();
    let _native_absent_stderr = fs::read(native.join("absent.stderr")).unwrap();
}
