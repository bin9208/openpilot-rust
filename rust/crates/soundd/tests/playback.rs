use openpilot_soundd::{Playback, Sound};
#[test]
fn finite_clip_stops_at_its_end_inside_a_callback() {
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
    // When: a callback is longer than the remaining finite clip.
    playback.render(&mut output).unwrap();
    // Then: only the remaining samples play; the rest is silence.
    assert_eq!(output, [3., 4., 0., 0., 0.]);
    assert_eq!(playback.frame, 4);
    playback.render(&mut output).unwrap();
    assert_eq!(output, [0.; 5]);
    assert_eq!(playback.frame, 4);
}

fn clip(loops: Option<usize>) -> Playback {
    let mut playback = Playback::new(vec![Sound {
        alert: 1,
        samples: vec![1., 2., 3., 4.],
        loops,
    }]);
    playback.update_alert(1);
    playback.volume = 1.;
    playback
}

#[test]
fn infinite_clip_wraps_to_its_start_across_callbacks() {
    let mut playback = clip(None);
    playback.frame = 2;
    let mut output = [0.; 5];
    playback.render(&mut output).unwrap();
    assert_eq!(output, [3., 4., 1., 2., 3.]);
    assert_eq!(playback.frame, 7);
    let mut next = [0.; 3];
    playback.render(&mut next).unwrap();
    assert_eq!(next, [4., 1., 2.]);
    assert_eq!(playback.frame, 10);
}

#[test]
fn finite_repeats_are_counted_within_a_single_callback() {
    let mut playback = clip(Some(2));
    let mut output = [0.; 11];
    playback.render(&mut output).unwrap();
    assert_eq!(output, [1., 2., 3., 4., 1., 2., 3., 4., 0., 0., 0.]);
    assert_eq!(playback.frame, 8);
}

#[test]
fn finite_repeats_keep_sequence_across_unequal_callbacks() {
    let mut playback = clip(Some(2));
    let mut first = [0.; 3];
    playback.render(&mut first).unwrap();
    assert_eq!(first, [1., 2., 3.]);
    let mut second = [0.; 4];
    playback.render(&mut second).unwrap();
    assert_eq!(second, [4., 1., 2., 3.]);
    let mut third = [0.; 3];
    playback.render(&mut third).unwrap();
    assert_eq!(third, [4., 0., 0.]);
    assert_eq!(playback.frame, 8);
}

#[test]
fn zero_repeats_and_empty_callbacks_do_not_advance_playback() {
    let mut zero = clip(Some(0));
    let mut output = [99.; 5];
    zero.render(&mut output).unwrap();
    assert_eq!(output, [0.; 5]);
    assert_eq!(zero.frame, 0);
    let mut infinite = clip(None);
    infinite.frame = 3;
    infinite.render(&mut []).unwrap();
    assert_eq!(infinite.frame, 3);
}

#[test]
fn sample_counter_overflow_is_reported_without_wrapping() {
    let mut playback = clip(None);
    playback.frame = usize::MAX;
    let error = playback.render(&mut [0.; 1]).unwrap_err();
    assert!(matches!(
        error,
        openpilot_soundd::Error::Contract("sample counter overflow")
    ));
    assert_eq!(playback.frame, usize::MAX);
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
fn shipped_samples_cross_full_callbacks_without_repeating_the_old_suffix() {
    use openpilot_logging::producer::Factory;
    use openpilot_soundd::assets::Assets;
    use std::path::PathBuf;

    const CALLBACK_FRAMES: usize = 4096;
    let mut logger = Factory::new("inproc://soundd-playback-392".into())
        .unwrap()
        .logger();
    for tizi in [false, true] {
        let assets = Assets {
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../openpilot/selfdrive/assets"),
            engage_volume: 0.37,
            tizi,
        };
        for sound in assets.load("en", &mut logger).unwrap() {
            if !matches!(sound.alert, 1 | 6 | 7 | 4 | 5) {
                continue;
            }
            let alert = sound.alert;
            let samples = sound.samples.clone();
            let limit = sound.loops.map(|loops| loops * samples.len());
            assert_ne!(samples.len() % CALLBACK_FRAMES, 0);
            let start = samples.len() - 2;
            let mut playback = Playback::new(vec![sound]);
            playback.update_alert(alert);
            playback.frame = start;
            playback.volume = 0.73;
            for block in 0..2 {
                // A per-sample linear index is independent of native span copying.
                let expected: Vec<f32> = (0..CALLBACK_FRAMES)
                    .map(|position| {
                        let index = start + block * CALLBACK_FRAMES + position;
                        let sample = if limit.is_some_and(|end| index >= end) {
                            0.
                        } else {
                            samples[index % samples.len()]
                        };
                        sample * 0.73_f32
                    })
                    .collect();
                let mut output = [99.; CALLBACK_FRAMES];
                playback.render(&mut output).unwrap();
                for (position, (actual, expected)) in output.iter().zip(&expected).enumerate() {
                    assert_eq!(
                        actual.to_bits(),
                        expected.to_bits(),
                        "tizi={tizi}, alert={alert}, block={block}, sample={position}"
                    );
                }
                let end = start + (block + 1) * CALLBACK_FRAMES;
                assert_eq!(playback.frame, limit.map_or(end, |limit| end.min(limit)));
            }
        }
    }
}
