use openpilot_log_format::{JsonValue, JsonView};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/registration")
        .join(name)
}
pub fn read(path: &Path) -> JsonValue {
    JsonValue::parse(&fs::read_to_string(path).unwrap()).unwrap()
}
pub fn field(value: &JsonValue, name: &str) -> JsonValue {
    value.get(name).unwrap_or_else(|| panic!("missing {name}"))
}
pub fn array(value: &JsonValue) -> Vec<JsonValue> {
    match value.view() {
        JsonView::Array(v) => v,
        _ => panic!("expected array"),
    }
}
pub fn text(value: &JsonValue) -> String {
    value.to_utf8().expect("expected scalar Unicode text")
}

// Compare independently captured values without serde's surrogate/NaN/int limits.
// Preserve numeric types, signed zero, arbitrary integers and all text points.
pub fn assert_json(actual: &JsonValue, expected: &JsonValue, context: &str) {
    fn equal(a: &JsonValue, b: &JsonValue) -> bool {
        match (a.view(), b.view()) {
            (JsonView::Null, JsonView::Null) => true,
            (JsonView::Bool(a), JsonView::Bool(b)) => a == b,
            (JsonView::Integer(a), JsonView::Integer(b)) => a == b,
            (JsonView::Float(a), JsonView::Float(b)) => {
                a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
            }
            (JsonView::Text(a), JsonView::Text(b)) => a == b,
            (JsonView::Array(a), JsonView::Array(b)) => {
                a.len() == b.len() && a.iter().zip(&b).all(|(a, b)| equal(a, b))
            }
            (JsonView::Object(a), JsonView::Object(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| {
                        b.iter()
                            .find(|(q, _)| q == k)
                            .is_some_and(|(_, b)| equal(v, b))
                    })
            }
            _ => false,
        }
    }
    assert!(
        equal(actual, expected),
        "{context}\nactual={}\noriginal={}",
        actual.to_json().unwrap(),
        expected.to_json().unwrap()
    );
}
pub fn worker() -> PathBuf {
    let path=PathBuf::from(std::env::var_os("REGISTRATION_WORKER_BINARY").expect("prerequisite: REGISTRATION_WORKER_BINARY must name the fresh source-matched check-registration ELF"));
    assert!(
        path.is_absolute() && path.is_file(),
        "missing registration worker prerequisite"
    );
    path
}
