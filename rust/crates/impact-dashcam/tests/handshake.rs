use openpilot_impact_dashcam::*;
use serde_json::json;
use std::{collections::BTreeMap, io};
#[derive(Default)]
struct Memory {
    values: BTreeMap<String, Vec<u8>>,
    writes: Vec<(String, Vec<u8>)>,
    drop_writes: bool,
}
impl Store for Memory {
    fn get(&mut self, key: &str) -> io::Result<Option<Vec<u8>>> {
        Ok(self.values.get(key).cloned())
    }
    fn put(&mut self, key: &str, v: &[u8]) -> io::Result<()> {
        if !self.drop_writes {
            self.values.insert(key.into(), v.into());
            self.writes.push((key.into(), v.into()));
        }
        Ok(())
    }
    fn remove(&mut self, key: &str) -> io::Result<()> {
        self.values.remove(key);
        Ok(())
    }
}
fn pending() -> (ImpactDashcam, Memory, Memory) {
    let (mut p, mut m) = (Memory::default(), Memory::default());
    let mut s = ImpactDashcam::new(&mut p, &mut m).unwrap();
    s.update(1., true, true, Some(false), json!({}), &mut p, &mut m)
        .unwrap();
    (s, p, m)
}
fn feedback(
    s: &mut ImpactDashcam,
    p: &mut Memory,
    m: &mut Memory,
    now: f64,
    cancel: bool,
) -> io::Result<()> {
    m.put(
        FEEDBACK_KEY,
        &serde_json::to_vec(&json!({"token":s.token,"visible":now,"cancel":cancel}))?,
    )?;
    s.update(now, true, false, Some(true), json!({}), p, m)
}
#[test]
fn durable_off_precedes_reboot_and_commit_is_once() {
    let (mut s, mut p, mut m) = pending();
    for i in 0..100 {
        feedback(&mut s, &mut p, &mut m, 1.1 + i as f64 / 10., false).unwrap();
        assert!(!s.committed);
    }
    feedback(&mut s, &mut p, &mut m, 11.1, false).unwrap();
    assert!(s.committed);
    assert_eq!(
        p.writes,
        vec![
            ("OpenpilotEnabledToggle".into(), b"0".to_vec()),
            (REBOOT_KEY.into(), b"1".to_vec()),
            ("DoReboot".into(), b"1".to_vec())
        ]
    );
    s.update(12., true, true, Some(false), json!({}), &mut p, &mut m)
        .unwrap();
    assert_eq!(p.writes.len(), 3);
}
#[test]
fn cancellation_at_deadline_wins_and_quiet_rearms() {
    let (mut s, mut p, mut m) = pending();
    for i in 0..100 {
        feedback(&mut s, &mut p, &mut m, 1.1 + i as f64 / 10., false).unwrap();
    }
    feedback(&mut s, &mut p, &mut m, 11.1, true).unwrap();
    assert!(!s.pending && p.writes.is_empty());
    for (t, tr, q) in [
        (11.2, true, false),
        (12., false, true),
        (13.01, false, true),
        (13.02, true, false),
    ] {
        s.update(t, true, tr, Some(q), json!({}), &mut p, &mut m)
            .unwrap();
    }
    assert!(s.pending);
}
#[test]
fn invisible_stale_future_malformed_or_wrong_token_cannot_reboot() {
    for mode in 0..7 {
        let (mut s, mut p, mut m) = pending();
        match mode {
            0 => {}
            1 | 2 => {
                feedback(&mut s, &mut p, &mut m, 1.1, false).unwrap();
            }
            3 => m.put(FEEDBACK_KEY, b"invalid").unwrap(),
            4 => m
                .put(FEEDBACK_KEY, b"{\"token\":\"old\",\"visible\":12}")
                .unwrap(),
            5 => {
                m.put(
                    FEEDBACK_KEY,
                    &serde_json::to_vec(&json!({"token":s.token,"visible":99})).unwrap(),
                )
                .unwrap();
            }
            _ => {}
        }
        s.update(
            12.,
            mode != 6,
            false,
            Some(false),
            json!({}),
            &mut p,
            &mut m,
        )
        .unwrap();
        assert!(!s.pending && p.writes.is_empty(), "mode {mode}");
    }
}
#[test]
fn ui_return_after_gap_cannot_skip_hidden_time() {
    let (mut s, mut p, mut m) = pending();
    feedback(&mut s, &mut p, &mut m, 1.1, false).unwrap();
    feedback(&mut s, &mut p, &mut m, 12., false).unwrap();
    assert!(!s.pending && p.writes.is_empty());
}
#[test]
fn readback_blocks_silent_write_failure() {
    let (mut s, mut p, mut m) = pending();
    p.put("OpenpilotEnabledToggle", b"1").unwrap();
    p.drop_writes = true;
    for i in 0..100 {
        feedback(&mut s, &mut p, &mut m, 1.1 + i as f64 / 10., false).unwrap();
    }
    assert!(feedback(&mut s, &mut p, &mut m, 11.1, false).is_err());
    assert!(!s.committed);
    assert!(p.get("DoReboot").unwrap().is_none());
}
#[test]
fn startup_retains_durable_interlock_and_clears_volatile_notice() {
    let (mut p, mut m) = (Memory::default(), Memory::default());
    p.put(REBOOT_KEY, b"1").unwrap();
    m.put(NOTICE_KEY, b"stale").unwrap();
    m.put(FEEDBACK_KEY, b"stale").unwrap();
    let s = ImpactDashcam::new(&mut p, &mut m).unwrap();
    assert!(s.committed && m.values.is_empty());
}

