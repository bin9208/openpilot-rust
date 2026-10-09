mod execute;
mod memory;
mod program;
use crate::Error;
pub use execute::Execution;
pub use memory::Memory;
pub use program::Program;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Function {
    Control,
    Bulk,
}
impl Function {
    pub const fn token(self) -> u64 {
        match self {
            Self::Control => 1,
            Self::Bulk => 2,
        }
    }
}
pub trait Host {
    fn poll(&mut self) -> Result<(), Error>;
    fn call(
        &mut self,
        function: Function,
        arguments: &[u64],
        memory: &mut Memory,
    ) -> Result<u64, Error>;
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Op {
    Argument { slot: usize },
    Local { bytes: usize },
    Constant { value: u64 },
    Index { stride: u64 },
    Cast,
    Bitcast,
    Load,
    Store,
    After,
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Less,
    NotEqual,
    Equal,
    Where,
    Function { function: Function },
    Call,
    Range,
    End,
    Noop,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Node {
    pub op: Op,
    pub src: Vec<usize>,
    #[serde(default)]
    pub bits: u8,
    #[serde(default)]
    pub signed: bool,
    #[serde(default)]
    pub pointer: bool,
}
impl Node {
    fn normalize(&self, value: u64) -> u64 {
        if self.pointer || self.bits == 0 || self.bits == 64 {
            return value;
        }
        let shift = 64 - self.bits;
        if self.signed {
            ((value << shift) as i64 >> shift) as u64
        } else {
            value & (u64::MAX >> shift)
        }
    }
}
