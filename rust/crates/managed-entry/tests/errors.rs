use openpilot_managed_entry::{EntryError, NativeError, Stage};

#[test]
fn typed_native_cause_and_stage_survive_the_boundary() {
    let original = std::io::Error::from_raw_os_error(2);
    let failure = EntryError::Raised {
        stage: Stage::Prepare,
        error: NativeError::new(original),
    };
    let EntryError::Raised { stage, error } = failure else {
        panic!("wrong failure branch")
    };
    assert_eq!(stage, Stage::Prepare);
    assert_eq!(
        error.exception.kind,
        std::any::type_name::<std::io::Error>()
    );
    assert!(error.exception.message.contains("os error 2"));
    assert!(std::error::Error::source(&error)
        .unwrap()
        .is::<std::io::Error>());
}
