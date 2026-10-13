use super::fixture::{file, Fixture, State};
use super::*;
use std::{
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
};

#[test]
fn interrupted_put_is_cleaned_and_retried_from_start() {
    let fixture = Fixture::new(State {
        interrupt_put: true,
        ..State::default()
    });
    let root = tempfile::tempdir().unwrap();
    let bytes = vec![19; 2 * 1024 * 1024];
    file(root.path(), "qcamera.ts", &bytes);
    assert!(fixture
        .client
        .upload_folder(
            super::super::LocalFiles {
                folder: root.path(),
                names: &["qcamera.ts".into()]
            },
            super::super::RouteSegment {
                directory: "car device",
                name: "segment"
            },
            &mut Observer::default()
        )
        .unwrap());
    let state = fixture.state();
    assert_eq!(state.puts, 2);
    assert_eq!(state.files.len(), 1);
    assert_eq!(
        state.files["/openpilot/routes/car%20device/segment/qcamera.ts"],
        bytes
    );
    assert!(state
        .requests
        .iter()
        .any(|(method, _, _)| method == "DELETE"));
}

#[test]
fn canceled_put_leaves_no_partial_or_final_file() {
    let fixture = Fixture::new(State::default());
    let root = tempfile::tempdir().unwrap();
    file(root.path(), "qcamera.ts", &vec![5; 2 * 1024 * 1024]);
    let stopped = AtomicBool::new(false);
    let mut cancel = || stopped.load(Ordering::Acquire);
    let mut progress = |event: Progress<'_>| {
        if event.sent >= 64 * 1024 {
            stopped.store(true, Ordering::Release);
        }
        Ok(())
    };
    let error = fixture
        .client
        .upload_folder(
            super::super::LocalFiles {
                folder: root.path(),
                names: &["qcamera.ts".into()],
            },
            super::super::RouteSegment {
                directory: "car device",
                name: "segment",
            },
            &mut Observer {
                cancel: Some(&mut cancel),
                progress: Some(&mut progress),
            },
        )
        .unwrap_err();
    assert!(matches!(error, Error::Canceled));
    assert!(fixture.state().files.is_empty());
    assert!(!fixture
        .state()
        .requests
        .iter()
        .any(|(method, _, _)| method == "MOVE"));
}

#[test]
fn failed_move_cleans_temporary_file_and_remains_failure() {
    let fixture = Fixture::new(State {
        fail_method: Some(("MOVE", 500)),
        ..State::default()
    });
    let root = tempfile::tempdir().unwrap();
    file(root.path(), "rlog.zst", b"route log");
    assert!(fixture
        .client
        .upload_folder(
            super::super::LocalFiles {
                folder: root.path(),
                names: &["rlog.zst".into()]
            },
            super::super::RouteSegment {
                directory: "car device",
                name: "segment"
            },
            &mut Observer::default()
        )
        .is_err());
    let state = fixture.state();
    assert!(state.files.is_empty());
    assert_eq!(state.puts, 2);
    assert_eq!(
        state
            .requests
            .iter()
            .filter(|(method, _, _)| method == "DELETE")
            .count(),
        2
    );
}

#[test]
fn source_growth_during_put_is_not_published_as_a_complete_file() {
    let fixture = Fixture::new(State::default());
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("qcamera.ts");
    file(root.path(), "qcamera.ts", &vec![5; 128 * 1024]);
    let mut changed = false;
    let mut progress = |event: Progress<'_>| {
        if event.sent > 0 && !changed {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(b"more")
                .unwrap();
            changed = true;
        }
        Ok(())
    };
    assert!(fixture
        .client
        .upload_folder(
            super::super::LocalFiles {
                folder: root.path(),
                names: &["qcamera.ts".into()]
            },
            super::super::RouteSegment {
                directory: "car device",
                name: "segment"
            },
            &mut Observer {
                cancel: None,
                progress: Some(&mut progress)
            }
        )
        .is_err());
    assert!(fixture.state().files.is_empty());
    assert!(!fixture
        .state()
        .requests
        .iter()
        .any(|(method, _, _)| method == "MOVE"));
}
