use crate::{numerics::Numerics, Error};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Nano {
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
        let layers = [
            (&self.w_1, &self.b_1),
            (&self.w_2, &self.b_2),
            (&self.w_3, &self.b_3),
            (&self.w_4, &self.b_4),
        ];
        for (i, (weights, bias)) in layers.into_iter().enumerate() {
            if weights.len() != values.len() || weights.iter().any(|row| row.len() != bias.len()) {
                return Err(Error::Contract("Nano layer dimensions"));
            }
            let flat = weights.iter().flatten().copied().collect::<Vec<_>>();
            values = kernel.double_matrix(&values, bias.len(), &flat)?;
            for (value, bias) in values.iter_mut().zip(bias) {
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
