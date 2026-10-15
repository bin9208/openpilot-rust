//! Native ONNX import and offline admission, derived from tinygrad/nn/onnx.py
//! and model_export/offline_onnx.py. Includes typed reference lowering and
//! offline lifetime planning; backend generation/device execution are separate.
//! Tinygrad MIT provenance: ../qcom/LICENSE.
pub mod lower;
pub mod memory;
pub mod metadata;
mod parse;
mod protobuf;
pub mod render;
pub mod scalar;
pub mod schedule;
use std::{collections::BTreeMap, path::Path};
pub const STATIC_OPERATORS: &[&str] = &[
    "Identity",
    "Cast",
    "Concat",
    "Sub",
    "Div",
    "Conv",
    "Gelu",
    "Mul",
    "Add",
    "ReduceMean",
    "Relu",
    "Sigmoid",
    "Reshape",
    "Gemm",
    "ReduceL2",
    "Clip",
    "Expand",
    "Slice",
    "Unsqueeze",
    "Transpose",
    "GatherND",
    "MatMul",
    "LayerNormalization",
    "Squeeze",
    "Where",
    "Softmax",
    "Gather",
];
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeDimension {
    Fixed(i64),
    Symbol(String),
    Unknown,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Value {
    pub name: String,
    pub dtype: u32,
    pub shape: Vec<ShapeDimension>,
    pub optional: bool,
    pub sequence: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Tensor {
    pub dtype: u32,
    pub shape: Vec<i64>,
    pub data: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Attribute {
    Float(f32),
    Integer(i64),
    String(String),
    Tensor(Tensor),
    Graph(Vec<u8>),
    Floats(Vec<f32>),
    Integers(Vec<i64>),
    Strings(Vec<String>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub name: String,
    pub op: String,
    pub domain: String,
    pub opset: i64,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub attributes: BTreeMap<String, Attribute>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub initializers: BTreeMap<String, Tensor>,
    pub inputs: Vec<Value>,
    pub outputs: Vec<Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Metadata {
    pub checkpoint: String,
    pub slices: BTreeMap<String, (usize, usize)>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub graph: Graph,
    pub opsets: BTreeMap<String, i64>,
    pub metadata_properties: BTreeMap<String, String>,
}
impl Model {
    pub fn load(path: &Path) -> Result<Self, String> {
        let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        if size > 4 * 1024 * 1024 * 1024 {
            return Err("ONNX source exceeds 4 GiB".into());
        }
        Self::parse_with_origin(&std::fs::read(path).map_err(|e| e.to_string())?, Some(path))
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        Self::parse_with_origin(bytes, None)
    }
    pub(crate) fn parse_with_origin(bytes: &[u8], origin: Option<&Path>) -> Result<Self, String> {
        parse::model(bytes, origin)
    }
    pub fn pipeline_metadata(&self) -> Result<Metadata, String> {
        metadata::pipeline(self)
    }
    pub fn admit_offline(&self) -> Result<(), String> {
        for node in &self.graph.nodes {
            if matches!(
                node.domain.as_str(),
                "ai.onnx.training" | "ai.onnx.preview.training"
            ) {
                return Err("offline ONNX training graph is unsupported".into());
            }
            if !STATIC_OPERATORS.contains(&node.op.as_str()) {
                return Err(format!("unsupported offline ONNX operator: {}", node.op));
            }
            let controls: &[usize] = match node.op.as_str() {
                "Expand" | "Reshape" | "Squeeze" | "Unsqueeze" | "ReduceMean" | "ReduceL2" => &[1],
                "Slice" => &[1, 2, 3, 4],
                _ => &[],
            };
            for &index in controls {
                if let Some(name) = node.inputs.get(index).filter(|s| !s.is_empty()) {
                    if !self.graph.initializers.contains_key(name) {
                        return Err("offline shape control depends on runtime content".into());
                    }
                }
            }
        }
        Ok(())
    }
}
