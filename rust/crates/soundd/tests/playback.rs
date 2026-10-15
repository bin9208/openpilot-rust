use openpilot_soundd::{Playback, Sound};
#[test]
fn callback_retains_original_tail_repetition() {
    // Given: one finite sound, already partway through its samples.
    let mut playback = Playback::new(vec![Sound {
        alert: 1,
        samples: vec![1., 2., 3., 4.],
        loops: Some(1),
    }]);
    playback.update_alert(1);
    playback.frame = 2;
    playback.volume = 1.;
    let mut output = [0.; 5];
    // When: a callback spans the remaining tail several times.
    playback.render(&mut output).unwrap();
    // Then: the original local offset is retained throughout that callback.
    assert_eq!(output, [3., 4., 3., 4., 3.]);
    assert_eq!(playback.frame, 7);
    playback.render(&mut output).unwrap();
    assert_eq!(output, [0.; 5]);
}

#[test]
fn silent_buffer_retains_source_negative_volume_multiplication() {
    // Given: no alert with a negative stored volume adjustment.
    let mut playback = Playback::new(Vec::new());
    playback.volume = -0.5;
    let mut output = [1.; 4];
    // When: source semantics multiply even an all-zero silent buffer.
    playback.render(&mut output).unwrap();
    // Then: the output retains IEEE-754 negative zero.
    assert!(output
        .iter()
        .all(|sample| sample.to_bits() == (-0_f32).to_bits()));
}

#[test]
fn system_ready_plays_one_prompt_at_full_asset_gain() {
    use openpilot_cereal::car_capnp::car_control::h_u_d_control::AudibleAlert;
    use openpilot_logging::producer::Factory;
    use openpilot_soundd::assets::Assets;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../openpilot/selfdrive/assets");
    let mut logger = Factory::new("invalid://endpoint".into()).unwrap().logger();
    let sounds = Assets {
        root,
        engage_volume: 0.25,
        tizi: false,
    }
    .load("en", &mut logger)
    .unwrap();
    let ready = sounds
        .iter()
        .find(|sound| sound.alert == u16::from(AudibleAlert::SystemReady))
        .expect("systemReady source mapping is prompt.wav");
    let prompt = sounds
        .iter()
        .find(|sound| sound.alert == u16::from(AudibleAlert::Prompt))
        .unwrap();
    assert_eq!(ready.loops, Some(1));
    assert!(!ready.samples.is_empty());
    assert_eq!(ready.samples, prompt.samples);
}

#[test]
fn monitoring_gain_follows_event_identity_even_when_another_event_reuses_the_wave() {
    use openpilot_soundd::{Input, Policy};
    let mut policy = Policy::new(
        [5, 8]
            .into_iter()
            .map(|alert| Sound {
                alert,
                samples: vec![1.; 4],
                loops: None,
            })
            .collect(),
        false,
    );
    policy.playback.volume = 0.1;
    for (alert, alert_type, expected) in [
        (8, "driverDistracted2/permanent", 0.7),
        (8, "navigation/permanent", 0.1),
        (5, "driverUnresponsive3/permanent", 1.),
        (5, "controlsUnresponsive/permanent", 0.1),
        (5, "driverDistracted2/permanent", 0.1),
        (8, "driverUnresponsive2/permanent", 0.7),
    ] {
        let input: Input = serde_json::from_value(serde_json::json!({
            "updated_selfdrive":true, "alert":alert, "alert_type":alert_type,
        }))
        .unwrap();
        policy.step(&input);
        let mut output = [0.];
        policy.playback.render(&mut output).unwrap();
        assert_eq!(output, [expected], "{alert_type}");
    }
}
