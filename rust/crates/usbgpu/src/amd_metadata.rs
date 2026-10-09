use crate::Error;
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DataType {
    Integer {
        size: usize,
        #[serde(default)]
        signed: bool,
    },
    Opaque {
        size: usize,
    },
    Pointer {
        size: usize,
    },
    Record {
        name: String,
    },
    Array {
        element: Box<DataType>,
        count: usize,
    },
}
#[derive(Clone, Debug, Deserialize)]
pub struct Field {
    pub datatype: DataType,
    pub offset: usize,
    pub bits: Vec<usize>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Layout {
    pub size: usize,
    pub fields: BTreeMap<String, Field>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct RegisterDefinition(pub u32, pub usize, pub BTreeMap<String, (u8, u8)>);
#[derive(Deserialize)]
pub struct Catalog {
    pub modules: Vec<String>,
    pub registers: BTreeMap<String, BTreeMap<String, RegisterDefinition>>,
    pub layouts: BTreeMap<String, Layout>,
    pub constants: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    pub firmware_hashes: BTreeMap<String, String>,
    pub hardware_ids: BTreeMap<u32, u32>,
    pub sources: BTreeMap<String, String>,
    pub macros: BTreeMap<String, BTreeMap<String, (u64, u32)>>,
}
impl Catalog {
    pub fn bundled() -> Result<Self, Error> {
        Ok(serde_json::from_str(include_str!(
            "../assets/amd-metadata.json"
        ))?)
    }
    pub fn layout(&self, name: &str) -> Result<&Layout, Error> {
        self.layouts
            .get(name)
            .ok_or_else(|| Error::Protocol(format!("unknown AMD layout {name}")))
    }
    pub fn size(&self, datatype: &DataType) -> Result<usize, Error> {
        match datatype {
            DataType::Integer { size, .. }
            | DataType::Pointer { size }
            | DataType::Opaque { size } => Ok(*size),
            DataType::Record { name } => Ok(self.layout(name)?.size),
            DataType::Array { element, count } => self
                .size(element)?
                .checked_mul(*count)
                .ok_or(Error::Contract("AMD array size overflow")),
        }
    }
    pub fn field<'a>(&'a self, record: &str, path: &[&str]) -> Result<(&'a Field, usize), Error> {
        let (first, rest) = path
            .split_first()
            .ok_or(Error::Contract("empty AMD field path"))?;
        let field = self
            .layout(record)?
            .fields
            .get(*first)
            .ok_or_else(|| Error::Protocol(format!("unknown AMD field {record}.{first}")))?;
        if rest.is_empty() {
            return Ok((field, field.offset));
        }
        let DataType::Record { name } = &field.datatype else {
            return Err(Error::Contract("AMD field path is not a record"));
        };
        let (nested, offset) = self.field(name, rest)?;
        Ok((
            nested,
            offset
                .checked_add(field.offset)
                .ok_or(Error::Contract("AMD field offset overflow"))?,
        ))
    }
    fn integer_span(field: &Field) -> Result<usize, Error> {
        let size = match field.datatype {
            DataType::Integer { size, .. } | DataType::Pointer { size } => size,
            _ => return Err(Error::Contract("AMD field is not an integer")),
        };
        if size == 0 || size > 8 {
            return Err(Error::Contract("invalid AMD integer size"));
        }
        match field.bits.as_slice() {
            [] => Ok(size),
            [width, shift]
                if *width > 0 && *width <= 64 && *shift < 64 && width + shift <= size * 8 =>
            {
                Ok((width + shift).div_ceil(8))
            }
            _ => Err(Error::Contract("invalid AMD bitfield span")),
        }
    }
    pub fn read_signed(&self, record: &str, path: &[&str], bytes: &[u8]) -> Result<i64, Error> {
        let (field, _) = self.field(record, path)?;
        let DataType::Integer { size, signed: true } = field.datatype else {
            return Err(Error::Contract("AMD field is not a signed integer"));
        };
        let value = self.read(record, path, bytes)?;
        let width = field.bits.first().copied().unwrap_or(size * 8);
        Ok(((value << (64 - width)) as i64) >> (64 - width))
    }
    pub fn encode_macro(&self, module: &str, name: &str, value: u32) -> Result<u32, Error> {
        let &(mask, shift) = self
            .macros
            .get(module)
            .and_then(|m| m.get(name))
            .ok_or_else(|| Error::Protocol(format!("unknown AMD macro {module}.{name}")))?;
        u32::try_from(
            (u64::from(value) & mask)
                .checked_shl(shift)
                .ok_or(Error::Contract("AMD macro shift overflow"))?,
        )
        .map_err(|_| Error::Contract("AMD macro value overflow"))
    }
    pub fn read(&self, record: &str, path: &[&str], bytes: &[u8]) -> Result<u64, Error> {
        let (field, offset) = self.field(record, path)?;
        let size = Self::integer_span(field)?;
        let end = offset
            .checked_add(size)
            .ok_or(Error::Contract("AMD field range overflow"))?;
        let data = bytes
            .get(offset..end)
            .ok_or(Error::Contract("truncated AMD record"))?;
        if size > 8 {
            return Err(Error::Contract("AMD integer wider than 64 bits"));
        }
        let mut raw = [0; 8];
        raw[..size].copy_from_slice(data);
        let value = u64::from_le_bytes(raw);
        Ok(if field.bits.is_empty() {
            value
        } else {
            let width = field.bits[0];
            let shift = field.bits[1];
            (value >> shift) & (u64::MAX >> (64 - width))
        })
    }
    pub fn write(
        &self,
        record: &str,
        path: &[&str],
        bytes: &mut [u8],
        value: u64,
    ) -> Result<(), Error> {
        let (field, offset) = self.field(record, path)?;
        let size = Self::integer_span(field)?;
        let end = offset
            .checked_add(size)
            .ok_or(Error::Contract("AMD field range overflow"))?;
        let data = bytes
            .get_mut(offset..end)
            .ok_or(Error::Contract("truncated AMD record"))?;
        if size > 8 {
            return Err(Error::Contract("AMD integer wider than 64 bits"));
        }
        let value = if field.bits.is_empty() {
            value
        } else {
            let width = field.bits[0];
            let shift = field.bits[1];
            let mask = u64::MAX >> (64 - width);
            if value & mask != value {
                return Err(Error::Contract("AMD bitfield value overflow"));
            }
            let mut old = [0; 8];
            old[..size].copy_from_slice(data);
            (u64::from_le_bytes(old) & !(mask << shift)) | (value << shift)
        };
        if size < 8 && value >> (size * 8) != 0 {
            return Err(Error::Contract("AMD integer value overflow"));
        }
        data.copy_from_slice(&value.to_le_bytes()[..size]);
        Ok(())
    }
    pub fn constant(&self, module: &str, name: &str) -> Result<u64, Error> {
        self.constants
            .get(module)
            .and_then(|m| m.get(name))
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                Error::Protocol(format!("unknown unsigned AMD constant {module}.{name}"))
            })
    }
    pub fn queue_registers(
        &self,
        gfx: [u8; 3],
        nbio: [u8; 3],
    ) -> Result<BTreeMap<String, Register>, Error> {
        let module = if gfx[0] == 9 {
            "vega_offsets"
        } else {
            "navi_offsets"
        };
        let offsets = self
            .constants
            .get(module)
            .ok_or(Error::Contract("queue register offsets missing"))?;
        let mut result = BTreeMap::new();
        for (prefix, base_prefix, version, segments) in [
            ("gc", "GC", gfx, 6),
            (if gfx[0] < 12 { "nbio" } else { "nbif" }, "NBIO", nbio, 9),
        ] {
            let mut bases = BTreeMap::new();
            for instance in 0..6 {
                let mut values = Vec::new();
                for segment in 0..segments {
                    values.push(
                        match offsets
                            .get(&format!("{base_prefix}_BASE__INST{instance}_SEG{segment}"))
                        {
                            None => 0,
                            Some(value) => value
                                .as_u64()
                                .ok_or(Error::Contract("invalid queue register offset"))?,
                        },
                    );
                }
                bases.insert(instance, values);
            }
            for (name, definition) in self.register_module(prefix, version)? {
                result.insert(name.clone(), Register::bind(definition, &bases)?);
            }
        }
        Ok(result)
    }
    pub fn register_module(
        &self,
        prefix: &str,
        version: [u8; 3],
    ) -> Result<&BTreeMap<String, RegisterDefinition>, Error> {
        for module in self.modules.iter().rev() {
            let Some(suffix) = module.strip_prefix(&format!("{prefix}_")) else {
                continue;
            };
            let version_parts = suffix
                .split('_')
                .map(str::parse::<u8>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| Error::Contract("invalid AMD module version"))?;
            if version_parts.len() == 3
                && version_parts[0] == version[0]
                && version_parts.as_slice() <= version.as_slice()
            {
                return self
                    .registers
                    .get(module)
                    .ok_or(Error::Contract("missing AMD register module"));
            }
        }
        Err(Error::Protocol(format!(
            "unsupported AMD registers {prefix} {version:?}"
        )))
    }
}
#[derive(Clone, Debug)]
pub struct Register {
    pub addresses: BTreeMap<u8, u64>,
    pub fields: BTreeMap<String, (u8, u8)>,
}
impl Register {
    pub fn bind(
        definition: &RegisterDefinition,
        bases: &BTreeMap<u8, Vec<u64>>,
    ) -> Result<Self, Error> {
        let mut addresses = BTreeMap::new();
        for (&instance, segments) in bases {
            let base = segments
                .get(definition.1)
                .ok_or(Error::Contract("AMD register segment missing"))?;
            addresses.insert(
                instance,
                base.checked_add(u64::from(definition.0))
                    .ok_or(Error::Contract("AMD register address overflow"))?,
            );
        }
        Ok(Self {
            addresses,
            fields: definition.2.clone(),
        })
    }
    pub fn address(&self, instance: u8) -> Result<u64, Error> {
        self.addresses
            .get(&instance)
            .copied()
            .ok_or(Error::Contract("AMD register instance missing"))
    }
    pub fn encode(&self, fields: &[(&str, u32)]) -> Result<u32, Error> {
        let mut result = 0u64;
        for &(name, value) in fields {
            let &(start, _) = self
                .fields
                .get(name)
                .ok_or_else(|| Error::Protocol(format!("unknown AMD register field {name}")))?;
            result |= u64::from(value) << start;
        }
        u32::try_from(result).map_err(|_| Error::Contract("AMD register value overflow"))
    }
    pub fn field_mask(&self, names: &[&str]) -> Result<u32, Error> {
        let mut result = 0u64;
        for &name in names {
            let &(start, end) = self
                .fields
                .get(name)
                .ok_or_else(|| Error::Protocol(format!("unknown AMD register field {name}")))?;
            result |= ((1u64 << (end - start + 1)) - 1) << start;
        }
        u32::try_from(result).map_err(|_| Error::Contract("AMD register mask overflow"))
    }
    pub fn decode(&self, name: &str, value: u32) -> Result<u32, Error> {
        let &(start, end) = self
            .fields
            .get(name)
            .ok_or_else(|| Error::Protocol(format!("unknown AMD register field {name}")))?;
        Ok(((u64::from(value) >> start) & ((1u64 << (end - start + 1)) - 1)) as u32)
    }
}
