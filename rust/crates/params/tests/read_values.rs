use openpilot_params::{Error, Params};
use std::{fs, io};

#[test]
fn complete_binary_values_across_read_buffer_boundaries() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    for size in [0, 1, 255, 256, 257, 4095, 4096, 4097, 65536, 1_048_576] {
        let expected = [0, 255, 42, 128]
            .into_iter()
            .cycle()
            .take(size)
            .collect::<Vec<_>>();
        params.put("CarParams", &expected).unwrap();

        let value = params.get("CarParams").unwrap();

        assert_eq!(value.as_deref(), Some(expected.as_slice()), "size={size}");
    }
}

#[test]
fn read_observes_atomic_replacement_without_reopening_namespace() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    params.put("CarParams", b"old").unwrap();
    let replacement = root.path().join("replacement");
    fs::write(&replacement, b"new\0\xff").unwrap();
    fs::rename(replacement, params.directory().join("CarParams")).unwrap();

    let value = params.get("CarParams").unwrap();

    assert_eq!(value.as_deref(), Some(b"new\0\xff".as_slice()));
}

#[test]
fn read_directory_error_is_not_a_missing_or_empty_value() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    fs::create_dir(params.directory().join("CarParams")).unwrap();

    let error = params.get("CarParams").unwrap_err();

    assert!(matches!(error, Error::Io(error) if error.kind() == io::ErrorKind::IsADirectory));
}
