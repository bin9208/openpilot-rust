//! Live offline exporter bookkeeping. Tensor/UOp lowering and the genuine
//! compiler remain upstream of this byte/identity boundary.
use crate::{
    compiler_buffers::{
        descriptor, Allocation, Buffer, BufferTable, Descriptor, OfflineBufferTable, View,
    },
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
pub struct RawStage {
    name: String,
    calls: Vec<u64>,
}

// Missing fields select the version's schema; explicit null is never a synonym
// for absence. In particular, v2 must not silently consume a v1 stage_ends field.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
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
    #[serde(default, deserialize_with = "present")]
    stage_ends: Option<Vec<StageEnd>>,
    #[serde(default, deserialize_with = "present")]
    stages: Option<Vec<RawStage>>,
    #[serde(default, deserialize_with = "present")]
    retained_calls: Option<Vec<u64>>,
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

fn resolve_launch(
    global: &[Number],
    local: Option<&[Number]>,
    source: &str,
    binary: &[u8],
) -> Result<LaunchResult, String> {
    let global = global
        .iter()
        .map(Number::dimension)
        .collect::<Result<Vec<_>, _>>()?;
    let local = local
        .map(|values| {
            values
                .iter()
                .map(Number::dimension)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let (global, local) = launch_dimensions(&global, local.as_deref(), source, binary)?;
    Ok(LaunchResult {
        global: global.into_iter().map(Number::from).collect(),
        local: local.into_iter().map(Number::from).collect(),
    })
}

pub fn prepare(request: Request) -> Result<Plan, String> {
    if request.events.len() > 1_048_576
        || [
            request.initialized.len(),
            request.programs.len(),
            request.launches.len(),
        ]
        .iter()
        .any(|n| *n > 65536)
        || request.emitted_by_source.is_empty()
        || request.emitted_by_source.len() > 65537
    {
        return Err("unsupported export-plan version/count".into());
    }
    let (stages, retained) = match (
        request.version,
        request.stage_ends,
        request.stages,
        request.retained_calls,
    ) {
        // Explicit historical wire support, never a retry/fallback from v2.
        (1, Some(ends), None, None) if ends.len() <= 65536 => {
            let retained = (0..request.emitted_by_source.len() - 1)
                .map(|id| id as u64)
                .collect::<Vec<_>>();
            let mut stages = Vec::new();
            let mut start = 0;
            let mut names = BTreeSet::new();
            for stage in ends {
                if stage.end < start
                    || stage.end > retained.len()
                    || !names.insert(stage.name.clone())
                {
                    return Err("invalid prepared stage boundary".into());
                }
                stages.push(PipelineStage {
                    name: stage.name,
                    calls: retained[start..stage.end].to_vec(),
                });
                start = stage.end;
            }
            (stages, retained)
        }
        (2, None, Some(stages), Some(retained))
            if stages.len() <= 65536 && retained.len() <= 65536 =>
        {
            stages.iter().try_fold(0usize, |sum, stage| {
                sum.checked_add(stage.calls.len())
                    .filter(|sum| *sum <= 1_048_576)
                    .ok_or("raw stage occurrences exceed bound")
            })?;
            (
                stages
                    .into_iter()
                    .map(|stage| PipelineStage {
                        name: stage.name,
                        calls: stage.calls,
                    })
                    .collect(),
                retained,
            )
        }
        _ => return Err("unsupported export-plan version/stage contract".into()),
    };
    let entries = pipeline_entrypoints(&stages, &retained, &request.emitted_by_source)?;
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
        launches.push(resolve_launch(
            &launch.global,
            launch.local.as_deref(),
            &program.source,
            &bytes(&program.binary)?,
        )?);
    }
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

// Distinct ordered protocol. A checkpoint is never a completed export Plan.
pub const MAX_STREAM_FRAMES: usize = 1_048_576 + 2 * 65536 + 2;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamFrame {
    session: String,
    sequence: u64,
    command: StreamCommand,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum StreamCommand {
    Begin {
        version: u32,
        initialized: Vec<Initializer>,
    },
    Event {
        event: Event,
    },
    Program {
        key: u64,
        program: Program,
    },
    Launch {
        key: u64,
        global: Vec<Number>,
        local: Option<Vec<Number>>,
    },
    Finish {
        emitted_by_source: Vec<usize>,
        stages: Vec<RawStage>,
        retained_calls: Vec<u64>,
    },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamReply {
    Checkpoint {
        session: String,
        sequence: u64,
        complete: bool,
    },
    Final {
        session: String,
        sequence: u64,
        complete: bool,
        plan: Plan,
    },
}
impl StreamReply {
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Final { .. })
    }
}

#[derive(Default)]
pub struct StreamSession {
    session: Option<String>,
    sequence: u64,
    failed: bool,
    finished: bool,
    initialized: BTreeMap<u64, Vec<u8>>,
    table: BufferTable,
    bases: BTreeMap<u64, usize>,
    storage_bytes: usize,
    held_bytes: usize,
    event_views: Vec<usize>,
    programs: BTreeMap<u64, (String, Vec<u8>)>,
    launches: Vec<LaunchResult>,
}

impl StreamSession {
    pub fn handle(&mut self, frame: StreamFrame) -> Result<StreamReply, String> {
        if self.failed || self.finished {
            return Err("export stream is failed or finished".into());
        }
        let result = self.apply(frame);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn retain(&mut self, length: usize) -> Result<(), String> {
        self.held_bytes = self
            .held_bytes
            .checked_add(length)
            .filter(|total| *total <= MAX_MESSAGE)
            .ok_or("export stream retained bytes exceed bound")?;
        Ok(())
    }

    fn apply(&mut self, frame: StreamFrame) -> Result<StreamReply, String> {
        if frame.sequence != self.sequence
            || self.sequence >= MAX_STREAM_FRAMES as u64
            || frame.session.len() != 32
            || !frame
                .session
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("invalid export stream session/sequence".into());
        }
        if let Some(session) = &self.session {
            if *session != frame.session {
                return Err("export stream owner differs".into());
            }
        } else if !matches!(&frame.command, StreamCommand::Begin { .. }) {
            return Err("export stream must begin explicitly".into());
        }
        match frame.command {
            StreamCommand::Begin {
                version,
                initialized,
            } => {
                if self.session.is_some() || version != 1 || initialized.len() > 65536 {
                    return Err("invalid export stream begin".into());
                }
                for item in initialized {
                    let data = bytes(&item.data)?;
                    self.retain(data.len())?;
                    // Length/base admission remains lazy, at the actual read.
                    self.initialized.insert(item.key, data);
                }
                self.session = Some(frame.session.clone());
            }
            StreamCommand::Event { event } => {
                if self.event_views.len() >= 1_048_576 {
                    return Err("export stream event bound".into());
                }
                let buffer = event.buffer.buffer()?;
                if let Some(length) = self.bases.get(&buffer.base) {
                    if *length != buffer.allocation_bytes {
                        return Err("base identity changed size".into());
                    }
                } else {
                    self.storage_bytes = self
                        .storage_bytes
                        .checked_add(buffer.allocation_bytes)
                        .filter(|sum| *sum <= 1024 * 1024 * 1024)
                        .ok_or("export-plan storage exceeds 1 GiB")?;
                    self.bases.insert(buffer.base, buffer.allocation_bytes);
                }
                let index = self.table.view(buffer)?;
                match event.op {
                    Operation::View => (),
                    Operation::Read => self.table.read_initialized(
                        buffer,
                        self.initialized.get(&buffer.base).map(Vec::as_slice),
                    )?,
                    Operation::Write => self.table.write(buffer)?,
                }
                self.event_views.push(index);
            }
            StreamCommand::Program { key, program } => {
                if self.programs.len() >= 65536 || self.programs.contains_key(&key) {
                    return Err("duplicate/excess export stream program".into());
                }
                let binary = bytes(&program.binary)?;
                self.retain(program.source.len())?;
                self.retain(binary.len())?;
                self.programs.insert(key, (program.source, binary));
            }
            StreamCommand::Launch { key, global, local } => {
                if self.launches.len() >= 65536 {
                    return Err("export stream launch bound".into());
                }
                let (source, binary) = self
                    .programs
                    .get(&key)
                    .ok_or("unknown export stream program")?;
                self.launches
                    .push(resolve_launch(&global, local.as_deref(), source, binary)?);
            }
            StreamCommand::Finish {
                emitted_by_source,
                stages,
                retained_calls,
            } => {
                // Original final boundary: weights first, then stage projection.
                let (allocations, packed) = self.table.weights();
                if packed.len() > MAX_MESSAGE / 2 {
                    return Err("export-plan weights exceed wire bound".into());
                }
                if stages.len() > 65536
                    || retained_calls.len() > 65536
                    || emitted_by_source.is_empty()
                    || emitted_by_source.len() > 65537
                    || self.launches.len() > *emitted_by_source.last().unwrap()
                {
                    return Err("invalid export stream final counts".into());
                }
                stages.iter().try_fold(0usize, |sum, stage| {
                    sum.checked_add(stage.calls.len())
                        .filter(|sum| *sum <= 1_048_576)
                        .ok_or("raw stage occurrences exceed bound")
                })?;
                let stages = stages
                    .into_iter()
                    .map(|stage| PipelineStage {
                        name: stage.name,
                        calls: stage.calls,
                    })
                    .collect::<Vec<_>>();
                let entries = pipeline_entrypoints(&stages, &retained_calls, &emitted_by_source)?;
                let plan = Plan {
                    version: entries.version,
                    allocations,
                    views: self
                        .table
                        .views
                        .iter()
                        .map(|v| View {
                            allocation: v.allocation,
                            offset: v.offset,
                            bytes: v.bytes,
                        })
                        .collect(),
                    event_views: std::mem::take(&mut self.event_views),
                    weights: hex(&packed),
                    weights_sha256: format!("{:x}", Sha256::digest(&packed)),
                    launches: std::mem::take(&mut self.launches),
                    entrypoints: entries.entrypoints,
                };
                self.finished = true;
                self.sequence += 1;
                return Ok(StreamReply::Final {
                    session: frame.session,
                    sequence: frame.sequence,
                    complete: true,
                    plan,
                });
            }
        }
        self.sequence += 1;
        Ok(StreamReply::Checkpoint {
            session: frame.session,
            sequence: frame.sequence,
            complete: false,
        })
    }
}
