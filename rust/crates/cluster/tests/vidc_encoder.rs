#![cfg(feature = "native-media")]
use openpilot_cluster::h264::vidc::{Config, Encoder};

#[test]
fn source_invalid_configuration_fails_before_opening_a_kernel_device() {
    for config in [
        Config::default(),
        Config {
            width: 95,
            height: 64,
            ..Config::default()
        },
        Config {
            width: 96,
            height: 64,
            fps: 0,
            ..Config::default()
        },
        Config {
            width: 96,
            height: 64,
            bitrate: 0,
            ..Config::default()
        },
        Config {
            width: 96,
            height: 64,
            gop: 0,
            ..Config::default()
        },
        Config {
            width: 96,
            height: 64,
            slice_max_bytes: -1,
            ..Config::default()
        },
        Config {
            width: 96,
            height: 64,
            rate_control: 8,
            ..Config::default()
        },
        Config {
            width: 96,
            height: 64,
            device_path: String::new(),
            ..Config::default()
        },
    ] {
        assert!(Encoder::new(config).is_err());
    }
}

#[test]
fn actual_non_v4l_host_fd_preserves_querycap_errno_and_repeated_cleanup() {
    let mut encoder = Encoder::new(Config {
        width: 96,
        height: 64,
        device_path: "/dev/null".into(),
        ..Config::default()
    })
    .unwrap();
    let error = encoder.open().unwrap_err();
    assert!(
        matches!(error, openpilot_encoderd::Error::Io(ref error) if error.raw_os_error() == Some(25))
    );
    assert!(!encoder.is_open());
    encoder.close();
    encoder.close();
    assert!(!encoder.is_open());
}
