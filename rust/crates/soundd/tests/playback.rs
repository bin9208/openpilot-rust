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
