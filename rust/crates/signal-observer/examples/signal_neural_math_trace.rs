//! Executes native neural pixel/box policy with source-oracle scripted logits.
use openpilot_signal_observer::{
    neural::{Inference, Neural, SourceSort, Tensor},
    neural_sequence::Error,
    pixels::Rgb,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
};
#[derive(Deserialize)]
struct Row {
    index: usize,
    values: [f32; 8],
}
#[derive(Deserialize)]
struct Frame {
    path: PathBuf,
    width: u32,
    height: u32,
    rect: Option<[i32; 4]>,
    #[serde(rename = "box")]
    bounds: [f64; 4],
    rows: Vec<Row>,
    probabilities: Vec<f32>,
}
struct Scripted {
    output: Tensor,
    captures: Vec<Value>,
}
impl Inference for Scripted {
    fn run(&mut self, name: &str, input: &Tensor) -> Result<Tensor, Error> {
        let mut hash = Sha256::new();
        for v in &input.values {
            hash.update(v.to_le_bytes());
        }
        self.captures.push(
            json!({"name":name,"shape":input.shape,"sha256":format!("{:x}",hash.finalize())}),
        );
        Ok(self.output.clone())
    }
}
fn main() -> Result<(), Error> {
    openpilot_opencv_runtime::initialize(1)?;
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let frames: Vec<Frame> = serde_json::from_str(&text)?;
    let mut result = Vec::new();
    for frame in frames {
        let mut values = vec![0.; 3549 * 8];
        for row in frame.rows {
            values[row.index * 8..row.index * 8 + 8].copy_from_slice(&row.values);
        }
        let mut network = Neural {
            detector: Scripted {
                output: Tensor {
                    shape: vec![1, 3549, 8],
                    values,
                },
                captures: Vec::new(),
            },
            classifier: Scripted {
                output: Tensor {
                    shape: vec![1, 11],
                    values: frame.probabilities,
                },
                captures: Vec::new(),
            },
            sort: SourceSort,
        };
        let rgb = Rgb {
            width: frame.width,
            height: frame.height,
            data: fs::read(frame.path)?,
        };
        let proposals = network.proposals(&rgb, frame.rect)?;
        let color = network.color(&rgb, frame.bounds)?;
        let mut captures = network.detector.captures;
        captures.extend(network.classifier.captures);
        result.push(json!({"proposals":proposals,"color":color,"captures":captures}));
    }
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
