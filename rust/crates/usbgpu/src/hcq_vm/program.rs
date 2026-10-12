use super::{Execution, Memory, Node, Op};
use crate::Error;

pub struct Program {
    pub(super) nodes: Vec<Node>,
    pub(super) ends: Vec<usize>,
}
impl Program {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > 16 << 20 {
            return Err(Error::Contract("HCQ dispatcher manifest exceeds limit"));
        }
        let nodes: Vec<Node> = serde_json::from_slice(bytes)?;
        if nodes.is_empty() || nodes.len() > 65536 {
            return Err(Error::Contract("HCQ dispatcher node count"));
        }
        let mut ends = vec![0; nodes.len()];
        let mut stack = Vec::new();
        let mut local_bytes = 0usize;
        for (index, node) in nodes.iter().enumerate() {
            if ![0, 1, 8, 16, 32, 64].contains(&node.bits) || node.src.iter().any(|&s| s >= index) {
                return Err(Error::Contract("invalid HCQ dispatcher type or dependency"));
            }
            let valid = match node.op {
                Op::Argument { .. } | Op::Constant { .. } => node.src.is_empty(),
                Op::Local { bytes } => {
                    local_bytes = local_bytes
                        .checked_add(bytes)
                        .ok_or(Error::Contract("HCQ local size overflow"))?;
                    bytes > 0 && node.src.is_empty() && local_bytes <= 16 << 20
                }
                Op::Index { stride } => node.src.len() >= 2 && [1, 2, 4, 8].contains(&stride),
                Op::Cast | Op::Bitcast | Op::Load | Op::Function { .. } => node.src.len() == 1,
                Op::Store => node.src.len() == 2,
                Op::After => !node.src.is_empty(),
                Op::Add
                | Op::Sub
                | Op::Mul
                | Op::And
                | Op::Or
                | Op::Xor
                | Op::Shl
                | Op::Shr
                | Op::Less
                | Op::NotEqual
                | Op::Equal => node.src.len() == 2,
                Op::Where => node.src.len() == 3,
                Op::Call => node.src.first().is_some_and(|&s| match nodes[s].op {
                    Op::Function {
                        function: super::Function::Control,
                    } => node.src.len() == 9,
                    Op::Function {
                        function: super::Function::Bulk,
                    } => node.src.len() == 7,
                    _ => false,
                }),
                Op::Range => {
                    stack.push(index);
                    !node.src.is_empty()
                }
                Op::End => {
                    if !(2..=3).contains(&node.src.len()) || stack.pop() != node.src.get(1).copied()
                    {
                        return Err(Error::Contract("HCQ loop nesting mismatch"));
                    }
                    ends[node.src[1]] = index;
                    true
                }
                Op::Noop => true,
            };
            if !valid {
                return Err(Error::Contract("HCQ dispatcher operation arity"));
            }
            if matches!(node.op, Op::Load | Op::Store) && node.bits == 0 {
                return Err(Error::Contract("HCQ memory operation has no width"));
            }
        }
        if !stack.is_empty() {
            return Err(Error::Contract("unterminated HCQ loop"));
        }
        Ok(Self { nodes, ends })
    }
    pub fn bind(self, memory: &mut Memory, arguments: &[u64]) -> Result<Execution, Error> {
        let mut initial = vec![0; self.nodes.len()];
        for (index, node) in self.nodes.iter().enumerate() {
            initial[index] = match node.op {
                Op::Argument { slot } => *arguments
                    .get(slot)
                    .ok_or(Error::Contract("missing HCQ argument"))?,
                Op::Local { bytes } => memory.allocate(vec![0; bytes])?,
                _ => 0,
            };
        }
        let values = initial.clone();
        Ok(Execution {
            program: self,
            initial,
            values,
        })
    }
}
