use super::{
    protobuf::{fields, integers, Wire},
    *,
};
use std::io::{Read, Seek};
fn entry(bytes: &[u8]) -> Result<(String, String), String> {
    let (mut k, mut v) = (String::new(), String::new());
    for (f, w) in fields(bytes)? {
        match f {
            1 => k = w.string()?,
            2 => v = w.string()?,
            _ => (),
        }
    }
    Ok((k, v))
}
pub(super) fn model(bytes: &[u8], origin: Option<&Path>) -> Result<Model, String> {
    if bytes.len() > 4 * 1024 * 1024 * 1024 {
        return Err("ONNX source exceeds 4 GiB".into());
    }
    let (mut graph, mut opsets, mut metadata_properties) = (None, BTreeMap::new(), BTreeMap::new());
    for (f, w) in fields(bytes)? {
        match f {
            7 => graph = Some(parse_graph(w.bytes()?, origin)?),
            8 => {
                let (mut domain, mut version) = (String::new(), 1);
                for (f, w) in fields(w.bytes()?)? {
                    match f {
                        1 => domain = w.string()?,
                        2 => version = w.integer()? as i64,
                        _ => (),
                    }
                }
                opsets.insert(domain, version);
            }
            14 => {
                let (k, v) = entry(w.bytes()?)?;
                metadata_properties.entry(k).or_insert(v);
            }
            _ => (),
        }
    }
    let mut graph = graph.ok_or("ONNX graph missing")?;
    for node in &mut graph.nodes {
        node.opset = *opsets
            .get(&node.domain)
            .or_else(|| {
                if node.domain == "ai.onnx" {
                    opsets.get("")
                } else {
                    None
                }
            })
            .unwrap_or(&1);
    }
    // Initializers listed as graph inputs are constants, matching OnnxRunner.
    graph
        .inputs
        .retain(|v| !graph.initializers.contains_key(&v.name));
    Ok(Model {
        graph,
        opsets,
        metadata_properties,
    })
}
fn parse_graph(bytes: &[u8], origin: Option<&Path>) -> Result<Graph, String> {
    let mut graph = Graph::default();
    for (f, w) in fields(bytes)? {
        match f {
            1 => graph.nodes.push(node(w.bytes()?, origin)?),
            5 => {
                let (name, tensor) = tensor(w.bytes()?, origin)?;
                if graph.initializers.insert(name, tensor).is_some() {
                    return Err("duplicate ONNX initializer".into());
                }
            }
            11 => graph.inputs.push(value(w.bytes()?)?),
            12 => graph.outputs.push(value(w.bytes()?)?),
            _ => (),
        }
    }
    Ok(graph)
}
fn value(bytes: &[u8]) -> Result<Value, String> {
    let mut result = Value {
        name: String::new(),
        dtype: 0,
        shape: Vec::new(),
        optional: false,
        sequence: false,
    };
    let mut kind = None;
    for (f, w) in fields(bytes)? {
        match f {
            1 => result.name = w.string()?,
            2 => kind = Some(w.bytes()?),
            _ => (),
        }
    }
    value_type(kind.ok_or("ONNX value type missing")?, &mut result, 0)?;
    width(result.dtype)?;
    Ok(result)
}
fn value_type(bytes: &[u8], value: &mut Value, depth: usize) -> Result<(), String> {
    if depth > 16 {
        return Err("nested ONNX type exceeds limit".into());
    }
    for (f, w) in fields(bytes)? {
        match f {
            1 => {
                for (f, w) in fields(w.bytes()?)? {
                    match f {
                        1 => {
                            value.dtype =
                                u32::try_from(w.integer()?).map_err(|_| "invalid ONNX dtype")?
                        }
                        2 => {
                            for (f, w) in fields(w.bytes()?)? {
                                if f == 1 {
                                    let mut dimension = ShapeDimension::Unknown;
                                    for (f, w) in fields(w.bytes()?)? {
                                        match f {
                                            1 => {
                                                dimension =
                                                    ShapeDimension::Fixed(w.integer()? as i64)
                                            }
                                            2 => {
                                                let s = w.string()?;
                                                if !s.is_empty() {
                                                    dimension = ShapeDimension::Symbol(s);
                                                }
                                            }
                                            _ => (),
                                        }
                                    }
                                    value.shape.push(dimension);
                                }
                            }
                        }
                        _ => (),
                    }
                }
            }
            4 | 9 => {
                if f == 4 {
                    value.sequence = true;
                } else {
                    value.optional = true;
                }
                for (f, w) in fields(w.bytes()?)? {
                    if f == 1 {
                        value_type(w.bytes()?, value, depth + 1)?;
                    }
                }
            }
            _ => (),
        }
    }
    Ok(())
}
fn node(bytes: &[u8], origin: Option<&Path>) -> Result<Node, String> {
    let mut node = Node {
        name: String::new(),
        op: String::new(),
        domain: String::new(),
        opset: 1,
        inputs: Vec::new(),
        outputs: Vec::new(),
        attributes: BTreeMap::new(),
    };
    for (f, w) in fields(bytes)? {
        match f {
            1 => node.inputs.push(w.string()?),
            2 => node.outputs.push(w.string()?),
            3 => node.name = w.string()?,
            4 => node.op = w.string()?,
            5 => {
                let (k, v) = attribute(w.bytes()?, origin)?;
                node.attributes.insert(k, v);
            }
            7 => node.domain = w.string()?,
            _ => (),
        }
    }
    if node.op.is_empty() {
        return Err("ONNX node operator missing".into());
    }
    Ok(node)
}
fn floats(w: Wire<'_>) -> Result<Vec<f32>, String> {
    match w {
        Wire::Fixed32(v) => Ok(vec![f32::from_bits(v)]),
        Wire::Bytes(b) => {
            if b.len() % 4 != 0 {
                return Err("invalid packed ONNX floats".into());
            }
            Ok(b.chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect())
        }
        _ => Err("expected ONNX float".into()),
    }
}
fn attribute(bytes: &[u8], origin: Option<&Path>) -> Result<(String, Attribute), String> {
    let (mut name, mut kind) = (String::new(), 0);
    let (mut scalar, mut fv, mut iv, mut sv) = (None, Vec::new(), Vec::new(), Vec::new());
    for (f, w) in fields(bytes)? {
        match f {
            1 => name = w.string()?,
            2 => {
                let Wire::Fixed32(value) = w else {
                    return Err("invalid scalar ONNX float attribute".into());
                };
                scalar = Some(Attribute::Float(f32::from_bits(value)));
            }
            3 => scalar = Some(Attribute::Integer(w.integer()? as i64)),
            4 => scalar = Some(Attribute::String(w.string()?)),
            5 => scalar = Some(Attribute::Tensor(tensor(w.bytes()?, origin)?.1)),
            6 => scalar = Some(Attribute::Graph(w.bytes()?.to_vec())),
            7 => fv.extend(floats(w)?),
            8 => iv.extend(integers(w)?),
            9 => sv.push(w.string()?),
            20 => kind = w.integer()?,
            _ => (),
        }
    }
    let output = match kind {
        1..=5 => {
            let value = scalar.ok_or("ONNX attribute value missing")?;
            if !matches!(
                (kind, &value),
                (1, Attribute::Float(_))
                    | (2, Attribute::Integer(_))
                    | (3, Attribute::String(_))
                    | (4, Attribute::Tensor(_))
                    | (5, Attribute::Graph(_))
            ) {
                return Err("ONNX attribute kind disagrees with value".into());
            }
            value
        }
        6 => Attribute::Floats(fv),
        7 => Attribute::Integers(iv),
        8 => Attribute::Strings(sv),
        _ => return Err("unsupported ONNX attribute type".into()),
    };
    Ok((name, output))
}
pub(super) fn width(dtype: u32) -> Result<usize, String> {
    match dtype {
        1 | 6 | 12 => Ok(4),
        2 | 3 | 9 => Ok(1),
        4 | 5 | 10 | 16 => Ok(2),
        7 | 11 | 13 => Ok(8),
        _ => Err("unsupported ONNX tensor dtype".into()),
    }
}
fn tensor(bytes: &[u8], origin: Option<&Path>) -> Result<(String, Tensor), String> {
    let (mut name, mut dtype, mut shape, mut location) = (String::new(), 0, Vec::new(), 0);
    let (mut external, mut data_fields) = (BTreeMap::new(), BTreeMap::<u32, Vec<u8>>::new());
    let mut typed_ints = BTreeMap::<u32, Vec<i64>>::new();
    for (f, w) in fields(bytes)? {
        match f {
            1 => shape.extend(integers(w)?),
            2 => dtype = u32::try_from(w.integer()?).map_err(|_| "invalid ONNX dtype")?,
            8 => name = w.string()?,
            9 => {
                data_fields.insert(f, w.bytes()?.to_vec());
            }
            4 | 10 => {
                let target = data_fields.entry(f).or_default();
                match w {
                    Wire::Fixed32(v) => target.extend_from_slice(&v.to_le_bytes()),
                    Wire::Fixed64(v) => target.extend_from_slice(&v.to_le_bytes()),
                    Wire::Bytes(b) => target.extend_from_slice(b),
                    _ => return Err("invalid ONNX typed float data".into()),
                }
            }
            5 | 7 | 11 => typed_ints.entry(f).or_default().extend(integers(w)?),
            13 => {
                let (k, v) = entry(w.bytes()?)?;
                external.insert(k, v);
            }
            14 => location = w.integer()?,
            _ => (),
        }
    }
    let size = width(dtype)?;
    let count = shape.iter().try_fold(1usize, |n, &d| {
        let d = usize::try_from(d).map_err(|_| "negative ONNX tensor dimension")?;
        n.checked_mul(d).ok_or("ONNX tensor shape overflow")
    })?;
    let expected = count.checked_mul(size).ok_or("ONNX tensor byte overflow")?;
    if expected > 4 * 1024 * 1024 * 1024 {
        return Err("ONNX tensor exceeds 4 GiB".into());
    }
    if location == 1 {
        let origin = origin.ok_or("onnx external_data needs the origin file path")?;
        let path = external
            .get("location")
            .ok_or("no location in external_data")?;
        let offset = external
            .get("offset")
            .map(|s| s.parse::<u64>())
            .transpose()
            .map_err(|_| "invalid external offset")?
            .unwrap_or(0);
        let mut file = std::fs::File::open(origin.parent().ok_or("ONNX parent absent")?.join(path))
            .map_err(|e| e.to_string())?;
        let length = external
            .get("length")
            .map(|s| s.parse::<u64>())
            .transpose()
            .map_err(|_| "invalid external length")?
            .unwrap_or(
                file.metadata()
                    .map_err(|e| e.to_string())?
                    .len()
                    .checked_sub(offset)
                    .ok_or("external offset exceeds file")?,
            );
        if length != expected as u64 {
            return Err("external ONNX tensor byte length mismatch".into());
        }
        file.seek(std::io::SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let mut data = vec![0; expected];
        file.read_exact(&mut data).map_err(|e| e.to_string())?;
        data_fields.insert(9, data);
    }
    if data_fields.len() + typed_ints.len() != 1 {
        return Err("only 1 ONNX data field is allowed".into());
    }
    let data = if let Some((_, data)) = data_fields.into_iter().next() {
        // The pinned original takes its scalar .item() path here, whose
        // memoryview format does not support BF16. Preserve that rejection;
        // numeric typed-list BF16 initializers take a different source path.
        if dtype == 16 && shape.is_empty() {
            return Err("original ONNX parser does not support raw scalar BF16".into());
        }
        data
    } else {
        let (_, values) = typed_ints.into_iter().next().unwrap();
        let mut output = Vec::with_capacity(expected);
        for v in values {
            // Preserve the original parser's numeric list casts, including
            // FP16 int32_data; do not substitute ONNX bit reinterpretation.
            match dtype {
                1 => output.extend_from_slice(&(v as f32).to_le_bytes()),
                11 => output.extend_from_slice(&(v as f64).to_le_bytes()),
                10 => output.extend_from_slice(&integer_half(v).to_le_bytes()),
                16 => {
                    // _frompy first packs float32, then casts to BF16 with
                    // round-to-nearest ties-to-even (dtype.float_to_bf16).
                    let bits = (v as f32).to_bits();
                    let rounded = bits.wrapping_add(0x7fff + ((bits >> 16) & 1));
                    output.extend_from_slice(&((rounded >> 16) as u16).to_le_bytes());
                }
                9 => output.push(u8::from(v != 0)),
                _ => output.extend_from_slice(&v.to_le_bytes()[..size]),
            }
        }
        output
    };
    if data.len() != expected {
        return Err("ONNX tensor byte length differs from dtype/shape".into());
    }
    Ok((name, Tensor { dtype, shape, data }))
}

fn integer_half(value: i64) -> u16 {
    let sign = if value < 0 { 0x8000 } else { 0 };
    let magnitude = value.unsigned_abs();
    if magnitude == 0 {
        return 0;
    }
    let exponent = 63 - magnitude.leading_zeros();
    if exponent > 15 {
        return sign | 0x7c00;
    }
    let significand = if exponent <= 10 {
        magnitude << (10 - exponent)
    } else {
        let shift = exponent - 10;
        let truncated = magnitude >> shift;
        let remainder = magnitude & ((1 << shift) - 1);
        let midpoint = 1 << (shift - 1);
        truncated + u64::from(remainder > midpoint || (remainder == midpoint && truncated & 1 != 0))
    };
    sign | (((exponent + 14) << 10) as u16 + significand as u16)
}
