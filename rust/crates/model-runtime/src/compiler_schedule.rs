//! Pipeline entrypoint emission, derived from model_export/offline_schedule.py
//! (`export_pipeline` and `emit_schedule`). This does not schedule tinygrad UOps
//! or evaluate constants: the caller supplies their stable identities after
//! constant preparation and the actual emitted-call prefix counts.

use crate::entrypoint::{self, Entrypoint};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
pub struct PipelineStage {
    pub name: String,
    /// Stable source-call identities, in original stage order. Copies count too.
    pub calls: Vec<u64>,
}

#[derive(Debug, Serialize)]
pub struct PipelineEntrypoints {
    pub version: u32,
    pub entrypoints: Vec<Entrypoint>,
}

/// Map stage boundaries through constant pruning and call emission. Identity
/// membership follows the original set-based test, including repeated source
/// identities. `emitted_by_source[0]` is zero; each subsequent element is the
/// number of emitted calls after that many prepared source calls. Kernel reuse
/// must not remove kernel *calls* from this prefix, and copies must be counted.
pub fn pipeline_entrypoints(
    stages: &[PipelineStage],
    retained_calls: &[u64],
    emitted_by_source: &[usize],
) -> Result<PipelineEntrypoints, String> {
    if emitted_by_source.len().checked_sub(1) != Some(retained_calls.len())
        || emitted_by_source.first() != Some(&0)
        || emitted_by_source.windows(2).any(|pair| pair[0] > pair[1])
    {
        return Err("invalid offline emitted-call prefix".into());
    }
    let retained = retained_calls.iter().copied().collect::<HashSet<_>>();
    let mut entries = Vec::with_capacity(stages.len());
    let mut end = 0usize;
    let mut start = 0;
    for stage in stages {
        end = end
            .checked_add(
                stage
                    .calls
                    .iter()
                    .filter(|id| retained.contains(*id))
                    .count(),
            )
            .ok_or("offline pipeline stage count overflow")?;
        let emitted_end = *emitted_by_source
            .get(end)
            .ok_or("offline pipeline stage exceeds prepared schedule")?;
        entries.push(Entrypoint {
            name: stage.name.clone(),
            start,
            end: emitted_end,
        });
        start = emitted_end;
    }
    let version = if entries.is_empty() { 1 } else { 2 };
    entrypoint::validate(version, &entries, *emitted_by_source.last().unwrap())
        .map_err(|error| error.to_string())?;
    Ok(PipelineEntrypoints {
        version,
        entrypoints: entries,
    })
}
