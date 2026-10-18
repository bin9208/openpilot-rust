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
