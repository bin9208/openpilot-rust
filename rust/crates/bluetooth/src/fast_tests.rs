//! carrot's `bluetooth/tests/test_fast_blinker.py`.
use crate::{
    types::Gestures, Action, CommandWriter, Config, Decoder, Event, Intent, Mapping, Profile,
    Seconds, Token,
};

const OFF: Option<Gestures> = Some(Gestures {
    double: false,
    long: false,
});

fn mapping(entries: &[(&str, Action)]) -> Mapping {
    Mapping(
        entries
            .iter()
            .map(|(k, a)| (Token((*k).to_owned()), *a))
            .collect(),
    )
}

fn feed(d: &mut Decoder, kind: u16, code: u16, value: i32, at: f64) -> Vec<String> {
    d.feed(Event {
        kind,
        code,
        value,
        at: Seconds(at),
    })
    .into_iter()
    .map(|t| t.0)
    .collect()
}

fn frame(d: &mut Decoder, events: &[(u16, u16, i32)], at: f64) -> Vec<String> {
    let mut result = Vec::new();
    for &(kind, code, value) in events {
        result.extend(feed(d, kind, code, value, at));
    }
    result.extend(feed(d, 0, 0, 0, at));
    result
}

fn flush(d: &mut Decoder, at: f64) -> Vec<String> {
    d.flush(Seconds(at)).into_iter().map(|t| t.0).collect()
}

#[test]
fn fast_key_fires_on_press_not_release_repeat_or_long_timer() {
    let m = mapping(&[
        ("key:30", Action::BlinkerLeft),
        ("key:30@long", Action::BlinkerRight),
        ("key:30@double", Action::LaneLeft),
    ]);
    let mut d = Decoder::new(Profile::Generic, m, false, OFF);
    assert_eq!(frame(&mut d, &[(1, 30, 1)], 1.), ["key:30"]);
    assert!(frame(&mut d, &[(1, 30, 2)], 1.8).is_empty());
    assert!(flush(&mut d, 1.8).is_empty());
    assert!(frame(&mut d, &[(1, 30, 0)], 2.).is_empty());
    assert_eq!(frame(&mut d, &[(1, 30, 1)], 2.1), ["key:30"]);
}

#[test]
fn enabled_double_waits_and_does_not_fire_single_on_press() {
    let m = mapping(&[
        ("key:30", Action::BlinkerLeft),
        ("key:30@double", Action::BlinkerRight),
    ]);
    let g = Some(Gestures {
        double: true,
        long: false,
    });
    let mut d = Decoder::new(Profile::Generic, m, false, g);
    for stamp in [1., 1.2] {
        assert!(frame(&mut d, &[(1, 30, 1)], stamp).is_empty());
        let released = frame(&mut d, &[(1, 30, 0)], stamp + 0.05);
        if stamp == 1. {
            assert!(released.is_empty());
        } else {
            assert_eq!(released, ["key:30@double"]);
        }
    }
    assert!(flush(&mut d, 2.).is_empty());
}

#[test]
fn enabled_long_still_fires_once_after_threshold() {
    let m = mapping(&[
        ("key:30", Action::BlinkerLeft),
        ("key:30@long", Action::BlinkerRight),
    ]);
    let g = Some(Gestures {
        double: false,
        long: true,
    });
    let mut d = Decoder::new(Profile::Generic, m, false, g);
    assert!(frame(&mut d, &[(1, 30, 1)], 1.).is_empty());
    assert!(flush(&mut d, 1.69).is_empty());
    assert_eq!(flush(&mut d, 1.71), ["key:30@long"]);
    assert!(flush(&mut d, 2.5).is_empty());
    assert!(frame(&mut d, &[(1, 30, 0)], 2.6).is_empty());
}

#[test]
fn fast_yiser_swipe_precedes_release_and_never_becomes_second_action() {
    let m = mapping(&[("left", Action::BlinkerLeft)]);
    let mut d = Decoder::new(Profile::YiserJ6, m, false, OFF);
    assert!(frame(&mut d, &[(3, 0, 300), (3, 1, 500), (1, 330, 1)], 1.).is_empty());
    assert_eq!(frame(&mut d, &[(3, 0, 450)], 1.05), ["left"]);
    assert!(frame(&mut d, &[(3, 0, 500)], 1.1).is_empty());
    assert!(frame(&mut d, &[(3, 0, 100)], 1.2).is_empty());
    assert!(frame(&mut d, &[(1, 330, 0)], 1.3).is_empty());
}

#[test]
fn dropped_frame_and_learning_do_not_fire_on_press() {
    for learning in [false, true] {
        let mut d = Decoder::new(Profile::Generic, Mapping::default(), learning, OFF);
        if !learning {
            feed(&mut d, 0, 3, 0, 0.);
        }
        assert!(frame(&mut d, &[(1, 30, 1)], 1.).is_empty());
    }
}

#[test]
fn held_key_at_connect_or_mapping_reload_must_be_released_first() {
    let mut d = Decoder::new(Profile::Generic, Mapping::default(), false, OFF);
    d.synchronize_pressed(&[30].into_iter().collect(), Seconds(1.));
    assert!(frame(&mut d, &[(1, 30, 1)], 1.1).is_empty());
    assert!(frame(&mut d, &[(1, 30, 2)], 1.2).is_empty());
    assert!(frame(&mut d, &[(1, 30, 0)], 1.3).is_empty());
    assert_eq!(frame(&mut d, &[(1, 30, 1)], 1.4), ["key:30"]);
}

#[test]
fn gesture_options_preserve_legacy_mappings_and_can_disable_them() {
    let mac = "00:11:22:33:44:55";
    let device = |gestures: &str| {
        format!(
            r#"{{"devices":{{"{mac}":{{"profile":"generic","enabled":true,"mapping":{{"key:30":"blinkerLeft","key:30@long":"blinkerRight"}}{gestures}}}}}}}"#
        )
    };
    let saved = Config::parse(&device("")).unwrap();
    let saved = saved.devices.values().next().unwrap();
    assert_eq!(
        saved.gestures,
        Gestures {
            double: false,
            long: true
        }
    );
    let off = Config::parse(&device(r#","gestures":{"double":false,"long":false}"#)).unwrap();
    let off = off.devices.values().next().unwrap();
    assert!(off.mapping == saved.mapping);
    assert_eq!(off.gestures, Gestures::default());
    assert!(Config::parse(&device(r#","gestures":{"double":"false","long":false}"#)).is_err());
    assert!(Config::parse(&device(r#","gestures":{"double":false}"#)).is_err());
}

#[test]
fn physical_indicators_have_their_own_journal() {
    let dir = std::env::temp_dir().join(format!("bt-blinker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut writer = CommandWriter::new(&dir);
    let address = crate::Address::parse("00:11:22:33:44:55").unwrap();
    for action in [Action::BlinkerLeft, Action::BlinkerRight] {
        writer
            .send(Intent {
                address: address.clone(),
                action,
                at: Seconds(11.),
                hold: None,
                repeat: false,
            })
            .unwrap();
    }
    let text = std::fs::read_to_string(dir.join("blinker.json")).unwrap();
    assert!(text.contains("blinkerLeft") && text.contains("blinkerRight"));
    for other in ["lane.json", "cruise.json"] {
        let empty =
            std::fs::read_to_string(dir.join(other)).map_or(true, |t| !t.contains("blinker"));
        assert!(empty, "{other}");
    }
    std::fs::remove_dir_all(&dir).ok();
}
