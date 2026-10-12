use openpilot_cluster::route_replay::video::{
    path_for_stream, segment_at, Reader, Segment, VideoConfig,
};
use std::path::PathBuf;

fn segment(index: Option<i64>, start_t: f64, end_t: f64) -> Segment {
    Segment {
        index,
        path: PathBuf::from("owned.hevc"),
        start_t,
        end_t,
    }
}
#[test]
fn source_gap_grace_and_outside_bounds_select_segments() {
    let segments = vec![segment(None, 2.0, 3.0), segment(Some(7), 8.0, 9.0)];
    assert!(segment_at(&[], 0.0).is_none());
    for time in [-10.0, 2.0, 3.0, 3.5] {
        assert_eq!(segment_at(&segments, time), Some(&segments[0]));
    }
    for time in [3.500_000_1, 7.0, 8.0, 99.0] {
        assert_eq!(segment_at(&segments, time), Some(&segments[1]));
    }
}
#[test]
fn camera_paths_use_source_priority_and_only_files() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    std::fs::create_dir(dir.path().join("qcamera.ts"))?;
    std::fs::write(dir.path().join("fcamera.ts"), [])?;
    std::fs::write(dir.path().join("fcamera.hevc"), [])?;
    std::fs::write(dir.path().join("ecamera.ts"), [])?;
    assert_eq!(
        path_for_stream(dir.path(), "road"),
        Some(dir.path().join("fcamera.hevc"))
    );
    assert_eq!(
        path_for_stream(dir.path(), "other"),
        path_for_stream(dir.path(), "road")
    );
    assert_eq!(
        path_for_stream(dir.path(), "wide"),
        Some(dir.path().join("ecamera.ts"))
    );
    Ok(())
}
#[test]
fn reader_reports_missing_inputs_without_launching() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut reader = Reader::new(
        vec![],
        "wide",
        VideoConfig {
            ffmpeg: None,
            cwd: dir.path().into(),
        },
    );
    assert_eq!(reader.status_text(), "wide camera waiting");
    assert!(reader.frame_at(0.0)?.is_none());
    assert_eq!(reader.status_text(), "wide camera missing");
    let path = dir.path().join("input.hevc");
    reader.set_segments(vec![Segment {
        path: path.clone(),
        ..segment(None, 0.0, 2.0)
    }]);
    assert!(reader.frame_at(0.0)?.is_none());
    assert_eq!(reader.status_text(), "wide camera file missing");
    std::fs::write(path, [])?;
    assert!(reader.frame_at(0.0)?.is_none());
    assert_eq!(reader.status_text(), "wide camera ffmpeg missing");
    reader.close()?;
    Ok(())
}
