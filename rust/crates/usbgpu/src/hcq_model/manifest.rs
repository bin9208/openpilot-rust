use crate::Error;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub version: u32,
    pub model_sha256: String,
    pub model_bytes: u64,
    pub kernel_count: usize,
    pub buffers: Vec<Buffer>,
    pub patches: Vec<Patch>,
    pub arguments: Vec<View>,
    pub parameters: Vec<Parameter>,
    pub bindings: Vec<Binding>,
    pub input_table: InputTable,
    pub dispatcher: serde_json::Value,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Buffer {
    Allocation {
        bytes: u64,
        host: bool,
        cpu_access: bool,
        uncached: bool,
        initial: Blob,
    },
    Placeholder {
        tag: String,
        bytes: u64,
        elements: u64,
        host: bool,
        cpu_access: bool,
        uncached: bool,
        device: String,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Blob {
    pub offset: u64,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct View {
    pub buffer: usize,
    pub offset: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameter {
    pub slot: usize,
    pub name: String,
    pub bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub name: String,
    pub shape: Vec<u64>,
    pub bytes: u64,
    pub dtype: String,
    pub output: bool,
    pub alias: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputTable {
    pub argument: usize,
    pub entries: Vec<Input>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    pub slot: usize,
    pub offset: u64,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Expression {
    Constant { value: u64 },
    Address { view: View, space: Space },
    Add { left: Box<Self>, right: Box<Self> },
    Shr { left: Box<Self>, right: Box<Self> },
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Space {
    Host,
    Device,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Patch {
    Blob {
        view: View,
        blob: Blob,
    },
    Word {
        view: View,
        bytes: u8,
        value: Expression,
    },
}
impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > 64 << 20 {
            return Err(Error::Contract("HCQ model manifest size limit"));
        }
        let result: Self = serde_json::from_slice(bytes)?;
        if result.version != 1
            || result.model_bytes == 0
            || result.model_bytes > 4 << 30
            || result.kernel_count == 0
            || result.buffers.len() > 65536
            || result.patches.len() > 1_000_000
            || result.bindings.len() > 1024
        {
            return Err(Error::Contract("HCQ model manifest limits"));
        }
        let mut names = std::collections::HashSet::new();
        for (index, binding) in result.bindings.iter().enumerate() {
            let width = match binding.dtype.as_str() {
                "uint8" => 1,
                "float32" => 4,
                _ => return Err(Error::Contract("HCQ tensor dtype")),
            };
            let size = binding
                .shape
                .iter()
                .try_fold(width, |size: u64, dim| size.checked_mul(*dim));
            if size != Some(binding.bytes)
                || binding.bytes == 0
                || binding.bytes > 4 << 30
                || !names.insert(&binding.name)
            {
                return Err(Error::Contract("HCQ tensor shape or name"));
            }
            if let Some(alias) = binding.alias {
                if alias >= index
                    || !binding.output
                    || result.bindings[alias].bytes != binding.bytes
                    || result.bindings[alias].dtype != binding.dtype
                    || result.bindings[alias].output
                {
                    return Err(Error::Contract("HCQ recurrent alias mismatch"));
                }
            }
        }
        for entry in &result.input_table.entries {
            if result
                .bindings
                .get(entry.slot)
                .is_none_or(|b| entry.offset >= b.bytes)
            {
                return Err(Error::Contract("HCQ dynamic input range"));
            }
        }
        Ok(result)
    }
}
