//! Hosted-only, same-input comparison with the frozen deployed implementation.
use openpilot_xiaoge::lane::{select, Candidate, Head, ResultData, MASK_CHANNELS, PROTO_SIZE};
use openpilot_xiaoge::nv12::{Frame, Layout};
use openpilot_xiaoge::Error;
use serde_json::{json, Value};
use std::{hint::black_box, time::Instant};

#[path = "../tests/support/lane_head_0682e933.rs"]
mod baseline;
#[path = "../tests/support/compute_cases.rs"]
mod inputs;

fn elapsed(iterations: usize, operation: &mut impl FnMut()) -> f64 {
    let start = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    start.elapsed().as_secs_f64() * 1e6 / iterations as f64
}

fn pair(name: &str, iterations: usize, mut before: impl FnMut(), mut after: impl FnMut()) -> Value {
    // Alternate order, warm both implementations, and retain every raw sample.
    before();
    after();
    let mut baseline_us = Vec::new();
    let mut candidate_us = Vec::new();
    for round in 0..9 {
        if round % 2 == 0 {
            baseline_us.push(elapsed(iterations, &mut before));
            candidate_us.push(elapsed(iterations, &mut after));
        } else {
            candidate_us.push(elapsed(iterations, &mut after));
            baseline_us.push(elapsed(iterations, &mut before));
        }
    }
    let median = |values: &[f64]| {
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        sorted[sorted.len() / 2]
    };
    json!({"name": name, "iterations_per_sample": iterations,
        "baseline_us": baseline_us, "candidate_us": candidate_us,
        "baseline_median_us": median(&baseline_us),
        "candidate_median_us": median(&candidate_us),
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
