//! Pinned neural pixel/tensor/box interpretation; inference and numeric sort owners are explicit.
use crate::{
    neural_sequence::{Color, Error, Network, Proposal},
    numpy_exp, numpy_sort,
    pixels::Rgb,
};
use openpilot_opencv_runtime as cv;
use openpilot_plannerd::signal::tracker::overlap;
#[derive(Clone)]
pub struct Tensor {
    pub shape: Vec<i64>,
    pub values: Vec<f32>,
}
pub trait Inference {
    fn run(&mut self, name: &str, input: &Tensor) -> Result<Tensor, Error>;
}
pub trait Sort {
    fn ascending(&mut self, keys: &[f32]) -> Result<Vec<usize>, Error>;
}
pub struct PortableSort;
impl Sort for PortableSort {
    fn ascending(&mut self, keys: &[f32]) -> Result<Vec<usize>, Error> {
        Ok(numpy_sort::scalar_argsort(keys))
    }
}
pub struct SourceSort;
impl Sort for SourceSort {
    fn ascending(&mut self, keys: &[f32]) -> Result<Vec<usize>, Error> {
        Ok(numpy_sort::argsort(keys))
    }
}
pub struct Neural<D, C, A> {
    pub detector: D,
    pub classifier: C,
    pub sort: A,
}
fn slice(index: i32, length: i32) -> i32 {
    if index < 0 {
        (length + index).max(0)
    } else {
        index.min(length)
    }
}
fn crop(rgb: &Rgb, rect: [i32; 4], bgr: bool) -> Result<Option<Rgb>, Error> {
    rgb.view()?;
    let [a, b, c, d] = rect;
    let w = rgb.width as i32;
    let h = rgb.height as i32;
    let [a, b, c, d] = [slice(a, w), slice(b, h), slice(c, w), slice(d, h)];
    if c <= a || d <= b {
        return Ok(None);
    }
    let mut data = Vec::with_capacity((c - a) as usize * (d - b) as usize * 3);
    for y in b..d {
        for x in a..c {
            let offset = (y as usize * rgb.width as usize + x as usize) * 3;
            let pixel = &rgb.data[offset..offset + 3];
            if bgr {
                data.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            } else {
                data.extend_from_slice(pixel);
            }
        }
    }
    Ok(Some(Rgb {
        width: (c - a) as u32,
        height: (d - b) as u32,
        data,
    }))
}
fn resized(rgb: &Rgb, width: u32, height: u32) -> Result<cv::Image, Error> {
    Ok(cv::resize_linear(
        rgb.view()?,
        cv::Dimensions::new(width, height)?,
    )?)
}
fn channel_tensor(
    data: &[u8],
    width: usize,
    height: usize,
    target: usize,
    classifier: bool,
) -> Tensor {
    let mut values = vec![if classifier { 0. } else { 114. }; 3 * target * target];
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                values[c * target * target + y * target + x] =
                    f32::from(data[(y * width + x) * 3 + c]);
            }
        }
    }
    if classifier {
        let means = [123.675_f32, 116.28, 103.53];
        let deviations = [58.395_f32, 57.12, 57.375];
        for c in 0..3 {
            for v in &mut values[c * target * target..(c + 1) * target * target] {
                *v = (*v - means[c]) / deviations[c];
            }
        }
    }
    Tensor {
        shape: vec![1, 3, target as i64, target as i64],
        values,
    }
}
const LABELS: [&str; 11] = [
    "green",
    "left,red",
    "left,red,straight",
    "red",
    "red,right",
    "red,straight",
    "unknown",
    "yellow",
    "red,up_left",
    "red,right,straight",
    "red,up_right",
];
impl<D: Inference, C: Inference, A: Sort> Neural<D, C, A> {
    /// Capture all three source warmup pairs before camera subscription.
    pub fn warm_up(&mut self) -> Result<(), Error> {
        let detector = Tensor {
            shape: vec![1, 3, 416, 416],
            values: vec![0.; 3 * 416 * 416],
        };
        let classifier = Tensor {
            shape: vec![1, 3, 224, 224],
            values: vec![0.; 3 * 224 * 224],
        };
        for _ in 0..3 {
            self.detector.run("images", &detector)?;
            self.classifier.run("input", &classifier)?;
        }
        Ok(())
    }
    pub fn proposals(&mut self, rgb: &Rgb, rect: Option<[i32; 4]>) -> Result<Vec<Proposal>, Error> {
        let rect = rect.unwrap_or([0, 0, rgb.width as i32, rgb.height as i32]);
        let image = crop(rgb, rect, true)?.ok_or("empty detector crop")?;
        let scale = (416. / f64::from(image.width)).min(416. / f64::from(image.height));
        let width = (f64::from(image.width) * scale) as u32;
        let height = (f64::from(image.height) * scale) as u32;
        let resized = resized(&image, width, height)?;
        let input = channel_tensor(
            resized.view().data(),
            width as usize,
            height as usize,
            416,
            false,
        );
        let output = self.detector.run("images", &input)?;
        if output.shape != [1, 3549, 8] || output.values.len() != 3549 * 8 {
            return Err("detector tensor contract".into());
        }
        let mut selected = Vec::new();
        let mut scores = Vec::new();
        for i in 0..3549 {
            let score = output.values[i * 8 + 4] * output.values[i * 8 + 6];
            if score > 0.3 {
                selected.push(i);
                scores.push(-score);
            }
        }
        let order = self.sort.ascending(&scores)?;
        if order.len() != selected.len() {
            return Err("sort length contract".into());
        }
        let mut seen = vec![false; order.len()];
        let mut kept: Vec<Proposal> = Vec::new();
        for j in order {
            if j >= seen.len() || seen[j] {
                return Err("sort permutation contract".into());
            }
            seen[j] = true;
            let index = selected[j];
            let q = &output.values[index * 8..index * 8 + 8];
            let (position, width, stride) = if index < 2704 {
                (index, 52, 8_f32)
            } else if index < 3380 {
                (index - 2704, 26, 16_f32)
            } else {
                (index - 3380, 13, 32_f32)
            };
            let cx = (q[0] + (position % width) as f32) * stride;
            let cy = (q[1] + (position / width) as f32) * stride;
            let sx = numpy_exp::exp(q[2]) * stride;
            let sy = numpy_exp::exp(q[3]) * stride;
            let raw = [cx - sx / 2., cy - sy / 2., cx + sx / 2., cy + sy / 2.];
            let bounds = std::array::from_fn(|i| {
                let value = f64::from(raw[i] / scale as f32) + f64::from(rect[i % 2]);
                value.clamp(
                    0.,
                    f64::from(if i % 2 == 0 { rgb.width } else { rgb.height }),
                )
            });
            let [a, b, c, d] = bounds;
            if !bounds.iter().all(|v| v.is_finite()) || a >= c || b >= d {
                continue;
            }
            if kept.iter().all(|p| overlap(bounds, p.bounds) < 0.65) {
                kept.push(Proposal {
                    bounds,
                    score: f64::from(-scores[j]),
                });
            }
        }
        Ok(kept)
    }
    pub fn color(&mut self, rgb: &Rgb, bounds: [f64; 4]) -> Result<Color, Error> {
        if !bounds.iter().all(|v| v.is_finite()) {
            return Err("invalid classifier box".into());
        }
        let [a, b, c, d] = bounds.map(|v| v as i32);
        let Some(image) = crop(
            rgb,
            [
                a.max(0),
                b.max(0),
                c.min(rgb.width as i32),
                d.min(rgb.height as i32),
            ],
            false,
        )?
        else {
            return Ok(Color {
                label: "unknown".into(),
                score: 0.,
            });
        };
        let scale = (224. / f64::from(image.width)).min(224. / f64::from(image.height));
        let width = (f64::from(image.width) * scale).round_ties_even().max(1.) as u32;
        let height = (f64::from(image.height) * scale).round_ties_even().max(1.) as u32;
        let resized = resized(&image, width, height)?;
        let input = channel_tensor(
            resized.view().data(),
            width as usize,
            height as usize,
            224,
            true,
        );
        let output = self.classifier.run("input", &input)?;
        if output.shape != [1, 11] || output.values.len() != 11 {
            return Err("classifier tensor contract".into());
        }
        let mut index = 0;
        for i in 1..11 {
            if !output.values[index].is_nan()
                && (output.values[i].is_nan() || output.values[i] > output.values[index])
            {
                index = i;
            }
        }
        Ok(Color {
            label: LABELS[index].into(),
            score: f64::from(output.values[index]),
        })
    }
}
impl<D: Inference, C: Inference, A: Sort> Network for Neural<D, C, A> {
    fn detect(&mut self, rgb: &Rgb, rect: [i32; 4]) -> Result<Vec<Proposal>, Error> {
        self.proposals(rgb, Some(rect))
    }
    fn classify(&mut self, rgb: &Rgb, bounds: [f64; 4]) -> Result<Color, Error> {
        self.color(rgb, bounds)
    }
}

