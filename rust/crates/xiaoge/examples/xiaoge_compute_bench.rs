//! Hosted-only, same-input comparison with the frozen deployed implementation.
use openpilot_xiaoge::lane::{select, Candidate, Head, ResultData, MASK_CHANNELS, PROTO_SIZE};
use openpilot_xiaoge::nv12::{Frame, Layout};
use openpilot_xiaoge::Error;
use serde_json::{json, Value};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

#[path = "../tests/support/lane_head_0682e933.rs"]
mod baseline;
#[path = "../tests/support/compute_cases.rs"]
mod inputs;

#[derive(serde::Serialize)]
struct Sample {
    wall_us: f64,
    process_cpu_us: Option<f64>,
}

#[cfg(feature = "native-skip-miri")]
fn cpu_now() -> Option<Duration> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    Some(Duration::new(
        time.tv_sec.try_into().unwrap(),
        time.tv_nsec.try_into().unwrap(),
    ))
}

#[cfg(not(feature = "native-skip-miri"))]
fn cpu_now() -> Option<Duration> {
    None
}

fn elapsed(iterations: usize, operation: &mut impl FnMut()) -> Sample {
    let start = Instant::now();
    let cpu = cpu_now();
    for _ in 0..iterations {
        operation();
    }
    let process_cpu_us =
        cpu.map(|start| (cpu_now().unwrap() - start).as_secs_f64() * 1e6 / iterations as f64);
    Sample {
        wall_us: start.elapsed().as_secs_f64() * 1e6 / iterations as f64,
        process_cpu_us,
    }
}

fn pair(name: &str, iterations: usize, mut before: impl FnMut(), mut after: impl FnMut()) -> Value {
    // Alternate order, warm both implementations, and retain every raw sample.
    before();
    after();
    let mut baseline_samples = Vec::new();
    let mut candidate_samples = Vec::new();
    for round in 0..9 {
        if round % 2 == 0 {
            baseline_samples.push(elapsed(iterations, &mut before));
            candidate_samples.push(elapsed(iterations, &mut after));
        } else {
            candidate_samples.push(elapsed(iterations, &mut after));
            baseline_samples.push(elapsed(iterations, &mut before));
        }
    }
    let median = |values: &[f64]| {
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        sorted[sorted.len() / 2]
    };
    let baseline_us: Vec<_> = baseline_samples
        .iter()
        .map(|sample| sample.wall_us)
        .collect();
    let candidate_us: Vec<_> = candidate_samples
        .iter()
        .map(|sample| sample.wall_us)
        .collect();
    let cpu_median = |samples: &[Sample]| {
        samples
            .iter()
            .map(|sample| sample.process_cpu_us)
            .collect::<Option<Vec<_>>>()
            .map(|values| median(&values))
    };
    json!({"name": name, "iterations_per_sample": iterations,
        "baseline_samples": baseline_samples, "candidate_samples": candidate_samples,
        "baseline_median_us": median(&baseline_us),
        "candidate_median_us": median(&candidate_us),
        "baseline_process_cpu_median_us": cpu_median(&baseline_samples),
        "candidate_process_cpu_median_us": cpu_median(&candidate_samples),
        "speedup": median(&baseline_us) / median(&candidate_us)})
}

fn packed_crop(frame: &Frame<'_>, [x, y, width, height]: [usize; 4]) -> Vec<u8> {
    let packed = frame.pack().unwrap();
    let layout = frame.layout();
    let mut output = Vec::with_capacity(width * (height + height / 2));
    for row in y..y + height {
        let offset = row * layout.width + x;
        output.extend_from_slice(&packed[offset..offset + width]);
    }
    for row in layout.height + y / 2..layout.height + (y + height) / 2 {
        let offset = row * layout.width + x;
        output.extend_from_slice(&packed[offset..offset + width]);
    }
    output
}

#[cfg(feature = "native-skip-miri")]
use openpilot_xiaoge::{config, nv12, vision};

// The frozen lane inference source must use the frozen head, not the candidate.
#[cfg(feature = "native-skip-miri")]
mod lane {
    pub use super::baseline::Head;
    pub use openpilot_xiaoge::lane::ResultData;
}

#[cfg(feature = "native-skip-miri")]
mod original {
    use openpilot_xiaoge::inference::Error;
    pub mod geometry {
        include!("../tests/support/geometry_0682e933.rs");
    }
    pub mod lane {
        include!("../tests/support/lane_inference_0682e933.rs");
    }
}

