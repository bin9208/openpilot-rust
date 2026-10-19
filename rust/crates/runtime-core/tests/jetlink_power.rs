use openpilot_runtime_core::jetlink_power::{boot_wanted, Action, Draw};
use std::io;

#[test]
fn enabled_link_cuts_draw_before_a_gadget_even_with_a_host_already_connected() {
    for role in [Some("none"), Some("source"), Some("sink"), None] {
        let mut policy = Draw::default();
        let mut calls = Vec::new();
        policy
            .update(true, role, |action| {
                calls.push(action);
                Ok(())
            })
            .unwrap();
        assert_eq!(calls, [Action::Cut]);
        assert!(policy.cut);
        assert_eq!(policy.settled, Some(true));
    }
}

#[test]
fn disabling_with_a_host_attached_defers_restore_until_the_port_is_empty() {
    let mut policy = Draw::default();
    let mut calls = Vec::new();
    for (wanted, role) in [
        (true, Some("sink")),
        (false, Some("sink")),
        (false, Some("source")),
        (false, None),
    ] {
        policy
            .update(wanted, role, |action| {
                calls.push(action);
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(calls, [Action::Cut]);
    assert!(policy.cut);
    policy
        .update(false, Some("none"), |action| {
            calls.push(action);
            Ok(())
        })
        .unwrap();
    assert_eq!(calls, [Action::Cut, Action::Restore]);
    assert!(!policy.cut);
}

#[test]
fn a_restarted_owner_restores_an_older_owners_cut_once_when_off_and_empty() {
    let mut policy = Draw::default();
    let mut calls = 0;
    for _ in 0..1000 {
        policy
            .update(false, Some("none"), |action| {
                assert_eq!(action, Action::Restore);
                calls += 1;
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(calls, 1);
}

#[test]
fn repeated_steps_and_mode_changes_do_not_spawn_repeated_privileged_work() {
    let mut policy = Draw::default();
    let mut calls = 0;
    for i in 0..10000 {
        policy
            .update(
                true,
                Some(if i % 2 == 0 { "sink" } else { "source" }),
                |_| {
                    calls += 1;
                    Ok(())
                },
            )
            .unwrap();
    }
    assert_eq!(calls, 1);
}

#[test]
fn a_failed_cut_is_reported_once_and_not_retried_each_poll() {
    let mut policy = Draw::default();
    let mut calls = 0;
    assert!(policy
        .update(true, Some("sink"), |_| {
            calls += 1;
            Err(io::Error::other("voter unavailable"))
        })
        .is_err());
    for _ in 0..1000 {
        policy
            .update(true, Some("sink"), |_| {
                calls += 1;
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(calls, 1);
    assert!(!policy.cut);
    assert_eq!(policy.settled, Some(true));
}

#[test]
fn boot_matches_the_upstream_nonzero_setting_file_policy() {
    for value in [None, Some(&b""[..]), Some(&b"0\n"[..])] {
        assert!(!boot_wanted(value));
    }
    for value in [b"1".as_slice(), b"2", b"3\n", b"android"] {
        assert!(boot_wanted(Some(value)));
    }
}
