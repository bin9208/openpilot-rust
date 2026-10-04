use super::Error;
use crate::{
    lane::{Head, ResultData},
    nv12::Frame,
};
use num_traits::ToPrimitive;
use openpilot_opencv_runtime::{
    self as cv, Dimensions, DnnNet, Format, Image, ImageLayout, Tensor, TensorView,
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
    for _ in 0..3 {
        values.extend(view.data().iter().map(|&byte| f32::from(byte) / 255.0));
    }
    Ok(Tensor::from_parts(vec![1, 3, 416, 416], values)?)
}

pub struct LaneModel {
    net: Option<DnnNet>,
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
        match DnnNet::load(path) {
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

    pub const fn loaded(&self) -> bool {
        self.net.is_some()
    }
    pub fn error(&self) -> &str {
        &self.error
    }

    pub fn infer(&mut self, frame: &Frame<'_>, threshold: f64) -> ResultData {
        let Some(net) = &mut self.net else {
            return ResultData::failed(if self.error.is_empty() {
                "Model not loaded".to_owned()
            } else {
                self.error.clone()
            });
        };
        match Self::forward(net, frame, threshold) {
            Ok(result) => result,
            Err(error) => ResultData::failed(error.to_string()),
        }
    }

    fn forward(net: &mut DnnNet, frame: &Frame<'_>, threshold: f64) -> Result<ResultData, Error> {
        let input = lane_tensor(&lane_image(frame)?)?;
        let names = net.output_names()?;
        let outputs = net.forward(input.view(), &names)?;
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
}
