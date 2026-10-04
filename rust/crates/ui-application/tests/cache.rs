use openpilot_ui_application::cache::TimedCache;
#[test]
fn pending_acknowledgement_and_deadline_preserve_source_values() {
    let mut cache = TimedCache::new(false);
    cache.store_pending(true, 1.0);
    assert!(*cache.refresh(1.49, || Ok::<_, ()>(false)));
    assert!(*cache.refresh(1.5, || Ok::<_, ()>(false)));
    assert!(!(*cache.refresh(2.0, || Ok::<_, ()>(false))));
    cache.store_pending(true, 2.0);
    assert!(*cache.refresh(2.5, || Ok::<_, ()>(true)));
    assert!(!(*cache.refresh(3.0, || Ok::<_, ()>(false))));
}
#[test]
fn retries_back_off_without_replacing_the_last_complete_value() {
    let mut cache = TimedCache::new(42);
    for (now, next) in [
        (0.0, 0.05),
        (0.05, 0.15),
        (0.16, 0.36),
        (0.36, 0.76),
        (0.76, 1.26),
        (1.26, 1.76),
    ] {
        assert_eq!(*cache.refresh(now, || Err::<i32, _>(())), 42);
        assert!((cache.next_refresh_time - next).abs() < 1e-12);
    }
    assert_eq!(*cache.refresh(2.0, || Ok::<_, ()>(7)), 7);
    assert_eq!(cache.next_refresh_time, 2.5);
}
