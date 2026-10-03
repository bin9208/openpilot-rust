use openpilot_can::{Frame, Packet};
use openpilot_card::{
    fingerprint::{Fingerprint, Legacy},
    toggle::{Button, MainToggle},
};
use std::collections::BTreeMap;

fn candidates() -> Vec<Legacy> {
    vec![
        Legacy {
            name: "one".into(),
            versions: vec![BTreeMap::from([(1, 8)])],
        },
        Legacy {
            name: "two".into(),
            versions: vec![BTreeMap::from([(2, 8)])],
        },
    ]
}

#[test]
fn can_fingerprint_counts_packets_and_keeps_initial_eight_bus_maps() {
    let mut fingerprint = Fingerprint::new(candidates());
    let packet = Packet {
        mono_time: 0,
        frames: vec![Frame {
            address: 1,
            data: vec![0; 8],
            bus: 0,
        }],
    };
    fingerprint.observe(std::slice::from_ref(&packet));
    assert!(!fingerprint.done);
    assert_eq!(fingerprint.observed.len(), 8);
    fingerprint.observe(&vec![packet; 101]);
    assert!(fingerprint.done);
    assert_eq!(fingerprint.selected.as_deref(), Some("one"));
    assert_eq!(fingerprint.frames, 102);
}

#[test]
fn a_received_batch_is_processed_even_after_the_first_success() {
    let mut fingerprint = Fingerprint::new(candidates());
    let packet = Packet {
        mono_time: 0,
        frames: vec![Frame {
            address: 1,
            data: vec![0; 8],
            bus: 0,
        }],
    };
    fingerprint.observe(&vec![packet; 300]);
    assert_eq!(fingerprint.frames, 300);
    assert_eq!(fingerprint.selected.as_deref(), Some("one"));
}

#[test]
fn main_toggle_holds_engaged_time_and_fires_once_after_disengagement() {
    let mut toggle = MainToggle::new(8);
    let pressed = Button {
        kind: 8,
        pressed: true,
    };
    assert!(!toggle.update(&[pressed], (true, 0.)));
    assert!(!toggle.update(&[], (true, 3.)));
    assert!(toggle.update(&[], (false, 3.1)));
    assert!(!toggle.update(&[], (false, 10.)));
    assert!(!toggle.update(
        &[Button {
            kind: 8,
            pressed: false
        }],
        (false, 10.)
    ));
}
