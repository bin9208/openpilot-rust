use super::{session::Session, Error};
use crate::compute::ComputeMode;
use crate::{
    lane::{Head, ResultData},
    nv12::Frame,
};
use num_traits::ToPrimitive;
use openpilot_opencv_runtime::{
    self as cv, Dimensions, Format, Image, ImageLayout, Tensor, TensorView,
};
use std::path::Path;

pub fn lane_image(frame: &Frame<'_>) -> Result<Image, Error> {
    let layout = frame.layout();
    let size = u32::try_from(layout.width.min(layout.height))
        .map_err(|_| Error::Contract("lane image dimension overflow"))?;
    let image = Image::from_parts(
        ImageLayout::new(Dimensions::new(size, size)?, Format::Gray)?,
        frame.center_square()?,
    )?;
    if size == 416 {
        return Ok(image);
    }
    Ok(cv::resize_linear(image.view(), Dimensions::new(416, 416)?)?)
}

pub fn lane_tensor(image: &Image) -> Result<Tensor, Error> {
    let view = image.view();
    if view.layout().format() != Format::Gray
        || view.layout().dimensions() != Dimensions::new(416, 416)?
    {
        return Err(Error::Contract(
            "lane tensor requires 416x416 grayscale image",
        ));
    }
    let mut values = Vec::with_capacity(3 * 416 * 416);
    values.extend(view.data().iter().map(|&byte| f32::from(byte) / 255.0));
    // Every channel contains the same gray plane. Reuse its exact division bits.
    values.extend_from_within(..416 * 416);
    values.extend_from_within(..416 * 416);
    Ok(Tensor::from_parts(vec![1, 3, 416, 416], values)?)
}

pub struct LaneModel {
    net: Option<Session>,
    error: String,
}

impl LaneModel {
    pub fn load(path: &Path) -> Self {
        if !path.is_file() {
            return Self {
                net: None,
                error: format!("Model file not found: {}", path.display()),
            };
        }
        match Session::load(path) {
            Ok(net) => Self {
                net: Some(net),
                error: String::new(),
            },
            Err(error) => Self {
                net: None,
                error: error.to_string(),
            },
        }
    }

    pub fn loaded(&self) -> bool {
        self.net.as_ref().is_some_and(Session::loaded)
    }
    pub fn error(&self) -> &str {
        self.net
            .as_ref()
            .filter(|net| !net.loaded())
            .map_or(&self.error, Session::error)
    }

    pub fn set_compute_mode(&mut self, mode: ComputeMode, generation: u64) {
        if let Some(net) = &mut self.net {
            net.set_mode(mode, generation);
        }
    }
    pub fn compute_status(&self) -> serde_json::Value {
        self.net.as_ref().map_or_else(
            || serde_json::json!({"loaded":false,"error":self.error}),
            Session::status,
        )
    }
    pub fn compute_timing(&self) -> serde_json::Value {
        self.net.as_ref().map_or(serde_json::Value::Null, |net| {
            serde_json::to_value(&net.timing).unwrap_or(serde_json::Value::Null)
        })
    }

    pub fn infer(&mut self, frame: &Frame<'_>, threshold: f64) -> ResultData {
        let Some(net) = &mut self.net else {
            return ResultData::failed(if self.error.is_empty() {
                "Model not loaded".to_owned()
            } else {
                self.error.clone()
            });
        };
        net.begin_frame();
        match Self::forward(net, frame, threshold) {
            Ok(result) => result,
            Err(error) => ResultData::failed(error.to_string()),
        }
    }

    fn forward(net: &mut Session, frame: &Frame<'_>, threshold: f64) -> Result<ResultData, Error> {
        let input = lane_tensor(&lane_image(frame)?)?;
        let names = net.output_names()?;
        let outputs = net.forward(input.view(), &names)?;
        let result = decode(&names, &outputs, threshold)?;
        if let Some(shadow) = net.take_shadow() {
            let started = std::time::Instant::now();
            let cpu_started = super::session::cpu_seconds();
            match decode(&names, &shadow, threshold) {
                Ok(cpu) => {
                    let equal = cpu.valid == result.valid
                        && cpu.left_line == result.left_line
                        && cpu.right_line == result.right_line
                        && cpu.candidates_count == result.candidates_count;
                    let summaries = candidate_comparison(&names, &shadow, &outputs, threshold);
                    match summaries {
                        Ok((cpu_candidates, gpu_candidates, geometry_equal)) => net.decoded(
                            serde_json::json!({"result":cpu,"candidates":cpu_candidates}),
                            serde_json::json!({"result":result,"candidates":gpu_candidates}),
                            equal && geometry_equal,
                        ),
                        Err(error) => net.comparison_error(error.to_string()),
                    }
                }
                Err(error) => net.comparison_error(error.to_string()),
            }
            net.add_comparison_cost(
                started.elapsed().as_secs_f64() * 1000.0,
                (super::session::cpu_seconds() - cpu_started) * 1000.0,
            );
        }
        Ok(result)
    }
}

fn decode(names: &[String], outputs: &[Tensor], threshold: f64) -> Result<ResultData, Error> {
    if names.len() != outputs.len() {
        return Err(Error::Contract("lane output count mismatch"));
    }
    let output = |wanted| -> Result<TensorView<'_>, Error> {
        let index = names
            .iter()
            .position(|name| name == wanted)
            .ok_or(Error::Contract("lane model output name missing"))?;
        Ok(outputs[index].view())
    };
    let predictions = output("output0")?;
    let prototypes = output("output1")?;
    if !matches!(predictions.shape(), [1, 42, _]) || prototypes.shape() != [1, 32, 104, 104] {
        return Err(Error::Contract("invalid lane output tensor shape"));
    }
    let threshold = threshold
        .to_f32()
        .ok_or(Error::Contract("invalid lane threshold"))?;
    Ok(Head::new(predictions.values(), prototypes.values())?.result(threshold, 0.5)?)
}

// Diagnostic-only geometry comparison; normal inference does not repeat mask work.
fn candidate_comparison(
    names: &[String],
    cpu: &[Tensor],
    gpu: &[Tensor],
    threshold: f64,
) -> Result<(serde_json::Value, serde_json::Value, bool), Error> {
    let candidates = |outputs: &[Tensor]| -> Result<Vec<crate::lane::Candidate>, Error> {
        let get = |name| {
            names
                .iter()
                .position(|candidate| candidate == name)
                .and_then(|index| outputs.get(index))
                .ok_or(Error::Contract("comparison output missing"))
        };
        let predictions = get("output0")?.view();
        let prototypes = get("output1")?.view();
        let threshold = threshold
            .to_f32()
            .ok_or(Error::Contract("invalid lane threshold"))?;
        Ok(Head::new(predictions.values(), prototypes.values())?.candidates(threshold, 0.5)?)
    };
    let cpu = candidates(cpu)?;
    let gpu = candidates(gpu)?;
    let equal = cpu.len() == gpu.len()
        && cpu.iter().zip(&gpu).all(|(cpu, gpu)| {
            cpu.class_id == gpu.class_id && cpu.bottom == gpu.bottom && cpu.center == gpu.center
        });
    let summarize = |values: &[crate::lane::Candidate]| {
        serde_json::json!({
        "total":values.len(),"truncated":values.len()>16,"first":values.iter().take(16).collect::<Vec<_>>()})
    };
    Ok((summarize(&cpu), summarize(&gpu), equal))
}