#[cfg(feature = "native-skip-miri")]
fn native(frame: &Frame<'_>, rows: &mut Vec<Value>) -> Result<(), Box<dyn std::error::Error>> {
    use openpilot_opencv_runtime::{self as cv, Dimensions};
    use openpilot_xiaoge::{
        config::Config,
        inference::{lane_image, lane_tensor, Geometry, LaneModel},
        vision::Side,
    };
    use std::path::Path;

    cv::initialize(2)?;
    let image = lane_image(frame)?;
    let old = original::lane::lane_tensor(&image)?;
    let new = lane_tensor(&image)?;
    assert_eq!(old.view().values(), new.view().values());
    rows.push(pair(
        "lane-normalization",
        32,
        || {
            black_box(original::lane::lane_tensor(black_box(&image)).unwrap());
        },
        || {
            black_box(lane_tensor(black_box(&image)).unwrap());
        },
    ));

    let config = Config::normalize(&json!({"width":1928,"height":1208,
        "poly_left":[[0,550],[550,480],[650,950],[0,1200]],
        "poly_right":[[1378,480],[1927,550],[1927,1200],[1278,950]]}))?;
    let size = Dimensions::new(1928, 1208)?;
    let before = original::geometry::Geometry::new(&config, size)?;
    let after = Geometry::new(&config, size)?;
    assert_eq!(before.dimensions(), after.dimensions());
    for side in [Side::Left, Side::Right] {
        let old = before.region(side).unwrap();
        let new = after.region(side).unwrap();
        assert_eq!(old.bounds(), new.bounds());
        assert_eq!(old.mask().view().data(), new.mask().view().data());
        assert_eq!(
            before.tensor(frame, side)?.unwrap().view().values(),
            after.tensor(frame, side)?.unwrap().view().values()
        );
        rows.push(pair(
            &format!("bsd-preprocess-{}", side.name()),
            4,
            || {
                black_box(before.tensor(black_box(frame), side).unwrap());
            },
            || {
                black_box(after.tensor(black_box(frame), side).unwrap());
            },
        ));
    }

    let models = std::env::var("XIAOGE_BENCH_MODELS")?;
    let path = Path::new(&models).join("lane.onnx");
    let mut before = original::lane::LaneModel::load(&path);
    let mut after = LaneModel::load(&path);
    assert!(before.loaded(), "{}", before.error());
    assert!(after.loaded(), "{}", after.error());
    let expected = before.infer(frame, 0.25);
    let actual = after.infer(frame, 0.25);
    assert!(expected.valid && actual.valid);
    assert_eq!(
        serde_json::to_value(expected)?,
        serde_json::to_value(actual)?
    );
    rows.push(pair(
        "lane-complete-inference",
        2,
        || {
            black_box(before.infer(black_box(frame), 0.25));
        },
        || {
            black_box(after.infer(black_box(frame), 0.25));
        },
    ));
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    assert_eq!(
        std::env::var("RUNNER_ENVIRONMENT").as_deref(),
        Ok("github-hosted")
    );
    let mut rows = Vec::new();
    for case in inputs::cases() {
        let before = baseline::Head::new(&case.predictions, &case.prototypes)?;
        let after = Head::new(&case.predictions, &case.prototypes)?;
        assert_eq!(
            serde_json::to_value(before.result(0.25, 0.5)?)?,
            serde_json::to_value(after.result(0.25, 0.5)?)?
        );
        rows.push(pair(
            &format!("mask-{}", case.name),
            16,
            || {
                black_box(before.result(black_box(0.25), 0.5).unwrap());
            },
            || {
                black_box(after.result(black_box(0.25), 0.5).unwrap());
            },
        ));
    }
    let layout = Layout {
        width: 1928,
        height: 1208,
        stride: 1984,
        uv_offset: 1984 * 1208 + 1024,
    };
    let data: Vec<u8> = (0..layout.uv_offset + layout.stride * layout.height / 2)
        .map(|index| (index.wrapping_mul(73) % 251) as u8)
        .collect();
    let frame = Frame::new(&data, layout)?;
    let bounds = [0, 480, 652, 720];
    assert_eq!(packed_crop(&frame, bounds), frame.crop_nv12(bounds)?);
    rows.push(pair(
        "nv12-roi-copy",
        64,
        || {
            black_box(packed_crop(black_box(&frame), bounds));
        },
        || {
            black_box(frame.crop_nv12(black_box(bounds)).unwrap());
        },
    ));
    #[cfg(feature = "native-skip-miri")]
    native(&frame, &mut rows)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "source_commit": std::env::var("GITHUB_SHA")?,
            "baseline_source": "0682e933d3fea06e8953ace63fc521a29ee02b6e",
            "architecture": std::env::consts::ARCH,
            "native_models": cfg!(feature = "native-skip-miri"),
            "nv12_full_frame_bytes_avoided_per_bsd_inference": layout.width * (layout.height + layout.height/2),
            "lane_normalizations_before": 3 * 416 * 416,
            "lane_normalizations_after": 416 * 416,
            "scope":"Same hosted runner and inputs, alternating order; stage timings are not device CPU savings or GPU utilization.",
            "measurements": rows
        }))?
    );
    Ok(())
}
