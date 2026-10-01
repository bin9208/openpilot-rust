use crate::{numerics::Numerics, numpy_exp, Error};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct RawLayer {
    activation: String,
    #[serde(flatten)]
    tensors: BTreeMap<String, Vec<Vec<f32>>>,
}
#[derive(Deserialize)]
struct RawModel {
    input_size: usize,
    input_mean: Vec<Vec<f32>>,
    input_std: Vec<Vec<f32>>,
    layers: Vec<RawLayer>,
}
enum Activation {
    Sigmoid,
    Identity,
}
struct Layer {
    weights: Vec<f32>,
    bias: Vec<f32>,
    activation: Activation,
}
pub struct Flux {
    mean: Vec<f32>,
    std: Vec<f32>,
    layers: Vec<Layer>,
    pub friction_override: bool,
}
impl Flux {
    pub fn decode(bytes: &[u8], kernel: &Numerics) -> Result<Self, Error> {
        let raw: RawModel = serde_json::from_slice(bytes)?;
        let mean = column(raw.input_mean)?;
        let std = column(raw.input_std)?;
        if mean.len() != raw.input_size || std.len() != raw.input_size {
            return Err(Error::Contract("Flux normalization dimensions"));
        }
        let mut input_size = raw.input_size;
        let mut layers = Vec::new();
        for raw in raw.layers {
            let weights = raw
                .tensors
                .iter()
                .find(|(key, _)| key.ends_with("_W"))
                .ok_or(Error::Contract("Flux weights"))?
                .1;
            let bias = raw
                .tensors
                .iter()
                .find(|(key, _)| key.ends_with("_b"))
                .ok_or(Error::Contract("Flux bias"))?
                .1
                .clone();
            let bias = column(bias)?;
            if bias.is_empty()
                || bias.len() != weights.len()
                || weights.iter().any(|row| row.len() != input_size)
            {
                return Err(Error::Contract("Flux layer dimensions"));
            }
            let activation = match raw.activation.as_str() {
                "σ" | "sigmoid" => Activation::Sigmoid,
                "identity" => Activation::Identity,
                _ => return Err(Error::Contract("Flux activation")),
            };
            layers.push(Layer {
                weights: weights.iter().flatten().copied().collect(),
                bias,
                activation,
            });
            input_size = weights.len();
        }
        if layers.is_empty() {
            return Err(Error::Contract("Flux layers"));
        }
        let mut model = Self {
            mean,
            std,
            layers,
            friction_override: false,
        };
        model.friction_override = model.evaluate(&[10., 0., 0.2], kernel)? < 0.1;
        Ok(model)
    }
    pub fn evaluate(&self, input: &[f64], kernel: &Numerics) -> Result<f64, Error> {
        if input.len() != self.mean.len() && (input.len() < 2 || input.len() > self.mean.len()) {
            return Err(Error::Contract("Flux input dimensions"));
        }
        let mut values = vec![0_f32; self.mean.len()];
        for (out, value) in values.iter_mut().zip(input) {
            *out = *value as f32;
        }
        for (i, value) in values.iter_mut().enumerate() {
            *value = (*value - self.mean[i]) / self.std[i];
        }
        for layer in &self.layers {
            values = kernel.float_matrix(&values, layer.bias.len(), &layer.weights)?;
            for (value, bias) in values.iter_mut().zip(&layer.bias) {
                *value += bias;
                match layer.activation {
                    Activation::Sigmoid => *value = 1. / (1. + numpy_exp::exp(-*value)),
                    Activation::Identity => {}
                }
            }
        }
        Ok(f64::from(values[0]))
    }
}
fn column(values: Vec<Vec<f32>>) -> Result<Vec<f32>, Error> {
    values
        .into_iter()
        .map(|row| match row.as_slice() {
            [value] => Ok(*value),
            _ => Err(Error::Contract("Flux column dimensions")),
        })
        .collect()
}
