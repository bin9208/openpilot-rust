use crate::{numerics::Numerics, numpy_exp, Error};
use serde::Deserialize;
use std::collections::BTreeMap;

// #553: 진단 feature가 없으면 원래 수치 expression만 남긴다.
#[cfg(feature = "diagnostic-553")]
macro_rules! tap_553 {
    ($observer:ident, $stage:literal, $index:expr, $value:expr) => {{
        let value: f32 = $value;
        $observer.scalar($stage, $index, value)?
    }};
}
#[cfg(not(feature = "diagnostic-553"))]
macro_rules! tap_553 {
    ($_observer:ident, $_stage:literal, $_index:expr, $value:expr) => {
        $value
    };
}

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
        #[cfg(feature = "diagnostic-553")]
        let mut observation = diagnostic_553::Observation::new(input, self)?;
        let mut values = vec![0_f32; self.mean.len()];
        for (out, value) in values.iter_mut().zip(input) {
            *out = *value as f32;
        }
        #[cfg(feature = "diagnostic-553")]
        observation.vector("cast", &values)?;
        for (i, value) in values.iter_mut().enumerate() {
            *value = tap_553!(
                observation,
                "normalize",
                i,
                tap_553!(observation, "subtract", i, *value - self.mean[i]) / self.std[i]
            );
        }
        for layer in &self.layers {
            #[cfg(feature = "diagnostic-553")]
            observation.begin_layer(layer, &values)?;
            values = kernel.float_matrix(&values, layer.bias.len(), &layer.weights)?;
            #[cfg(feature = "diagnostic-553")]
            observation.vector("dot", &values)?;
            for (neuron, (value, bias)) in values.iter_mut().zip(&layer.bias).enumerate() {
                #[cfg(not(feature = "diagnostic-553"))]
                let _ = neuron;
                *value += bias;
                #[cfg(feature = "diagnostic-553")]
                observation.scalar("bias_output", neuron, *value)?;
                match layer.activation {
                    Activation::Sigmoid => {
                        *value = tap_553!(
                            observation,
                            "sigmoid",
                            neuron,
                            1. / tap_553!(
                                observation,
                                "denominator",
                                neuron,
                                1. + tap_553!(
                                    observation,
                                    "exp_output",
                                    neuron,
                                    numpy_exp::exp(tap_553!(
                                        observation,
                                        "exp_input",
                                        neuron,
                                        -*value
                                    ))
                                )
                            )
                        );
                    }
                    Activation::Identity => {}
                }
                #[cfg(feature = "diagnostic-553")]
                {
                    observation.scalar("layer_output", neuron, *value)?;
                }
            }
        }
        #[cfg(feature = "diagnostic-553")]
        observation.finish(f64::from(values[0]))?;
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

// #553 한정 관측: default-off이며 production 수치 식과 gate를 변경하지 않는다.
#[cfg(feature = "diagnostic-553")]
mod diagnostic_553 {
    use super::{Error, Flux, Layer};
    use serde_json::{json, Value};
    use std::{
        io::Write,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static CALLS: AtomicUsize = AtomicUsize::new(0);
    const MAX_CALLS: usize = 16;
    const MAX_EVENTS: usize = 512;
    const MAX_VECTOR: usize = 256;
    const MAX_RECORD: usize = 2 * 1024 * 1024;

    pub(super) struct Observation {
        call: usize,
        layer: Option<usize>,
        input_bits: Vec<u64>,
        events: Vec<Value>,
    }
    impl Observation {
        pub(super) fn new(input: &[f64], model: &Flux) -> Result<Self, Error> {
            let shape = model
                .layers
                .iter()
                .map(|layer| layer.bias.len())
                .collect::<Vec<_>>();
            if model.mean.len() != 18 || model.std.len() != 18 || shape != [7, 13, 3, 1] {
                return Err(Error::Contract(
                    "#553 observer requires the bound Palisade shape",
                ));
            }
            let call = CALLS.fetch_add(1, Ordering::Relaxed);
            if call >= MAX_CALLS || input.len() > 18 {
                return Err(Error::Contract("#553 observer call/input bound"));
            }
            let mut observer = Self {
                call,
                layer: None,
                input_bits: input.iter().map(|value| value.to_bits()).collect(),
                events: Vec::new(),
            };
            observer.vector("mean", &model.mean)?;
            observer.vector("std", &model.std)?;
            observer.push(json!({"stage":"friction_before", "value":model.friction_override}))?;
            Ok(observer)
        }
        fn push(&mut self, event: Value) -> Result<(), Error> {
            if self.events.len() >= MAX_EVENTS {
                return Err(Error::Contract("#553 observer event bound"));
            }
            self.events.push(event);
            Ok(())
        }
        pub(super) fn vector(&mut self, stage: &str, values: &[f32]) -> Result<(), Error> {
            if values.len() > MAX_VECTOR {
                return Err(Error::Contract("#553 observer vector bound"));
            }
            self.push(json!({"stage":stage, "layer":self.layer, "bits":values.iter().map(|value|value.to_bits()).collect::<Vec<_>>(),
                "elements":values.len(), "stride_bytes":4, "address":values.as_ptr() as usize}))
        }
        pub(super) fn scalar(
            &mut self,
            stage: &str,
            index: usize,
            value: f32,
        ) -> Result<f32, Error> {
            self.push(
                json!({"stage":stage,"layer":self.layer,"index":index,"bits":value.to_bits()}),
            )?;
            Ok(value)
        }
        pub(super) fn begin_layer(&mut self, layer: &Layer, input: &[f32]) -> Result<(), Error> {
            self.layer = Some(self.layer.map_or(0, |value| value + 1));
            self.vector("dot_input", input)?;
            self.vector("weights_fortran_flat", &layer.weights)?;
            self.vector("bias", &layer.bias)
        }
        pub(super) fn finish(self, output: f64) -> Result<(), Error> {
            let record = json!({"format":553,"diagnostic_only":true,"call":self.call,
                "input_f64_bits":self.input_bits,"events":self.events,"output_f64_bits":output.to_bits()});
            let encoded = serde_json::to_vec(&record)?;
            if encoded.len() > MAX_RECORD {
                return Err(Error::Contract("#553 observer record bound"));
            }
            let mut output = std::io::stderr().lock();
            output.write_all(b"CONTROLSD_553_LAYER ")?;
            output.write_all(&encoded)?;
            output.write_all(b"\n")?;
            output.flush()?;
            Ok(())
        }
    }
}
