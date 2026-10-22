//! Actions-only, same-input comparison with the frozen deployed implementation.
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

#[cfg(feature = "native-skip-miri")]
#[path = "../tests/support/preprocessing_dc951380.rs"]
mod preprocessing_baseline;

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
    use openpilot_opencv_runtime::{self as cv, Dimensions, DnnNet};
    use openpilot_xiaoge::{
        config::Config,
        inference::{lane_image, lane_tensor, Geometry, LaneModel},
        vision::Side,
    };
    use std::path::Path;

    cv::initialize(2)?;
    let image = lane_image(frame)?;
    let previous_image = preprocessing_baseline::lane_image(frame)?;
    assert_eq!(image.view().data(), previous_image.view().data());
    let mut measurement = pair(
        "lane-image-strided-roi",
        32,
        || {
            black_box(preprocessing_baseline::lane_image(black_box(frame)).unwrap());
        },
        || {
            black_box(lane_image(black_box(frame)).unwrap());
        },
    );
    let side = frame.layout().width.min(frame.layout().height);
    measurement["baseline_source"] = json!("dc95138076836ea3a5c1b6c27262be3f3b912f71");
    measurement["copied_bytes_avoided_per_inference"] =
        json!(if side == 416 { 0 } else { side * side });
    measurement["rust_image_allocations_avoided_per_inference"] = json!(usize::from(side != 416));
    measurement["avoided_work_basis"] =
        json!("source-counted center-square packing; not measured memory traffic");
    rows.push(measurement);
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
    let interop_before = preprocessing_baseline::geometry::Geometry::new(&config, size)?;
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
        assert_eq!(interop_before.dimensions(), after.dimensions());
        let baseline_region = interop_before.region(side).unwrap();
        assert_eq!(baseline_region.bounds(), new.bounds());
        assert_eq!(
            baseline_region.mask().view().data(),
            new.mask().view().data()
        );
        let baseline = interop_before.tensor(frame, side)?.unwrap();
        let candidate = after.tensor(frame, side)?.unwrap();
        assert_eq!(baseline.view().shape(), candidate.view().shape());
        assert!(baseline
            .view()
            .values()
            .iter()
            .zip(candidate.view().values())
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        let mut measurement = pair(
            &format!("bsd-preprocess-interop-{}", side.name()),
            4,
            || {
                black_box(interop_before.tensor(black_box(frame), side).unwrap());
            },
            || {
                black_box(after.tensor(black_box(frame), side).unwrap());
            },
        );
        let [_, _, width, height] = new.bounds();
        measurement["baseline_source"] = json!("dc95138076836ea3a5c1b6c27262be3f3b912f71");
        measurement["copied_bytes_avoided_per_inference"] = json!(6 * width * height);
        measurement["rust_image_allocations_avoided_per_inference"] = json!(2);
        measurement["avoided_work_basis"] =
            json!("source-counted RGB and masked-RGB return copies; not measured memory traffic");
        rows.push(measurement);
    }

    let models = std::env::var("XIAOGE_BENCH_MODELS")?;
    let path = Path::new(&models).join("lane.onnx");
    let input = lane_tensor(&image)?;
    let mut forward_before = DnnNet::load(&path)?;
    let mut forward_after = DnnNet::load(&path)?;
    let names = forward_before.output_names()?;
    let outputs = forward_before.forward(input.view(), &names)?;
    let predictions = outputs[names.iter().position(|name| name == "output0").unwrap()].view();
    let prototypes = outputs[names.iter().position(|name| name == "output1").unwrap()].view();
    assert!(matches!(predictions.shape(), [1, 42, _]));
    assert_eq!(prototypes.shape(), [1, 32, 104, 104]);
    let old_head = baseline::Head::new(predictions.values(), prototypes.values())?;
    let new_head = Head::new(predictions.values(), prototypes.values())?;
    assert_eq!(
        serde_json::to_value(old_head.result(0.25, 0.5)?)?,
        serde_json::to_value(new_head.result(0.25, 0.5)?)?
    );
    rows.push(pair(
        "lane-real-output-head",
        32,
        || {
            black_box(old_head.result(black_box(0.25), 0.5).unwrap());
        },
        || {
            black_box(new_head.result(black_box(0.25), 0.5).unwrap());
        },
    ));
    // An unchanged-forward control separates model/backend cost and timing noise
    // from preprocessing/head savings. No speedup is expected for this row.
    rows.push(pair(
        "lane-unchanged-dnn-forward-control",
        2,
        || {
            black_box(forward_before.forward(input.view(), &names).unwrap());
        },
        || {
            black_box(forward_after.forward(input.view(), &names).unwrap());
        },
    ));
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
        std::env::var("GITHUB_REPOSITORY").as_deref(),
        Ok("bin9208/openpilot-rust")
    );
    let runner_environment = std::env::var("RUNNER_ENVIRONMENT").unwrap_or_default();
    let runner_name = std::env::var("RUNNER_NAME").unwrap_or_default();
    assert!(
        runner_environment == "github-hosted"
            || runner_environment == "self-hosted" && runner_name == "bin-actions"
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
            "preprocessing_baseline_provenance": serde_json::from_str::<Value>(include_str!("../tests/support/preprocessing_dc951380.json"))?,
            "architecture": std::env::consts::ARCH,
            "native_models": cfg!(feature = "native-skip-miri"),
            "wait_policy": std::env::var("XIAOGE_BENCH_POLICY").unwrap_or_else(|_| "default".to_owned()),
            "nv12_full_frame_bytes_avoided_per_bsd_inference": layout.width * (layout.height + layout.height/2),
            "lane_normalizations_before": 3 * 416 * 416,
            "lane_normalizations_after": 416 * 416,
            "runner_environment":runner_environment,
            "runner_name":runner_name,
            "architecture":std::env::consts::ARCH,
            "scope":"Same Actions runner and inputs, alternating order; stage timings are not device CPU savings or GPU utilization.",
            "measurements": rows
        }))?
    );
    Ok(())
}
