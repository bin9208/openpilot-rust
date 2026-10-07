use openpilot_opencv_runtime::{self as cv, Dimensions};
use openpilot_xiaoge::{
    config::Config,
    inference::{lane_image, lane_tensor, BlindspotModel, Geometry, LaneModel},
    nv12::{Frame, Layout},
    settings::Settings,
    vision::Side,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct Input {
    frame: PathBuf,
    width: usize,
    height: usize,
    stride: usize,
    uv_offset: usize,
    config: serde_json::Value,
    models: PathBuf,
    output: PathBuf,
}

fn tensor(path: &Path, values: &[f32]) -> std::io::Result<()> {
    let mut output = fs::File::create(path)?;
    for value in values {
        output.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

fn evaluate(input: Input) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    fs::create_dir(&input.output)?;
    let bytes = fs::read(input.frame)?;
    let frame = Frame::new(
        &bytes,
        Layout {
            width: input.width,
            height: input.height,
            stride: input.stride,
            uv_offset: input.uv_offset,
        },
    )?;
    let config = Config::normalize(&input.config)?;
    let geometry = Geometry::new(
        &config,
        Dimensions::new(u32::try_from(input.width)?, u32::try_from(input.height)?)?,
    )?;
    let gray = lane_image(&frame)?;
    fs::write(input.output.join("lane-gray.bin"), gray.view().data())?;
    tensor(
        &input.output.join("lane-blob.f32"),
        lane_tensor(&gray)?.view().values(),
    )?;
    let mut lane = LaneModel::load(&input.models.join("lane.onnx"));
    let lane_result = lane.infer(&frame, 0.25);
    let mut model = BlindspotModel::load(&input.models.join("v_asm_model.onnx"), config);
    let mut sides = Vec::new();
    for side in [Side::Left, Side::Right] {
        if let Some(region) = geometry.region(side) {
            fs::write(
                input.output.join(format!("{}-mask.bin", side.name())),
                region.mask().view().data(),
            )?;
            let input_tensor = geometry
                .tensor(&frame, side)?
                .ok_or("configured side missing tensor")?;
            tensor(
                &input.output.join(format!("{}-blob.f32", side.name())),
                input_tensor.view().values(),
            )?;
            model.update(&frame, side, &Settings::default(), 0.25)?;
            sides.push(json!({"side": side.name(), "bounds": region.bounds(), "detection": model.side(side)}));
        }
    }
    Ok(
        json!({"lane": lane_result, "sides": sides, "lane_loaded": lane.loaded(), "blindspot_loaded": model.loaded()}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    cv::initialize(2)?;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let inputs: Vec<Input> = serde_json::from_str(&input)?;
    let results = inputs
        .into_iter()
        .map(evaluate)
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_writer(std::io::stdout().lock(), &results)?;
    std::io::stdout().flush()?;
    Ok(())
}
