use crate::{numerics::Numerics, Error};
use serde::Deserialize;

#[derive(Deserialize)]
struct Weights {
    w_1: Vec<Vec<f64>>,
    b_1: Vec<f64>,
    w_2: Vec<Vec<f64>>,
    b_2: Vec<f64>,
    w_3: Vec<Vec<f64>>,
    b_3: Vec<f64>,
    w_4: Vec<Vec<f64>>,
    b_4: Vec<f64>,
    input_norm_mat: Vec<[f64; 2]>,
    output_norm_mat: [f64; 2],
}
struct Layer {
    weights: Vec<f64>,
    bias: Vec<f64>,
}

#[derive(Deserialize)]
#[serde(try_from = "Weights")]
pub struct Nano {
    layers: [Layer; 4],
    input_norm_mat: Vec<[f64; 2]>,
    output_norm_mat: [f64; 2],
}
impl TryFrom<Weights> for Nano {
    type Error = Error;
    fn try_from(raw: Weights) -> Result<Self, Error> {
        let mut inputs = raw.input_norm_mat.len();
        let mut flatten = |weights: Vec<Vec<f64>>, bias: Vec<f64>| {
            if inputs == 0
                || bias.is_empty()
                || weights.len() != inputs
                || weights.iter().any(|row| row.len() != bias.len())
            {
                return Err(Error::Contract("Nano layer dimensions"));
            }
            inputs = bias.len();
            Ok(Layer {
                weights: weights.into_iter().flatten().collect(),
                bias,
            })
        };
        let layers = [
            flatten(raw.w_1, raw.b_1)?,
            flatten(raw.w_2, raw.b_2)?,
            flatten(raw.w_3, raw.b_3)?,
            flatten(raw.w_4, raw.b_4)?,
        ];
        Ok(Self {
            layers,
            input_norm_mat: raw.input_norm_mat,
            output_norm_mat: raw.output_norm_mat,
        })
    }
}
impl Nano {
    pub fn predict(&self, input: &[f64], kernel: &Numerics) -> Result<f64, Error> {
        if input.len() != self.input_norm_mat.len() {
            return Err(Error::Contract("Nano input dimensions"));
        }
        let mut values: Vec<f64> = input
            .iter()
            .zip(&self.input_norm_mat)
            .map(|(v, norm)| (v - norm[0]) / (norm[1] - norm[0]))
            .collect();
        for (i, layer) in self.layers.iter().enumerate() {
            values = kernel.double_matrix(&values, layer.bias.len(), &layer.weights)?;
            for (value, bias) in values.iter_mut().zip(&layer.bias) {
                *value += bias;
                if i < 3 && *value < 0. {
                    *value = 0.;
                }
            }
        }
        Ok(
            values[0] * (self.output_norm_mat[1] - self.output_norm_mat[0])
                + self.output_norm_mat[0],
        )
    }
}
