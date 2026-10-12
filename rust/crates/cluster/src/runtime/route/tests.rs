use super::Playback;

#[test]
fn seek_pause_and_resume_preserve_the_selected_time_at_nonunit_rate() {
    let mut clock = Playback::new(2.0, 20.0, 100.0, 3.0);
    assert_eq!(clock.time(102.0).to_bits(), 8.0_f64.to_bits());
    clock.toggle(102.0, 8.0);
    assert_eq!(clock.time(500.0).to_bits(), 8.0_f64.to_bits());
    clock.seek(600.0, 12.5);
    assert!(clock.paused);
    clock.toggle(700.0, 12.5);
    assert_eq!(clock.time(701.0).to_bits(), 15.5_f64.to_bits());
}

#[test]
fn end_wait_retries_once_a_second_and_loop_resets_waiting() {
    let mut clock = Playback::new(-1.0, 20.0, 100.0, 1.0);
    assert_eq!(clock.time(100.0).to_bits(), 0.0_f64.to_bits());
    clock.finish(121.0, 20.0);
    assert!(clock.retry_due(121.0));
    assert!(!clock.retry_due(121.9));
    assert!(clock.retry_due(122.0));
    clock.resume_loop(123.0);
    assert!(!clock.waiting);
    assert!(!clock.paused);
    assert_eq!(clock.time(124.0).to_bits(), 1.0_f64.to_bits());
}

#[test]
fn only_recorded_sound_rising_edge_pauses_and_seek_rearms_it() {
    let mut clock = Playback::new(0.0, 20.0, 100.0, 1.0);
    assert!(!clock.cutin(false, true, 101.0, 1.0));
    assert!(clock.cutin(true, true, 102.0, 2.0));
    assert!(!clock.cutin(true, true, 103.0, 2.0));
    clock.seek(104.0, 1.0);
    assert!(clock.cutin(true, true, 105.0, 1.0));
    assert!(!clock.cutin(true, false, 106.0, 1.0));
    assert!(clock.cutin(true, true, 107.0, 1.0));
}
