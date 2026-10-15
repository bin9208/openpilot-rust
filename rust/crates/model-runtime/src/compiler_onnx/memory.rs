//! Offline buffer lifetime planning, ported from pinned tinygrad schedule/memory.py
//! and runtime/support/memory.py (MIT). No device allocation or kernel fusion.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[derive(Clone, Debug)]
pub struct BufferSpec {
    pub device: String,
    pub bytes: usize,
    pub held: bool,
    pub supports_offset: bool,
}
#[derive(Clone, Debug)]
pub struct Call {
    pub copy: bool,
    pub buffers: Vec<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arena {
    pub device: String,
    pub lane: u8,
    pub bytes: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub arena: usize,
    pub offset: usize,
    pub bytes: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryPlan {
    pub arenas: Vec<Arena>,
    pub views: BTreeMap<usize, View>,
}
fn round(value: usize, alignment: usize) -> Result<usize, String> {
    value
        .checked_next_multiple_of(alignment)
        .ok_or("memory planner size overflow".into())
}
fn bits(value: usize) -> usize {
    usize::BITS as usize - value.leading_zeros() as usize
}
fn bucket(size: usize) -> (usize, usize) {
    let first = bits(size);
    (
        first,
        (size - (1usize << (first - 1))) >> (first.saturating_sub(6)),
    )
}
#[derive(Clone, Copy)]
struct Block {
    size: usize,
    free: bool,
}
struct Tlsf {
    blocks: BTreeMap<usize, Block>,
    buckets: BTreeMap<(usize, usize), VecDeque<usize>>,
}
impl Tlsf {
    fn new(size: usize) -> Self {
        let mut a = Self {
            blocks: BTreeMap::new(),
            buckets: BTreeMap::new(),
        };
        if size > 0 {
            a.insert(0, size);
        }
        a
    }
    fn insert(&mut self, start: usize, size: usize) {
        self.blocks.insert(start, Block { size, free: true });
        self.buckets
            .entry(bucket(size))
            .or_default()
            .push_back(start);
    }
    fn remove(&mut self, start: usize) -> Result<Block, String> {
        let block = *self.blocks.get(&start).ok_or("missing TLSF block")?;
        if !block.free {
            return Err("TLSF block is not free".into());
        }
        let queue = self
            .buckets
            .get_mut(&bucket(block.size))
            .ok_or("missing TLSF bucket")?;
        let position = queue
            .iter()
            .position(|v| *v == start)
            .ok_or("missing TLSF free entry")?;
        queue.remove(position);
        self.blocks.get_mut(&start).unwrap().free = false;
        Ok(block)
    }
    fn alloc(&mut self, request: usize) -> Result<usize, String> {
        let request = request.max(256);
        let search = round(request, 1usize << (bits(request).saturating_sub(6)))?;
        let start = self
            .buckets
            .range(bucket(search)..)
            .find_map(|(_, q)| q.front().copied())
            .ok_or("TLSF allocation exhausted")?;
        let block = self.remove(start)?;
        if block.size < search {
            return Err("TLSF bucket cannot fit request".into());
        }
        if block.size > request {
            self.insert(start, request);
            self.insert(start + request, block.size - request);
            self.remove(start)?;
        }
        Ok(start)
    }
    fn free(&mut self, start: usize) -> Result<(), String> {
        let block = *self.blocks.get(&start).ok_or("unknown TLSF allocation")?;
        if block.free {
            return Err("duplicate TLSF free".into());
        }
        self.insert(start, block.size);
        let mut left = start;
        while let Some((&previous, block)) = self.blocks.range(..left).next_back() {
            if !block.free {
                break;
            }
            left = previous;
        }
        loop {
            let current = self.blocks[&left];
            let next = left + current.size;
            let Some(right) = self.blocks.get(&next).copied() else {
                break;
            };
            if !right.free {
                break;
            }
            self.remove(left)?;
            self.remove(next)?;
            self.blocks.remove(&next);
            self.insert(left, current.size + right.size);
        }
        Ok(())
    }
}
/// Preserve original encounter order, close-before-open events, copy holding
/// periods, separate copy/compute lanes, 256-byte blocks and TLSF bucket order.
pub fn plan_memory(buffers: &[BufferSpec], calls: &[Call]) -> Result<MemoryPlan, String> {
    plan_memory_enabled(buffers, calls, true)
}
/// The caller translates NO_MEMORY_PLANNER into this explicit option; disabling
/// planning preserves every original buffer and performs no capability probes.
pub fn plan_memory_enabled(
    buffers: &[BufferSpec],
    calls: &[Call],
    enabled: bool,
) -> Result<MemoryPlan, String> {
    if !enabled {
        return Ok(MemoryPlan::default());
    }

    let mut first = BTreeMap::new();
    let mut last = BTreeMap::new();
    let mut order = Vec::new();
    let mut copies = BTreeSet::new();
    for (time, call) in calls.iter().enumerate() {
        for &id in &call.buffers {
            let b = buffers
                .get(id)
                .ok_or("scheduled buffer index out of range")?;
            if b.held {
                continue;
            }
            let supported = match b.device.as_str() {
                "QCOM" => true,
                "CPU" | "NPY" | "PYTHON" => b.supports_offset,
                _ => return Err("unsupported offline planning device".into()),
            };
            if !supported {
                continue;
            }
            if first.insert(id, *first.get(&id).unwrap_or(&time)).is_none() {
                order.push(id);
            }
            last.insert(id, time);
            if call.copy {
                copies.insert(id);
            }
        }
    }
    let mut events = Vec::new();
    let mut total = 0usize;
    for (sequence, &id) in order.iter().enumerate() {
        let nbytes = round(buffers[id].bytes, 256)?;
        total = total
            .checked_add(nbytes)
            .ok_or("memory planner total overflow")?;
        let end = last[&id]
            .checked_add(1)
            .and_then(|v| {
                v.checked_add(if copies.contains(&id) {
                    last[&id] - first[&id] + 1
                } else {
                    0
                })
            })
            .ok_or("memory planner lifetime overflow")?;
        events.push((first[&id], true, sequence, id));
        events.push((end, false, sequence, id));
    }
    total = total
        .checked_mul(2)
        .ok_or("memory planner arena overflow")?;
    events.sort_by_key(|v| (v.0, v.1));
    let mut plan = MemoryPlan::default();
    let mut lanes = BTreeMap::<(String, u8), usize>::new();
    let mut allocators = Vec::new();
    let mut offsets = BTreeMap::new();
    for (_, open, _, id) in events {
        let b = &buffers[id];
        let lane = u8::from(copies.contains(&id));
        let key = (b.device.clone(), lane);
        let arena = if let Some(&v) = lanes.get(&key) {
            v
        } else {
            let id = plan.arenas.len();
            lanes.insert(key, id);
            plan.arenas.push(Arena {
                device: b.device.clone(),
                lane,
                bytes: 0,
            });
            allocators.push(Tlsf::new(total));
            id
        };
        if open {
            let offset = allocators[arena].alloc(round(b.bytes, 256)?)?;
            offsets.insert(id, offset);
            plan.views.insert(
                id,
                View {
                    arena,
                    offset,
                    bytes: b.bytes,
                },
            );
        } else {
            allocators[arena].free(offsets[&id])?;
        }
        let end = offsets[&id]
            .checked_add(b.bytes)
            .ok_or("memory planner range overflow")?;
        plan.arenas[arena].bytes = plan.arenas[arena].bytes.max(end);
    }
    for arena in &mut plan.arenas {
        arena.bytes = round(arena.bytes, 256)?;
    }
    Ok(plan)
}
