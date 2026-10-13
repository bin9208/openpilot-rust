use capnp::dynamic_value::Reader;
use num_traits::ToPrimitive;

#[derive(Clone, Copy)]
pub struct Data<'a>(pub Reader<'a>);

impl Data<'_> {
    pub fn field(self, name: &str) -> Self {
        Self(match self.0 {
            Reader::Struct(value) => value.get_named(name).unwrap_or(Reader::Void),
            _ => Reader::Void,
        })
    }
    pub fn float(self) -> Option<f64> {
        let value = match self.0 {
            Reader::Float32(value) => f64::from(value),
            Reader::Float64(value) => value,
            Reader::Int8(value) => f64::from(value),
            Reader::Int16(value) => f64::from(value),
            Reader::Int32(value) => f64::from(value),
            Reader::UInt8(value) => f64::from(value),
            Reader::UInt16(value) => f64::from(value),
            Reader::UInt32(value) => f64::from(value),
            Reader::Int64(value) => value.to_f64()?,
            Reader::UInt64(value) => value.to_f64()?,
            _ => return None,
        };
        value.is_finite().then_some(value)
    }
    pub fn int(self) -> Option<i64> {
        match self.0 {
            Reader::Int8(value) => Some(i64::from(value)),
            Reader::Int16(value) => Some(i64::from(value)),
            Reader::Int32(value) => Some(i64::from(value)),
            Reader::Int64(value) => Some(value),
            Reader::UInt8(value) => Some(i64::from(value)),
            Reader::UInt16(value) => Some(i64::from(value)),
            Reader::UInt32(value) => Some(i64::from(value)),
            Reader::UInt64(value) => i64::try_from(value).ok(),
            Reader::Enum(value) => Some(i64::from(value.get_value())),
            _ => self.float()?.to_i64(),
        }
    }
    pub fn uint(self) -> Option<u64> {
        match self.0 {
            Reader::UInt8(value) => Some(u64::from(value)),
            Reader::UInt16(value) => Some(u64::from(value)),
            Reader::UInt32(value) => Some(u64::from(value)),
            Reader::UInt64(value) => Some(value),
            _ => self.int().and_then(|value| u64::try_from(value).ok()),
        }
    }
    pub fn boolean(self) -> Option<bool> {
        match self.0 {
            Reader::Bool(value) => Some(value),
            _ => None,
        }
    }
    pub fn text(self) -> Option<String> {
        match self.0 {
            Reader::Text(value) => value.to_str().ok().map(str::to_owned),
            Reader::Enum(value) => value
                .get_enumerant()
                .ok()
                .flatten()?
                .get_proto()
                .get_name()
                .ok()?
                .to_str()
                .ok()
                .map(str::to_owned),
            _ => None,
        }
    }
    pub fn number(self, name: &str, fallback: f64) -> f64 {
        self.field(name).float().unwrap_or(fallback)
    }
    pub fn variant(self) -> Option<String> {
        let Reader::Struct(value) = self.0 else {
            return None;
        };
        value
            .which()
            .ok()
            .flatten()?
            .get_proto()
            .get_name()
            .ok()?
            .to_str()
            .ok()
            .map(str::to_owned)
    }
    pub fn flag(self, name: &str, fallback: bool) -> bool {
        self.field(name).boolean().unwrap_or(fallback)
    }
    pub fn index(self, index: usize) -> Self {
        Self(match self.0 {
            Reader::List(value) => u32::try_from(index)
                .ok()
                .filter(|index| *index < value.len())
                .and_then(|index| value.get(index).ok())
                .unwrap_or(Reader::Void),
            _ => Reader::Void,
        })
    }
    pub fn items(self) -> Vec<Self> {
        match self.0 {
            Reader::List(value) => value.iter().filter_map(Result::ok).map(Self).collect(),
            _ => Vec::new(),
        }
    }
    pub fn numbers(self, limit: usize, minimum: f64, maximum: f64) -> Vec<f64> {
        self.items()
            .into_iter()
            .take(limit)
            .filter_map(Self::float)
            .map(|value| value.clamp(minimum, maximum))
            .collect()
    }
    pub fn triple(self) -> Option<[f64; 3]> {
        let numbers = self.numbers(3, f64::NEG_INFINITY, f64::INFINITY);
        numbers.try_into().ok()
    }
    pub fn present(self) -> bool {
        !matches!(self.0, Reader::Void)
    }
}
