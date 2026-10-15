//! Native lowering to typed scalar expressions and explicit linear-index maps.
//! The reference executor verifies these primitives; backend scheduling/rendering
//! and device execution are separate stages and remain outside this module.
mod activation;
mod convolution;
mod indexing;
mod normalization;
mod reduction;
use super::{
    scalar::{self, Binary, Scalar},
    *,
};
#[derive(Clone, Debug, PartialEq)]
pub struct TensorType {
    pub dtype: u32,
    pub shape: Vec<usize>,
}
impl TensorType {
    fn count(&self) -> Result<usize, String> {
        self.shape.iter().try_fold(1usize, |n, &d| {
            n.checked_mul(d)
                .ok_or_else(|| "tensor element count overflow".into())
        })
    }
    fn bytes(&self) -> Result<usize, String> {
        self.count()?
            .checked_mul(super::parse::width(self.dtype)?)
            .filter(|&v| v <= 4 * 1024 * 1024 * 1024)
            .ok_or_else(|| "lowered tensor exceeds 4 GiB".into())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum IndexExpr {
    Index,
    Reduction,
    Constant(i128),
    Add(Box<Self>, Box<Self>),
    Mul(Box<Self>, i128),
    Div(Box<Self>, usize),
    Mod(Box<Self>, usize),
}
impl IndexExpr {
    fn eval(&self, index: usize, reduction: usize) -> i128 {
        match self {
            Self::Index => index as i128,
            Self::Reduction => reduction as i128,
            Self::Constant(v) => *v,
            Self::Add(a, b) => a.eval(index, reduction) + b.eval(index, reduction),
            Self::Mul(a, b) => a.eval(index, reduction) * b,
            Self::Div(a, b) => a.eval(index, reduction) / (*b as i128),
            Self::Mod(a, b) => a.eval(index, reduction) % (*b as i128),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum ScalarExpr {
    Input(String),
    PaddedLoad {
        plan: usize,
        coordinates: Vec<(IndexExpr, usize, usize)>,
    },
    GatherLoad {
        plan: usize,
        base: IndexExpr,
        axes: Vec<(ScalarExpr, usize, usize)>,
    },
    Constant(Vec<u8>),
    Load {
        plan: usize,
        index: IndexExpr,
    },
    Cast {
        dtype: u32,
        value: Box<Self>,
    },
    /// An observable materialized storage conversion, unlike a target's
    /// emulated arithmetic cast. Used by original IMAGE input/weight staging.
    StorageRound {
        dtype: u32,
        value: Box<Self>,
    },
    Binary {
        kind: Binary,
        dtype: u32,
        left: Box<Self>,
        right: Box<Self>,
    },
    Literal(Scalar),
    MulAcc {
        dtype: u32,
        a: Box<Self>,
        b: Box<Self>,
        c: Box<Self>,
    },
    FloatLiteral {
        dtype: u32,
        value: f64,
    },
    GreaterZero(Box<Self>),
    Select {
        condition: Box<Self>,
        yes: Box<Self>,
        no: Box<Self>,
    },
    IndexLess {
        index: IndexExpr,
        bound: i128,
    },
    Exp2 {
        dtype: u32,
        value: Box<Self>,
    },
    Sign {
        dtype: u32,
        value: Box<Self>,
    },
    Sqrt {
        dtype: u32,
        value: Box<Self>,
    },
    ReduceMax {
        dtype: u32,
        count: usize,
        value: Box<Self>,
    },
    ReduceSum {
        dtype: u32,
        count: usize,
        value: Box<Self>,
    },
    Trunc {
        dtype: u32,
        value: Box<Self>,
    },
    BoolNot(Box<Self>),
    Reciprocal {
        dtype: u32,
        value: Box<Self>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub ty: TensorType,
    pub expression: ScalarExpr,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoweredGraph {
    offline_images: bool,
    pub plans: Vec<Plan>,
    pub names: BTreeMap<String, usize>,
    pub outputs: BTreeMap<String, usize>,
}
fn add(a: IndexExpr, b: IndexExpr) -> IndexExpr {
    IndexExpr::Add(Box::new(a), Box::new(b))
}
fn mul(a: IndexExpr, b: usize) -> IndexExpr {
    IndexExpr::Mul(Box::new(a), b as i128)
}
fn coordinate(shape: &[usize], axis: usize) -> IndexExpr {
    IndexExpr::Mod(
        Box::new(IndexExpr::Div(
            Box::new(IndexExpr::Index),
            shape[axis + 1..].iter().product(),
        )),
        shape[axis],
    )
}
fn index_from_coordinates(
    shape: &[usize],
    coordinates: impl Iterator<Item = IndexExpr>,
) -> IndexExpr {
    coordinates
        .zip(shape)
        .fold(IndexExpr::Constant(0), |index, (c, &size)| {
            add(mul(index, size), c)
        })
}
fn broadcast_index(input: &[usize], output: &[usize]) -> IndexExpr {
    let offset = output.len() - input.len();
    index_from_coordinates(
        input,
        input.iter().enumerate().map(|(i, &size)| {
            if size == 1 {
                IndexExpr::Constant(0)
            } else {
                coordinate(output, offset + i)
            }
        }),
    )
}
pub fn broadcast(shapes: &[&[usize]]) -> Result<Vec<usize>, String> {
    let rank = shapes.iter().map(|s| s.len()).max().unwrap_or(0);
    let mut output = vec![1; rank];
    for axis in 0..rank {
        let dimensions = shapes
            .iter()
            .map(|shape| {
                if axis + shape.len() < rank {
                    1
                } else {
                    shape[axis + shape.len() - rank]
                }
            })
            .collect::<Vec<_>>();
        output[axis] = if dimensions.contains(&0) {
            0
        } else {
            *dimensions.iter().max().unwrap_or(&1)
        };
        if dimensions.iter().any(|&d| d != 1 && d != output[axis]) {
            return Err("shape mismatch: objects cannot be broadcast to a single shape".into());
        }
    }
    Ok(output)
}
fn axis(value: i64, rank: usize) -> Result<usize, String> {
    let value = if value < 0 {
        rank as i64 + value
    } else {
        value
    };
    usize::try_from(value)
        .ok()
        .filter(|&v| v < rank)
        .ok_or_else(|| "axis outside tensor rank".into())
}
impl LoweredGraph {
    fn temporary(&mut self, ty: TensorType, expression: ScalarExpr) -> Result<usize, String> {
        ty.bytes()?;
        let id = self.plans.len();
        self.plans.push(Plan { ty, expression });
        Ok(id)
    }
    fn push(
        &mut self,
        name: &str,
        ty: TensorType,
        expression: ScalarExpr,
    ) -> Result<usize, String> {
        let id = self.temporary(ty, expression)?;
        self.names.insert(name.into(), id);
        Ok(id)
    }
    fn input(&self, node: &Node, index: usize) -> Result<usize, String> {
        node.inputs
            .get(index)
            .filter(|s| !s.is_empty())
            .and_then(|name| self.names.get(name))
            .copied()
            .ok_or_else(|| format!("missing input {index} for {}", node.op))
    }
    fn load(&self, id: usize, shape: &[usize]) -> ScalarExpr {
        ScalarExpr::Load {
            plan: id,
            index: broadcast_index(&self.plans[id].ty.shape, shape),
        }
    }
    fn control(
        &self,
        model: &Model,
        node: &Node,
        index: usize,
        attribute: &str,
    ) -> Result<Option<Vec<i64>>, String> {
        if let Some(name) = node.inputs.get(index).filter(|s| !s.is_empty()) {
            let tensor = model
                .graph
                .initializers
                .get(name)
                .ok_or("offline shape control depends on runtime content")?;
            let width = super::parse::width(tensor.dtype)?;
            if matches!(tensor.dtype, 1 | 10 | 11 | 16) {
                return Err("static shape control requires integer values".into());
            }
            return tensor
                .data
                .chunks_exact(width)
                .map(|bytes| {
                    i64::try_from(Scalar::read(tensor.dtype, bytes)?.integer()?)
                        .map_err(|_| "static shape value exceeds i64".into())
                })
                .collect::<Result<Vec<_>, String>>()
                .map(Some);
        }
        match node.attributes.get(attribute) {
            None => Ok(None),
            Some(Attribute::Integers(v)) => Ok(Some(v.clone())),
            _ => Err("invalid integer-list node attribute".into()),
        }
    }
    pub fn lower(model: &Model) -> Result<Self, String> {
        Self::lower_bindings(model, false)
    }
    /// Match offline_onnx.load_model external bindings: symbolic dimensions
    /// become one (unknown dimensions still fail the original runner check), half inputs use float32, and all outputs use float32.
    /// This constructs typed IR only; it does not initialize a device backend.
    pub fn lower_offline(model: &Model) -> Result<Self, String> {
        Self::lower_bindings(model, true)
    }
    fn lower_bindings(model: &Model, offline: bool) -> Result<Self, String> {
        model.admit_offline()?;
        let mut graph = Self {
            offline_images: offline,
            ..Self::default()
        };
        for (name, tensor) in &model.graph.initializers {
            let shape = tensor
                .shape
                .iter()
                .map(|&v| usize::try_from(v).map_err(|_| "negative constant dimension".into()))
                .collect::<Result<Vec<_>, String>>()?;
            let ty = TensorType {
                dtype: tensor.dtype,
                shape,
            };
            if ty.bytes()? != tensor.data.len() {
                return Err("constant storage differs from dtype/shape".into());
            }
            graph.push(name, ty, ScalarExpr::Constant(tensor.data.clone()))?;
        }
        for input in &model.graph.inputs {
            if model.graph.initializers.contains_key(&input.name) {
                continue;
            }
            if input.optional || input.sequence {
                return Err("offline ONNX input requires a single mandatory Tensor".into());
            }
            let shape = input
                .shape
                .iter()
                .map(|v| match v {
                    ShapeDimension::Fixed(v) => {
                        usize::try_from(*v).map_err(|_| "negative runtime dimension".into())
                    }
                    ShapeDimension::Symbol(_) if offline => Ok(1),
                    ShapeDimension::Unknown if offline => {
                        Err("original offline runner rejects unknown input dimensions".into())
                    }
                    _ => Err("native lowering requires concrete input dimensions".into()),
                })
                .collect::<Result<Vec<_>, String>>()?;
            graph.push(
                &input.name,
                TensorType {
                    dtype: if offline && input.dtype == 10 {
                        1
                    } else {
                        input.dtype
                    },
                    shape,
                },
                ScalarExpr::Input(input.name.clone()),
            )?;
        }
        for node in &model.graph.nodes {
            graph.lower_node(model, node)?;
        }
        for output in &model.graph.outputs {
            let mut id = *graph
                .names
                .get(&output.name)
                .ok_or("model output has no lowered tensor")?;
            if offline && graph.plans[id].ty.dtype != 1 {
                let shape = graph.plans[id].ty.shape.clone();
                id = graph.temporary(
                    TensorType { dtype: 1, shape },
                    ScalarExpr::Cast {
                        dtype: 1,
                        value: Box::new(ScalarExpr::Load {
                            plan: id,
                            index: IndexExpr::Index,
                        }),
                    },
                )?;
            }
            graph.outputs.insert(output.name.clone(), id);
        }
        Ok(graph)
    }
    fn lower_node(&mut self, model: &Model, node: &Node) -> Result<(), String> {
        if node.op == "LayerNormalization" {
            return self.lower_normalization(model, node);
        }
        if node.outputs.len() != 1 {
            return Err(format!(
                "native lowering for {} output arity not implemented",
                node.op
            ));
        }
        if !matches!(
            node.op.as_str(),
            "Identity"
                | "Cast"
                | "Add"
                | "Sub"
                | "Mul"
                | "Div"
                | "Reshape"
                | "Transpose"
                | "Expand"
                | "Squeeze"
                | "Unsqueeze"
                | "Slice"
                | "Concat"
                | "Where"
                | "Relu"
                | "Clip"
                | "ReduceMean"
                | "ReduceL2"
                | "MatMul"
                | "Gemm"
                | "Sigmoid"
                | "Gelu"
                | "Gather"
                | "GatherND"
                | "Softmax"
                | "Conv"
        ) {
            return Err(format!(
                "native tensor lowering not implemented for {}",
                node.op
            ));
        }
        let a = self.input(node, 0)?;
        let input = self.plans[a].ty.clone();
        let direct = || ScalarExpr::Load {
            plan: a,
            index: IndexExpr::Index,
        };
        let (ty, expression) = match node.op.as_str() {
            "Identity" => (input, direct()),
            "Cast" => {
                let dtype = match node.attributes.get("to") {
                    Some(Attribute::Integer(v)) => {
                        u32::try_from(*v).map_err(|_| "invalid Cast dtype")?
                    }
                    _ => return Err("Cast requires to attribute".into()),
                };
                super::parse::width(dtype)?;
                (
                    TensorType {
                        dtype,
                        shape: input.shape,
                    },
                    ScalarExpr::Cast {
                        dtype,
                        value: Box::new(direct()),
                    },
                )
            }
            "Add" | "Sub" | "Mul" | "Div" => {
                let b = self.input(node, 1)?;
                let other = &self.plans[b].ty;
                let shape = broadcast(&[&input.shape, &other.shape])?;
                let mut dtype = scalar::promote(input.dtype, other.dtype)?;
                let left = Box::new(self.load(a, &shape));
                let mut right = Box::new(self.load(b, &shape));
                let mut kind = match node.op.as_str() {
                    "Add" => Binary::Add,
                    "Sub" => Binary::Sub,
                    "Mul" => Binary::Mul,
                    _ => Binary::DivTrunc,
                };
                if node.op == "Sub" && dtype == 9 {
                    kind = Binary::Add;
                    right = Box::new(ScalarExpr::BoolNot(right));
                }
                let numerator_integer = matches!(input.dtype, 2 | 3 | 4 | 5 | 6 | 7 | 12 | 13);
                let floating_division =
                    node.op == "Div" && (!numerator_integer || matches!(dtype, 1 | 10 | 11 | 16));
                if floating_division {
                    if !matches!(dtype, 1 | 10 | 11 | 16) {
                        dtype = scalar::promote(dtype, 1)?;
                    }
                    kind = Binary::Mul;
                    right = Box::new(ScalarExpr::Reciprocal {
                        dtype,
                        value: right,
                    });
                }
                let mut expression = ScalarExpr::Binary {
                    kind,
                    dtype,
                    left,
                    right,
                };
                // Onnx Div chooses truncation from the numerator's original
                // dtype before promotion; integer/float therefore keeps a
                // floating output with its fractional part truncated.
                if floating_division && numerator_integer {
                    expression = ScalarExpr::Trunc {
                        dtype,
                        value: Box::new(expression),
                    };
                }
                (TensorType { dtype, shape }, expression)
            }
            "Transpose" => {
                let perm = match node.attributes.get("perm") {
                    Some(Attribute::Integers(v)) if !v.is_empty() => v.clone(),
                    None | Some(Attribute::Integers(_)) => {
                        (0..input.shape.len() as i64).rev().collect()
                    }
                    _ => return Err("invalid Transpose perm".into()),
                };
                let order = perm
                    .iter()
                    .map(|&v| axis(v, input.shape.len()))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut sorted = order.clone();
                sorted.sort_unstable();
                if sorted != (0..input.shape.len()).collect::<Vec<_>>() {
                    return Err("order is not a valid permutation".into());
                }
                let shape = order.iter().map(|&i| input.shape[i]).collect::<Vec<_>>();
                let index = index_from_coordinates(
                    &input.shape,
                    (0..input.shape.len())
                        .map(|i| coordinate(&shape, order.iter().position(|&v| v == i).unwrap())),
                );
                (
                    TensorType {
                        dtype: input.dtype,
                        shape,
                    },
                    ScalarExpr::Load { plan: a, index },
                )
            }
            "Reshape" => {
                let shape = self
                    .control(model, node, 1, "shape")?
                    .ok_or("Reshape shape missing")?;
                let allowzero =
                    matches!(node.attributes.get("allowzero"),Some(Attribute::Integer(v))if *v!=0);
                let mut resolved = Vec::new();
                let mut inferred = None;
                let mut known = 1usize;
                for (i, &size) in shape.iter().enumerate() {
                    if size == -1 {
                        if inferred.replace(i).is_some() {
                            return Err("only one dimension can be inferred using -1".into());
                        }
                        resolved.push(1);
                    } else {
                        let size = if size == 0 && !allowzero {
                            *input
                                .shape
                                .get(i)
                                .ok_or("reshape zero axis outside input")?
                        } else {
                            usize::try_from(size).map_err(|_| "invalid reshape dimension")?
                        };
                        known = known.checked_mul(size).ok_or("reshape size overflow")?;
                        resolved.push(size);
                    }
                }
                let count = input.count()?;
                if let Some(i) = inferred {
                    if known == 0 || count % known != 0 {
                        return Err("size mismatch in inferred reshape".into());
                    }
                    resolved[i] = count / known;
                } else if count != known {
                    return Err("size mismatch in reshape".into());
                }
                (
                    TensorType {
                        dtype: input.dtype,
                        shape: resolved,
                    },
                    direct(),
                )
            }
            "Expand" | "Squeeze" | "Unsqueeze" | "Slice" | "Concat" | "Where" | "Relu" | "Clip" => {
                self.lower_static(model, node, a, &input)?
            }
            "ReduceMean" | "ReduceL2" => self.lower_reduction(model, node, a, &input)?,
            "MatMul" => {
                let b = self.input(node, 1)?;
                self.lower_matmul(a, b)?
            }
            "Gemm" => self.lower_gemm(node, a)?,
            "Conv" => self.lower_convolution(node, a)?,
            "Sigmoid" | "Gelu" => self.lower_activation(node, a, &input)?,
            "Gather" | "GatherND" => self.lower_gather(model, node, a, &input)?,
            "Softmax" => self.lower_softmax(node, a, &input)?,
            _ => unreachable!(),
        };
        self.push(&node.outputs[0], ty, expression)?;
        Ok(())
    }
    fn lower_static(
        &self,
        model: &Model,
        node: &Node,
        a: usize,
        input: &TensorType,
    ) -> Result<(TensorType, ScalarExpr), String> {
        let direct = || ScalarExpr::Load {
            plan: a,
            index: IndexExpr::Index,
        };
        let mut ty = input.clone();
        let expression = match node.op.as_str() {
            "Expand" => {
                let target = self
                    .control(model, node, 1, "shape")?
                    .ok_or("Expand shape missing")?
                    .into_iter()
                    .map(|v| usize::try_from(v).map_err(|_| "negative Expand dimension".into()))
                    .collect::<Result<Vec<_>, String>>()?;
                ty.shape = broadcast(&[&input.shape, &target])?;
                self.load(a, &ty.shape)
            }
            "Squeeze" => {
                if let Some(mut axes) = self.control(model, node, 1, "axes")? {
                    axes.sort_unstable_by(|a, b| b.cmp(a));
                    for v in axes {
                        if ty.shape.is_empty() && matches!(v, -1 | 0) {
                            continue;
                        }
                        let i = axis(v, ty.shape.len())?;
                        if ty.shape[i] == 1 {
                            ty.shape.remove(i);
                        }
                    }
                } else {
                    ty.shape.retain(|&d| d != 1);
                }
                direct()
            }
            "Unsqueeze" => {
                let mut axes = self
                    .control(model, node, 1, "axes")?
                    .ok_or("Unsqueeze axes missing")?;
                axes.sort_unstable();
                for v in axes {
                    let i = axis(v, ty.shape.len() + 1)?;
                    ty.shape.insert(i, 1);
                }
                direct()
            }
            "Slice" => {
                let starts = self
                    .control(model, node, 1, "starts")?
                    .ok_or("Slice starts missing")?;
                let ends = self
                    .control(model, node, 2, "ends")?
                    .ok_or("Slice ends missing")?;
                let axes = self
                    .control(model, node, 3, "axes")?
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| (0..input.shape.len() as i64).collect());
                let steps = self
                    .control(model, node, 4, "steps")?
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| vec![1; input.shape.len()]);
                let mut ranges = input
                    .shape
                    .iter()
                    .map(|&n| (0i128, 1i128, n))
                    .collect::<Vec<_>>();
                for (i, v) in axes.into_iter().enumerate() {
                    let dim = axis(v, input.shape.len())?;
                    let start = *starts.get(i).ok_or("Slice starts length mismatch")? as i128;
                    let end = *ends.get(i).ok_or("Slice ends length mismatch")? as i128;
                    let step = *steps.get(i).ok_or("Slice steps length mismatch")? as i128;
                    if step == 0 {
                        return Err("slice step cannot be zero".into());
                    }
                    let size = input.shape[dim] as i128;
                    let normalize = |v: i128| {
                        let value = if v < 0 { v + size } else { v };
                        if step > 0 {
                            value.clamp(0, size)
                        } else {
                            value.clamp(-1, size - 1)
                        }
                    };
                    let (start, end) = (normalize(start), normalize(end));
                    let count = if step > 0 {
                        if end > start {
                            (end - start - 1) / step + 1
                        } else {
                            0
                        }
                    } else if start > end {
                        (start - end - 1) / (-step) + 1
                    } else {
                        0
                    };
                    ranges[dim] = (start, step, count as usize);
                }
                ty.shape = ranges.iter().map(|v| v.2).collect();
                let index = index_from_coordinates(
                    &input.shape,
                    ranges.iter().enumerate().map(|(axis, (start, step, _))| {
                        add(
                            IndexExpr::Constant(*start),
                            IndexExpr::Mul(Box::new(coordinate(&ty.shape, axis)), *step),
                        )
                    }),
                );
                ScalarExpr::Load { plan: a, index }
            }
            "Concat" => {
                let dim = match node.attributes.get("axis") {
                    Some(Attribute::Integer(v)) => axis(*v, input.shape.len())?,
                    _ => return Err("Concat axis missing".into()),
                };
                let ids = (0..node.inputs.len())
                    .map(|i| self.input(node, i))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut length = 0usize;
                let mut offsets = Vec::new();
                for &id in &ids {
                    let other = &self.plans[id].ty;
                    if other.shape.len() != input.shape.len()
                        || other
                            .shape
                            .iter()
                            .enumerate()
                            .any(|(i, &n)| i != dim && n != input.shape[i])
                    {
                        return Err("Concat shape mismatch".into());
                    }
                    offsets.push(length);
                    length = length
                        .checked_add(other.shape[dim])
                        .ok_or("Concat dimension overflow")?;
                    ty.dtype = scalar::promote(ty.dtype, other.dtype)?;
                }
                ty.shape[dim] = length;
                let mut result = None;
                for (&id, &offset) in ids.iter().zip(&offsets).rev() {
                    let source = &self.plans[id].ty.shape;
                    let index = index_from_coordinates(
                        source,
                        (0..source.len()).map(|i| {
                            if i == dim {
                                add(
                                    coordinate(&ty.shape, i),
                                    IndexExpr::Constant(-(offset as i128)),
                                )
                            } else {
                                coordinate(&ty.shape, i)
                            }
                        }),
                    );
                    let load = ScalarExpr::Load { plan: id, index };
                    result = Some(match result {
                        None => load,
                        Some(no) => ScalarExpr::Select {
                            condition: Box::new(ScalarExpr::IndexLess {
                                index: coordinate(&ty.shape, dim),
                                bound: (offset + source[dim]) as i128,
                            }),
                            yes: Box::new(load),
                            no: Box::new(no),
                        },
                    });
                }
                result.ok_or("Concat requires tensors")?
            }
            "Where" => {
                let yes = self.input(node, 1)?;
                let no = self.input(node, 2)?;
                ty.dtype = scalar::promote(self.plans[yes].ty.dtype, self.plans[no].ty.dtype)?;
                ty.shape = broadcast(&[
                    &input.shape,
                    &self.plans[yes].ty.shape,
                    &self.plans[no].ty.shape,
                ])?;
                ScalarExpr::Select {
                    condition: Box::new(self.load(a, &ty.shape)),
                    yes: Box::new(self.load(yes, &ty.shape)),
                    no: Box::new(self.load(no, &ty.shape)),
                }
            }
            "Relu" => {
                // The original where(self, 0) promotes bool with the Python
                // integer zero to int32, rather than retaining bool output.
                if ty.dtype == 9 {
                    ty.dtype = 6;
                }
                ScalarExpr::Select {
                    condition: Box::new(ScalarExpr::GreaterZero(Box::new(direct()))),
                    yes: Box::new(direct()),
                    no: Box::new(ScalarExpr::Literal(Scalar::from_integer(ty.dtype, 0)?)),
                }
            }
            "Clip" => {
                enum Bound {
                    Tensor(usize),
                    Scalar(Scalar),
                }
                let mut bounds = Vec::new();
                for (i, name) in [(1, "min"), (2, "max")] {
                    if node.inputs.len() > i && node.attributes.contains_key(name) {
                        return Err(
                            "Clip bound supplied as both positional input and attribute".into()
                        );
                    }
                    if node.inputs.get(i).is_some_and(|n| !n.is_empty()) {
                        let id = self.input(node, i)?;
                        ty.dtype = scalar::promote(ty.dtype, self.plans[id].ty.dtype)?;
                        bounds.push((i, Bound::Tensor(id)));
                    } else if let Some(attribute) = node.attributes.get(name) {
                        let literal = match attribute {
                            Attribute::Float(v) => Scalar::from_float(
                                if matches!(ty.dtype, 1 | 10 | 11 | 16) {
                                    ty.dtype
                                } else {
                                    1
                                },
                                *v as f64,
                            )?,
                            Attribute::Integer(v) => Scalar::from_integer(
                                if ty.dtype == 9 { 6 } else { ty.dtype },
                                *v as i128,
                            )?,
                            _ => return Err("invalid scalar Clip attribute".into()),
                        };
                        ty.dtype = scalar::promote(ty.dtype, literal.dtype)?;
                        bounds.push((i, Bound::Scalar(literal)));
                    }
                }
                let mut shapes = vec![input.shape.as_slice()];
                for (_, bound) in &bounds {
                    if let Bound::Tensor(id) = bound {
                        shapes.push(self.plans[*id].ty.shape.as_slice());
                    }
                }
                ty.shape = broadcast(&shapes)?;
                let mut result = self.load(a, &ty.shape);
                for (i, bound) in bounds {
                    let right = match bound {
                        Bound::Tensor(id) => self.load(id, &ty.shape),
                        Bound::Scalar(value) => ScalarExpr::Literal(value),
                    };
                    result = ScalarExpr::Binary {
                        kind: if i == 1 {
                            Binary::Maximum
                        } else {
                            Binary::Minimum
                        },
                        dtype: ty.dtype,
                        left: Box::new(result),
                        right: Box::new(right),
                    };
                }
                result
            }
            _ => unreachable!(),
        };
        Ok((ty, expression))
    }
    pub fn evaluate(
        &self,
        inputs: &BTreeMap<String, Tensor>,
    ) -> Result<BTreeMap<String, Tensor>, String> {
        let mut values = Vec::<Tensor>::new();
        let mut total = 0usize;
        for plan in &self.plans {
            let bytes = plan.ty.bytes()?;
            total = total
                .checked_add(bytes)
                .filter(|&n| n <= 4 * 1024 * 1024 * 1024)
                .ok_or("native reference executor exceeds 4 GiB")?;
            let shape = plan.ty.shape.iter().map(|&v| v as i64).collect::<Vec<_>>();
            let data = match &plan.expression {
                ScalarExpr::Input(name) => {
                    let value = inputs.get(name).ok_or("runtime tensor input missing")?;
                    if value.dtype != plan.ty.dtype
                        || value.shape != shape
                        || value.data.len() != bytes
                    {
                        return Err("runtime tensor differs from lowered input type".into());
                    }
                    value.data.clone()
                }
                ScalarExpr::Constant(data) => data.clone(),
                expression => {
                    let mut data = Vec::with_capacity(bytes);
                    for index in 0..plan.ty.count()? {
                        data.extend(
                            eval(expression, index, &values)?
                                .cast(plan.ty.dtype)?
                                .bytes()?,
                        );
                    }
                    data
                }
            };
            values.push(Tensor {
                dtype: plan.ty.dtype,
                shape,
                data,
            });
        }
        self.outputs
            .iter()
            .map(|(name, &id)| {
                Ok((
                    name.clone(),
                    values
                        .get(id)
                        .ok_or("lowered output index invalid")?
                        .clone(),
                ))
            })
            .collect()
    }
}
fn eval(expression: &ScalarExpr, index: usize, values: &[Tensor]) -> Result<Scalar, String> {
    eval_at(expression, index, values, 0)
}
fn eval_at(
    expression: &ScalarExpr,
    index: usize,
    values: &[Tensor],
    reduction: usize,
) -> Result<Scalar, String> {
    match expression {
        ScalarExpr::Load { plan, index: map } => {
            let tensor = values.get(*plan).ok_or("scalar load precedes tensor")?;
            let at = usize::try_from(map.eval(index, reduction))
                .map_err(|_| "negative scalar load index")?;
            let width = super::parse::width(tensor.dtype)?;
            let offset = at.checked_mul(width).ok_or("scalar load index overflow")?;
            Scalar::read(
                tensor.dtype,
                tensor
                    .data
                    .get(offset..offset + width)
                    .ok_or("scalar load outside storage")?,
            )
        }
        ScalarExpr::PaddedLoad { plan, coordinates } => {
            let tensor = values.get(*plan).ok_or("padded load precedes tensor")?;
            let mut position = 0usize;
            for ((coordinate, low, high), size) in coordinates.iter().zip(&tensor.shape) {
                let at = coordinate.eval(index, reduction);
                if at < *low as i128 || at >= *high as i128 {
                    return Scalar::from_integer(tensor.dtype, 0);
                }
                position = position
                    .checked_mul(*size as usize)
                    .and_then(|v| v.checked_add(at as usize))
                    .ok_or("padded load overflow")?;
            }
            let width = super::parse::width(tensor.dtype)?;
            Scalar::read(
                tensor.dtype,
                tensor
                    .data
                    .get(position * width..(position + 1) * width)
                    .ok_or("padded load outside storage")?,
            )
        }
        ScalarExpr::GatherLoad { plan, base, axes } => {
            let tensor = values.get(*plan).ok_or("indirect load precedes tensor")?;
            let mut position = base.eval(index, reduction);
            for (expression, size, stride) in axes {
                let mut at = eval_at(expression, index, values, reduction)?.integer()?;
                if at < 0 {
                    at += *size as i128;
                }
                if at < 0 || at >= *size as i128 {
                    return Scalar::from_integer(tensor.dtype, 0);
                }
                position += at * (*stride as i128);
            }
            let width = super::parse::width(tensor.dtype)?;
            let position = usize::try_from(position)
                .ok()
                .and_then(|v| v.checked_mul(width))
                .ok_or("invalid indirect load index")?;
            Scalar::read(
                tensor.dtype,
                tensor
                    .data
                    .get(position..position + width)
                    .ok_or("indirect load outside storage")?,
            )
        }
        ScalarExpr::Cast { dtype, value } | ScalarExpr::StorageRound { dtype, value } => {
            eval_at(value, index, values, reduction)?.cast(*dtype)
        }
        ScalarExpr::Binary {
            kind,
            dtype,
            left,
            right,
        } => scalar::binary(
            *kind,
            *dtype,
            eval_at(left, index, values, reduction)?,
            eval_at(right, index, values, reduction)?,
        ),
        ScalarExpr::Literal(value) => Ok(*value),
        ScalarExpr::FloatLiteral { dtype, value } => Scalar::floating_literal(*dtype, *value),
        ScalarExpr::GreaterZero(value) => {
            let v = eval_at(value, index, values, reduction)?;
            Scalar::from_integer(
                9,
                i128::from(if v.floating() {
                    v.float()? > 0.
                } else {
                    v.integer()? > 0
                }),
            )
        }
        ScalarExpr::IndexLess { index: map, bound } => {
            Scalar::from_integer(9, i128::from(map.eval(index, reduction) < *bound))
        }
        ScalarExpr::Select { condition, yes, no } => {
            if eval_at(condition, index, values, reduction)?.truth()? {
                eval_at(yes, index, values, reduction)
            } else {
                eval_at(no, index, values, reduction)
            }
        }
        ScalarExpr::MulAcc { dtype, a, b, c } => {
            let a = eval_at(a, index, values, reduction)?
                .cast(*dtype)?
                .float()?;
            let b = eval_at(b, index, values, reduction)?
                .cast(*dtype)?
                .float()?;
            let c = eval_at(c, index, values, reduction)?
                .cast(*dtype)?
                .float()?;
            Scalar::from_float(*dtype, a * b + c)
        }
        ScalarExpr::Exp2 { dtype, value } => Scalar::from_float(
            *dtype,
            2f64.powf(eval_at(value, index, values, reduction)?.float()?),
        ),
        ScalarExpr::Sign { dtype, value } => {
            let v = eval_at(value, index, values, reduction)?;
            let sign = if v.floating() {
                let f = v.float()?;
                if f == 0. {
                    0
                } else if f < 0. {
                    -1
                } else {
                    1
                }
            } else {
                v.integer()?.signum()
            };
            Scalar::from_integer(*dtype, sign)
        }
        ScalarExpr::Sqrt { dtype, value } => {
            let value = eval_at(value, index, values, reduction)?.float()?;
            Scalar::from_float(*dtype, if value < 0. { f64::NAN } else { value.sqrt() })
        }
        ScalarExpr::ReduceMax {
            dtype,
            count,
            value,
        } => {
            let mut maximum = if matches!(dtype, 1 | 10 | 11 | 16) {
                Scalar::from_float(*dtype, f64::NEG_INFINITY)?
            } else {
                Scalar::from_integer(
                    *dtype,
                    if matches!(dtype, 3 | 5 | 6 | 7) {
                        -(1i128 << (super::parse::width(*dtype)? * 8 - 1))
                    } else {
                        0
                    },
                )?
            };
            for lane in 0..*count {
                maximum = scalar::binary(
                    Binary::Maximum,
                    *dtype,
                    maximum,
                    eval_at(value, index, values, lane)?,
                )?;
            }
            Ok(maximum)
        }
        ScalarExpr::ReduceSum {
            dtype,
            count,
            value,
        } => {
            let mut sum = Scalar::from_integer(*dtype, 0)?;
            for lane in 0..*count {
                sum = scalar::binary(
                    Binary::Add,
                    *dtype,
                    sum,
                    eval_at(value, index, values, lane)?,
                )?;
            }
            Ok(sum)
        }
        ScalarExpr::Trunc { dtype, value } => {
            let value = eval_at(value, index, values, reduction)?.float()?;
            if !value.is_finite() {
                return Err("original truncation requires finite value".into());
            }
            Scalar::from_float(*dtype, value.trunc())
        }
        ScalarExpr::BoolNot(value) => Scalar::from_integer(
            9,
            i128::from(!eval_at(value, index, values, reduction)?.truth()?),
        ),
        ScalarExpr::Reciprocal { dtype, value } => {
            let value = eval_at(value, index, values, reduction)?
                .cast(*dtype)?
                .float()?;
            Scalar::from_float(*dtype, 1. / value)
        }
        _ => Err("storage expression used inside scalar map".into()),
    }
}
