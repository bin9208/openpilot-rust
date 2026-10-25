//! Live offline exporter bookkeeping. Tensor/UOp lowering and the genuine
//! compiler remain upstream of this byte/identity boundary.
use crate::{
    compiler_buffers::{descriptor, Allocation, Buffer, Descriptor, OfflineBufferTable, View},
    compiler_launch::{launch_dimensions, Dimension},
    compiler_schedule::{pipeline_entrypoints, PipelineStage},
    entrypoint::Entrypoint,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_MESSAGE: usize = 256 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Storage {
    base: u64,
    allocation_bytes: usize,
    offset: usize,
    bytes: usize,
    device: String,
    allocated: bool,
}
impl Storage {
    fn buffer(&self) -> Result<Buffer, String> {
        descriptor(Descriptor::Single {
            buffer: Buffer {
                base: self.base,
                allocation_bytes: self.allocation_bytes,
                offset: self.offset,
                bytes: self.bytes,
            },
            device: &self.device,
            allocated: self.allocated,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Initializer {
    /// Identity of the mapping's actual key object, which may be an alias.
    key: u64,
    data: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    View,
    Read,
    Write,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    op: Operation,
    buffer: Storage,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Program {
    source: String,
    binary: String,
}
#[derive(Deserialize, Serialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Number {
    Integer(String),
    Float(f64),
    Boolean(bool),
}
impl Number {
    fn dimension(&self) -> Result<Dimension, String> {
        Ok(match self {
            Self::Integer(value) => {
                Dimension::Integer(value.parse().map_err(|_| "export dimension exceeds i128")?)
            }
            Self::Float(value) if value.is_finite() => Dimension::Float(*value),
            Self::Float(_) => return Err("nonfinite export dimension".into()),
            Self::Boolean(value) => Dimension::Boolean(*value),
        })
    }
}
impl From<Dimension> for Number {
    fn from(value: Dimension) -> Self {
        match value {
            Dimension::Integer(value) => Self::Integer(value.to_string()),
            Dimension::Float(value) => Self::Float(value),
            Dimension::Boolean(value) => Self::Boolean(value),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launch {
    program: usize,
    global: Vec<Number>,
    local: Option<Vec<Number>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageEnd {
    name: String,
    end: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    version: u32,
    initialized: Vec<Initializer>,
    events: Vec<Event>,
    programs: Vec<Program>,
    launches: Vec<Launch>,
    /// Genuine post-planning emission counts; constant pruning precedes this boundary.
    emitted_by_source: Vec<usize>,
    stage_ends: Vec<StageEnd>,
}
#[derive(Serialize)]
pub struct LaunchResult {
    global: Vec<Number>,
    local: Vec<Number>,
}
#[derive(Serialize)]
pub struct Plan {
    version: u32,
    allocations: Vec<Allocation>,
    views: Vec<View>,
    event_views: Vec<usize>,
    weights: String,
    weights_sha256: String,
    launches: Vec<LaunchResult>,
    entrypoints: Vec<Entrypoint>,
}
fn bytes(value: &str) -> Result<Vec<u8>, String> {
    if value.len() > MAX_MESSAGE || !value.len().is_multiple_of(2) {
        return Err("invalid bounded export hex bytes".into());
    }
    let nibble = |v| match v {
        b'0'..=b'9' => Ok(v - b'0'),
        b'a'..=b'f' => Ok(v - b'a' + 10),
        _ => Err("invalid export hex byte".to_owned()),
    };
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|v| Ok(nibble(v[0])? * 16 + nibble(v[1])?))
        .collect()
}
fn hex(value: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    value
        .iter()
        .flat_map(|b| {
            [
                DIGITS[(b >> 4) as usize] as char,
                DIGITS[(b & 15) as usize] as char,
            ]
        })
        .collect()
}

pub fn prepare(request: Request) -> Result<Plan, String> {
    if request.version != 1
        || request.events.len() > 1_048_576
        || [
            request.initialized.len(),
            request.programs.len(),
            request.launches.len(),
            request.stage_ends.len(),
        ]
        .iter()
        .any(|n| *n > 65536)
        || request.emitted_by_source.is_empty()
        || request.emitted_by_source.len() > 65537
    {
        return Err("unsupported export-plan version/count".into());
    }
    if request.launches.len() > *request.emitted_by_source.last().unwrap() {
        return Err("export launches exceed emitted calls".into());
    }
    // Admit distinct storage before allocating the original table's zero images.
    let mut bases = BTreeMap::new();
    let mut total = 0usize;
    for event in &request.events {
        let buffer = event.buffer.buffer()?;
        if let Some(size) = bases.insert(buffer.base, buffer.allocation_bytes) {
            if size != buffer.allocation_bytes {
                return Err("base identity changed size".into());
            }
        } else {
            total = total
                .checked_add(buffer.allocation_bytes)
                .ok_or("export storage sum overflow")?;
            if total > 1024 * 1024 * 1024 {
                return Err("export-plan storage exceeds 1 GiB".into());
            }
        }
    }
    let mut initialized = BTreeMap::new();
    for item in request.initialized {
        initialized.insert(item.key, bytes(&item.data)?);
    }
    let mut table = OfflineBufferTable::new(&initialized);
    let mut event_views = Vec::with_capacity(request.events.len());
    for event in request.events {
        let buffer = event.buffer.buffer()?;
        event_views.push(table.view(buffer)?);
        match event.op {
            Operation::View => (),
            Operation::Read => table.read(buffer)?,
            Operation::Write => table.write(buffer)?,
        }
    }
    let mut launches = Vec::new();
    for launch in request.launches {
        let program = request
            .programs
            .get(launch.program)
            .ok_or("export launch program out of range")?;
        let global = launch
            .global
            .iter()
            .map(Number::dimension)
            .collect::<Result<Vec<_>, _>>()?;
        let local = launch
            .local
            .as_ref()
            .map(|v| {
                v.iter()
                    .map(Number::dimension)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        let (global, local) = launch_dimensions(
            &global,
            local.as_deref(),
            &program.source,
            &bytes(&program.binary)?,
        )?;
        launches.push(LaunchResult {
            global: global.into_iter().map(Number::from).collect(),
            local: local.into_iter().map(Number::from).collect(),
        });
    }
    let retained = (0..request.emitted_by_source.len() - 1)
        .map(|id| id as u64)
        .collect::<Vec<_>>();
    let mut stages = Vec::new();
    let mut start = 0;
    let mut names = BTreeSet::new();
    for stage in request.stage_ends {
        if stage.end < start || stage.end > retained.len() || !names.insert(stage.name.clone()) {
            return Err("invalid prepared stage boundary".into());
        }
        stages.push(PipelineStage {
            name: stage.name,
            calls: retained[start..stage.end].to_vec(),
        });
        start = stage.end;
    }
    let entries = pipeline_entrypoints(&stages, &retained, &request.emitted_by_source)?;
    let (allocations, packed) = table.weights();
    if packed.len() > MAX_MESSAGE / 2 {
        return Err("export-plan weights exceed wire bound".into());
    }
    Ok(Plan {
        version: entries.version,
        allocations,
        views: table
            .views()
            .iter()
            .map(|v| View {
                allocation: v.allocation,
                offset: v.offset,
                bytes: v.bytes,
            })
            .collect(),
        event_views,
        weights: hex(&packed),
        weights_sha256: format!("{:x}", Sha256::digest(&packed)),
        launches,
        entrypoints: entries.entrypoints,
    })
}