/// Optional explicit CPU provider adapter; classical owners never load ORT.
#[cfg(feature = "neural-ort")]
impl Inference for openpilot_onnx_runtime::Session {
    fn run(&mut self, name: &str, input: &Tensor) -> Result<Tensor, Error> {
        let mut outputs = self.run(name, &input.values, &input.shape)?;
        if outputs.len() != 1 {
            return Err("signal network output count".into());
        }
        let output = outputs.remove(0);
        Ok(Tensor {
            shape: output.shape,
            values: output.values,
        })
    }
}

/// Load only the two authenticated reference models on the explicit CPU provider.
#[cfg(feature = "neural-ort")]
pub fn cpu(
    provider: &openpilot_onnx_runtime::Provider,
    models: &std::path::Path,
) -> Result<
    Neural<openpilot_onnx_runtime::Session, openpilot_onnx_runtime::Session, SourceSort>,
    Error,
> {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../upstream/signal-runtime.json"))?;
    let load = |name: &str| {
        openpilot_onnx_runtime::Session::load(
            provider,
            &models.join(format!("{name}.onnx")),
            fixture["models"][name]["sha256"]
                .as_str()
                .ok_or("missing model hash")?,
        )
        .map_err(Error::from)
    };
    Ok(Neural {
        detector: load("detector")?,
        classifier: load("classifier")?,
        sort: SourceSort,
    })
}
