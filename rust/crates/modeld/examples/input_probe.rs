use openpilot_modeld::{
    calibration::{CalibrationUpdate, DrivingCalibration},
    camera::{receive_pair, select_streams, CameraSource, Captured, FrameMeta},
    inputs::{DropTracker, PolicyInputs},
};
use serde::Deserialize;
use serde_json::json;
use std::{collections::VecDeque, convert::Infallible, error::Error, fs, path::PathBuf};

#[derive(Deserialize)]
struct Request {
    feature_count: usize,
    pairs: Vec<PairRequest>,
    streams: Vec<[bool; 3]>,
    frames: Vec<InputFrame>,
    calibration: Vec<CalibFrame>,
}

#[derive(Deserialize)]
struct PairRequest {
    main: Vec<Option<FrameMeta>>,
    extra: Option<Vec<Option<FrameMeta>>>,
    calls: usize,
}

#[derive(Deserialize)]
struct InputFrame {
    frame_id: u32,
    desire: i32,
    is_rhd: bool,
    lateral_time: f64,
    longitudinal_time: f64,
    features: Option<Vec<f32>>,
    reset: bool,
}

#[derive(Deserialize)]
struct CalibFrame {
    updated: bool,
    road_seen: bool,
    device_seen: bool,
    rpy_bits: [u32; 3],
    calibrated: bool,
    yaw_trim_degrees: f64,
    device: String,
    sensor: String,
    main_wide: bool,
    use_extra: bool,
}

struct Camera {
    frames: VecDeque<Option<FrameMeta>>,
    count: usize,
}

impl CameraSource for Camera {
    type Buffer = ();
    type Error = Infallible;
    fn receive(&mut self) -> Result<Option<Captured<()>>, Infallible> {
        self.count += 1;
        Ok(self.frames.pop_front().flatten().map(|metadata| Captured {
            metadata,
            buffer: (),
        }))
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = PathBuf::from(args.next().ok_or("expected request.json")?);
    let output = PathBuf::from(args.next().ok_or("expected output.json")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let request: Request = serde_json::from_slice(&fs::read(input)?)?;
    let mut pairs = Vec::new();
    for value in request.pairs {
        let mut main = Camera {
            frames: value.main.into(),
            count: 0,
        };
        let mut extra = value.extra.map(|frames| Camera {
            frames: frames.into(),
            count: 0,
        });
        let mut selected = Vec::new();
        for _ in 0..value.calls {
            let pair = receive_pair(&mut main, extra.as_mut())?;
            selected.push(pair.map(|pair| [pair.main().metadata, pair.extra().metadata]));
        }
        pairs.push(json!({"selected":selected, "main_receives":main.count, "extra_receives":extra.map_or(0, |camera|camera.count)}));
    }
    let streams: Vec<_> = request
        .streams
        .into_iter()
        .map(|[road, wide, use_wide]| select_streams(road, wide, use_wide))
        .collect();
    let mut drops = DropTracker::default();
    let mut policy = PolicyInputs::new(request.feature_count)?;
    let mut frames = Vec::new();
    for frame in request.frames {
        if frame.reset {
            drops.reset_warmup();
        }
        let drop = drops.observe(frame.frame_id);
        policy.update(
            frame.desire,
            frame.is_rhd,
            frame.lateral_time,
            frame.longitudinal_time,
        );
        let packed: Vec<_> = policy
            .packed()
            .iter()
            .map(|value| value.to_bits())
            .collect();
        if let Some(features) = frame.features {
            policy.set_features(&features)?;
        }
        frames.push(json!({"drop":drop, "packed_bits":packed}));
    }
    let mut state = DrivingCalibration::default();
    let mut calibration = Vec::new();
    for frame in request.calibration {
        let result = state.update(CalibrationUpdate {
            updated: frame.updated,
            road_seen: frame.road_seen,
            device_seen: frame.device_seen,
            rpy: frame.rpy_bits.map(f32::from_bits),
            calibrated: frame.calibrated,
            yaw_trim_degrees: frame.yaw_trim_degrees,
            device: &frame.device,
            sensor: &frame.sensor,
            main_wide: frame.main_wide,
            use_extra: frame.use_extra,
        });
        calibration.push(json!({"updated":result.ok(), "seen":state.seen(),
                               "main_bits":state.main().map(f32::to_bits), "extra_bits":state.extra().map(f32::to_bits)}));
    }
    fs::write(
        output,
        serde_json::to_vec(
            &json!({"pairs":pairs,"streams":streams,"frames":frames,"calibration":calibration}),
        )?,
    )?;
    Ok(())
}
