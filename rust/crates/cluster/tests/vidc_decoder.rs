#![cfg(feature = "native-media")]
use openpilot_cluster::media::vidc::{Config, Decoder};

#[test]
fn vidc_config_requires_source_positive_even_dimensions_and_fps() {
    let valid = Config {
        width: 96,
        height: 64,
        ..Config::default()
    };
    for config in [
        Config {
            width: 0,
            ..valid.clone()
        },
        Config {
            width: 95,
            ..valid.clone()
        },
        Config {
            height: 63,
            ..valid.clone()
        },
        Config {
            fps: 0,
            ..valid.clone()
        },
        Config {
            device_path: String::new(),
            ..valid.clone()
        },
    ] {
        assert!(Decoder::new(config).is_err());
    }
    assert!(Decoder::new(valid).is_ok());
}

#[test]
fn failed_native_vidc_open_preserves_kernel_error_and_closes_idempotently() {
    let mut decoder = Decoder::new(Config {
        width: 96,
        height: 64,
        device_path: "/dev/null".into(),
        ..Config::default()
    })
    .unwrap();
    let error = decoder.open().unwrap_err();
    assert!(
        matches!(&error,openpilot_encoderd::Error::Io(error) if error.raw_os_error()==Some(25)),
        "{error}"
    );
    decoder.close();
    decoder.close();
    println!("actual VIDC QUERYCAP ENOTTY returned and repeated close completed");
}
