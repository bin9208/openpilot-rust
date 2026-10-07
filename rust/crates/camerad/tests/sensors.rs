use openpilot_camerad::sensor::{Exposure, ExposureScore, Register, SensorKind};

#[test]
fn os04c10_exposure_crossing_keeps_both_gain_channels_in_delayed_group() {
    let sensor = SensorKind::Os04c10;
    let registers = sensor
        .exposure_registers(Exposure {
            time: 2309,
            gain_index: 40,
            dc_gain: true,
        })
        .expect("valid source exposure");
    assert_eq!(
        registers.as_slice(),
        &[
            Register(0x3208, 0),
            Register(0x3501, 9),
            Register(0x3502, 5),
            Register(0x3508, 4),
            Register(0x3509, 0x40),
            Register(0x350c, 4),
            Register(0x350d, 0x40),
            Register(0x3208, 0x10),
            Register(0x3208, 0xa0),
        ]
    );
}

#[test]
fn score_preserves_the_original_target_compilers_rounding() {
    let score = SensorKind::Ar0231.exposure_score(ExposureScore {
        desired_ev: 11_156.352,
        time: 2115,
        gain_index: 2,
        gain: 0.714_285_73,
        previous_gain_index: 10,
    });
    let expected = if cfg!(target_arch = "aarch64") {
        173_623.0_f32
    } else {
        173_622.98_f32
    };
    assert_eq!(score.to_bits(), expected.to_bits());
}

#[test]
fn sensor_limits_and_probe_addresses_match_the_source() {
    for (sensor, time, gains, address) in [
        (SensorKind::Ar0231, 2133, 13, [0x20, 0x30, 0x20]),
        (SensorKind::Ox03c10, 2016, 54, [0x6c, 0x20, 0x6c]),
        (SensorKind::Os04c10, 2352, 40, [0x6c, 0x20, 0x6c]),
    ] {
        assert_eq!(sensor.config().exposure_time_max, time);
        assert_eq!(sensor.config().analog_gain_max_idx, gains);
        for (port, expected) in address.into_iter().enumerate() {
            assert_eq!(sensor.slave_address(port), Ok(expected));
        }
        assert!(sensor.slave_address(3).is_err());
        assert!(sensor
            .exposure_registers(Exposure {
                time: 5,
                gain_index: gains + 1,
                dc_gain: false,
            })
            .is_err());
    }
}