#[test]
fn actual_pinned_source_handshake_trace_matches_every_state_and_param_action() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/final-handshake.json")).unwrap();
    let (mut p, mut m) = (Memory::default(), Memory::default());
    let mut s = ImpactDashcam::new(&mut p, &mut m).unwrap();
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        if row["reset"] == true {
            p = Memory::default();
            m = Memory::default();
            s = ImpactDashcam::new(&mut p, &mut m).unwrap();
        }
        if !row["feedback"].is_null() {
            m.put(FEEDBACK_KEY, &serde_json::to_vec(&row["feedback"]).unwrap())
                .unwrap();
        }
        s.update(
            row["now"].as_f64().unwrap(),
            row["allowed"].as_bool().unwrap(),
            row["trigger"].as_bool().unwrap(),
            row["quiet"].as_bool(),
            json!({"acceleration":null,"aEgo":0.}),
            &mut p,
            &mut m,
        )
        .unwrap();
        assert_eq!(serde_json::to_value(&s).unwrap(), row["state"], "state {i}");
        let durable: serde_json::Map<String, serde_json::Value> = p
            .values
            .iter()
            .map(|(k, v)| (k.clone(), json!(v == b"1")))
            .collect();
        assert_eq!(json!(durable), row["params"], "durable {i}");
        let volatile: serde_json::Map<String, serde_json::Value> = m
            .values
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::from_slice(v).unwrap()))
            .collect();
        assert_eq!(json!(volatile), row["memory"], "memory {i}");
    }
}

#[test]
fn native_params_absent_volatile_keys_do_not_block_initialization_or_cancel() {
    let root = std::env::temp_dir().join(format!("impact-native-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let durable = openpilot_params::Params::open(&root.join("durable"), "d").unwrap();
    let volatile = openpilot_params::Params::open(&root.join("volatile"), "d").unwrap();
    let mut state = ImpactDashcam::new(&mut &durable, &mut &volatile).unwrap();
    state
        .update(
            1.,
            true,
            true,
            Some(false),
            json!({}),
            &mut &durable,
            &mut &volatile,
        )
        .unwrap();
    assert!(state.pending);
    state.cancel(&mut &volatile).unwrap();
    state.cancel(&mut &volatile).unwrap();
    assert!(!state.pending);
    std::fs::remove_dir_all(root).unwrap();
}
