//! Independent original generator yield/resume and exception phase captures.
#![cfg(feature = "image-array-native")]
use openpilot_dev_tools::ui_application_lifecycle_loop::{
    Backend, Config, Event, Loop, LoopError, Outcome,
};
use serde_json::{json, Value};
struct Peer<'a> {
    name: &'a str,
    calls: Vec<Value>,
}
impl Backend for Peer<'_> {
    fn should_close(&mut self, frame: usize) -> Result<bool, LoopError> {
        self.calls.push(json!(["window-should-close"]));
        Ok(self.name == "window-close" && frame >= 2)
    }
    fn now(&self) -> f64 {
        1.0
    }
    fn recording(&self) -> Option<(f64, f64)> {
        matches!(
            self.name,
            "dynamic-record" | "queue-full" | "record-rollover"
        )
        .then_some((
            0.0,
            if self.name == "record-rollover" {
                0.0
            } else {
                60.0
            },
        ))
    }
    fn event(&mut self, event: Event) -> Result<(), LoopError> {
        let value = match event {
            Event::MousePoll => json!(["mouse-poll"]),
            Event::MouseEvents => json!(["mouse-events"]),
            Event::PollInput => json!(["poll-input"]),
            Event::Sleep(seconds) => json!(["sleep", seconds]),
            Event::Begin => json!(["begin"]),
            Event::TargetBegin => json!(["target-begin"]),
            Event::TargetEnd => json!(["target-end"]),
            Event::Clear => json!(["clear"]),
            Event::MatrixPush => json!(["matrix-push"]),
            Event::MatrixScale(scale) => json!(["matrix-scale", scale, scale, 1.0]),
            Event::MatrixPop => json!(["matrix-pop"]),
            Event::Ticks => json!(["tick"]),
            Event::Widgets { width, height } => json!(["widget", [0.0, 0.0, width, height]]),
            Event::ShaderBegin => json!(["shader-begin", "shader"]),
            Event::ShaderEnd => json!(["shader-end"]),
            Event::TargetDraw { width, height } => json!([
                "target-draw",
                [0.0, 0.0, width, -height],
                [0.0, 0.0, width, height]
            ]),
            Event::ShowFps => json!(["fps-overlay", 10, 10]),
            Event::ShowTouches => json!(["touch-overlay"]),
            Event::Grid => json!(["grid-overlay"]),
            Event::End => json!(["end"]),
            Event::RecordReadback => json!(["record-readback", "target"]),
            Event::RecordSubmit => {
                json!(["record-submit",{"bytes":8,"sha256":"924592b9b103f14f833faafb67f480691f01988aa457c0061769f58cd47311bc"}])
            }
            Event::RecordUnload => json!(["record-unload"]),
            Event::StopRecording => json!(["stop-recording"]),
            Event::StartRecording => json!(["start-recording"]),
            Event::MonitorFps => json!(["monitor-fps"]),
            Event::ProfileOutput(frame) => json!(["profile-output", frame]),
        };
        self.calls.push(value.clone());
        if value[0] == "tick" && self.name == "tick-error" {
            return Err(LoopError::Failure("tick rejected".into()));
        }
        if value[0] == "widget" && self.name == "widget-error" {
            return Err(LoopError::Failure("widget rejected".into()));
        }
        if value[0] == "record-submit" && self.name == "queue-full" {
            return Err(LoopError::QueueFull);
        }
        Ok(())
    }
}
#[test]
fn unchanged_original_generator_all_fifteen_phase_histories_and_failures() {
    let captured: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/render-loop-original.json"
    ))
    .unwrap();
    for case in captured["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut peer = Peer {
            name,
            calls: vec![],
        };
        let config = Config {
            profile_frames: (if matches!(
                name,
                "paused" | "ordinary-close" | "window-close" | "close-before" | "consumer-close"
            ) {
                0
            } else {
                4
            })
            .into(),
            texture: matches!(
                name,
                "global-record" | "dynamic-record" | "burn-in" | "record-rollover" | "queue-full"
            ),
            global_record: name == "global-record",
            scale: if name == "scale" { 2.0 } else { 1.0 },
            burn_in: name == "burn-in",
            show_fps: name == "overlays",
            show_touches: name == "overlays",
            grid: name == "overlays",
            ..Config::default()
        };
        let mut rendering = Loop::new(config);
        rendering.set_should_render(name != "paused");
        if name == "close-before" {
            rendering.request_close();
        }
        let mut skipped = 0;
        let mut error = Value::Null;
        for _ in 0..12 {
            match rendering.resume(&mut peer) {
                Ok(Outcome::Stopped) => break,
                Ok(Outcome::ProfileComplete) => {
                    error = json!({"type":"SystemExit","message":"0"});
                    break;
                }
                Ok(Outcome::Yield(rendered)) => {
                    peer.calls
                        .push(json!(["yield", rendered, rendering.frame()]));
                    if !rendered {
                        skipped += 1;
                        if skipped >= 2 {
                            rendering.set_should_render(true);
                        }
                    } else {
                        peer.calls
                            .push(json!(["consumer-capture", rendering.frame()]));
                        if name == "consumer-close" {
                            peer.calls.push(json!(["consumer-close"]));
                            break;
                        }
                        if (name == "paused" && rendering.frame() >= 3)
                            || (name == "ordinary-close" && rendering.frame() >= 1)
                        {
                            rendering.request_close();
                        }
                    }
                }
                Err(LoopError::Failure(message)) => {
                    error = json!({"type":"ValueError","message":message});
                    break;
                }
                Err(other) => panic!("{name}: unexpected {other:?}"),
            }
        }
        assert_eq!(peer.calls, *case["calls"].as_array().unwrap(), "{name}");
        assert_eq!(
            rendering.frame(),
            case["frame"].as_u64().unwrap() as usize,
            "{name}"
        );
        assert_eq!(
            rendering.record_frame_index(),
            case["record_frame_idx"].as_u64().unwrap() as usize,
            "{name}"
        );
        assert_eq!(
            skipped,
            case["skipped"].as_u64().unwrap() as usize,
            "{name}"
        );
        assert_eq!(error, case["error"], "{name}");
    }
}

#[test]
fn unchanged_original_mouse_slots_edges_scaling_and_unchanged_event_suppression() {
    use openpilot_dev_tools::ui_application_lifecycle_loop::Mouse;
    let captured: Value = serde_json::from_str(include_str!(
        "fixtures/ui-application-lifecycle/mouse-original.json"
    ))
    .unwrap();
    for case in captured["cases"].as_array().unwrap() {
        let mut mouse = Mouse::new(case["scale"].as_f64().unwrap());
        for (index, frame) in case["frames"].as_array().unwrap().iter().enumerate() {
            let x = frame[2].as_f64().unwrap();
            let y = frame[3].as_f64().unwrap();
            let events = mouse.poll(
                [frame[0].as_bool().unwrap(), frame[1].as_bool().unwrap()],
                [(x, y), (x + 1.0, y + 1.0)],
                index as f64 / 20.0,
            );
            assert_eq!(
                serde_json::to_value(events).unwrap(),
                case["events"][index],
                "{} frame{index}",
                case["name"]
            );
        }
    }
}
