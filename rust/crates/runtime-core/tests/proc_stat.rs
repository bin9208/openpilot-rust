use openpilot_runtime_core::proc_stat::{cpu_percent, ProcessStat};
use std::time::Duration;

fn line(name: &str, user: u64, system: u64, start: u64) -> String {
    let mut v = vec!["0".to_owned(); 50];
    v[0] = "R".into();
    v[1] = "1".into();
    v[11] = user.to_string();
    v[12] = system.to_string();
    v[15] = "20".into();
    v[16] = "-5".into();
    v[17] = "3".into();
    v[19] = start.to_string();
    v[20] = "4096".into();
    v[21] = "2".into();
    v[36] = "6".into();
    format!("123 ({name}) {}", v.join(" "))
}
fn record(user: u64, system: u64) -> ProcessStat {
    ProcessStat::parse(&line("worker", user, system, 500)).unwrap()
}

#[test]
fn preserves_names_and_signed_fields() {
    let p = ProcessStat::parse(&line("a ) (b\t c", 125, 25, 500)).unwrap();
    assert_eq!(p.name, "a ) (b\t c");
    assert_eq!(p.pid, 123);
    assert_eq!(p.user_ticks, 125);
    assert_eq!(p.system_ticks, 25);
    assert_eq!(p.start_ticks, 500);
    assert_eq!(p.processor, 6);
    assert_eq!(p.nice, -5);
    assert_eq!(p.threads, 3);
    assert_eq!(p.rss_pages, 2);
}
#[test]
fn rejects_truncated_and_malformed_records() {
    for l in [
        "".to_owned(),
        "123 no-parentheses".into(),
        "123 (x) R 1".into(),
        line("x", 1, 2, 3).replacen("123 (", "no (", 1),
        line("x", 1, 2, 3).replacen(" R ", " RUN ", 1),
    ] {
        assert!(ProcessStat::parse(&l).is_none());
    }
}
#[test]
fn computes_single_core_denominator_without_clamping() {
    let a = record(100, 100);
    let b = record(350, 150);
    assert_eq!(
        cpu_percent(&a, &b, Duration::from_secs(2), 100),
        Some(150.0)
    );
}
#[test]
fn rejects_pid_reuse_resets_and_invalid_timebase() {
    let a = record(100, 100);
    let mut b = record(200, 200);
    assert_eq!(cpu_percent(&a, &b, Duration::ZERO, 100), None);
    assert_eq!(cpu_percent(&a, &b, Duration::from_secs(1), 0), None);
    b.start_ticks += 1;
    assert_eq!(cpu_percent(&a, &b, Duration::from_secs(1), 100), None);
    b.start_ticks = a.start_ticks;
    b.pid += 1;
    assert_eq!(cpu_percent(&a, &b, Duration::from_secs(1), 100), None);
    assert_eq!(
        cpu_percent(&a, &record(99, 200), Duration::from_secs(1), 100),
        None
    );
}
#[test]
fn differences_large_counters_before_float_conversion() {
    let n = 1_u64 << 54;
    let a = record(n, n);
    let b = record(n + 1, n + 1);
    assert_eq!(cpu_percent(&a, &b, Duration::from_secs(1), 100), Some(2.0));
}
