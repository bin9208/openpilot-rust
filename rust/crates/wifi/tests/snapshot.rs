//! #397: nine snapshot fields, stable sorting and independent returned storage.
#[path = "snapshot/cases.rs"]
mod cases;

#[test]
fn snapshot_preserves_all_46_semantic_cases_and_source_storage() {
    assert_eq!(cases::semantic_checks(), 46);
}
