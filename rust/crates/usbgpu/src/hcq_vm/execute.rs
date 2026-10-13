use super::{Host, Memory, Op, Program};
use crate::Error;

pub struct Execution {
    pub(super) program: Program,
    pub(super) initial: Vec<u64>,
    pub(super) values: Vec<u64>,
}
impl Execution {
    pub fn run(&mut self, memory: &mut Memory, host: &mut impl Host) -> Result<(), Error> {
        self.values.copy_from_slice(&self.initial);
        let mut pc = 0;
        while let Some(node) = self.program.nodes.get(pc) {
            host.poll()?;
            let get = |i: usize| self.values[node.src[i]];
            let a = node.src.first().map_or(0, |&s| self.values[s]);
            let value = match node.op {
                Op::Argument { .. } | Op::Local { .. } => self.values[pc],
                Op::Constant { value } => value,
                Op::Index { stride } => a
                    .checked_add(
                        get(1)
                            .checked_mul(stride)
                            .ok_or(Error::Contract("HCQ pointer multiplication overflow"))?,
                    )
                    .ok_or(Error::Contract("HCQ pointer addition overflow"))?,
                Op::Cast | Op::Bitcast | Op::After => a,
                Op::Load => memory.scalar(a, node.bits)?,
                Op::Store => {
                    memory.write(
                        a,
                        &get(1).to_le_bytes()[..usize::from(node.bits.div_ceil(8))],
                    )?;
                    0
                }
                Op::Add => a.wrapping_add(get(1)),
                Op::Sub => a.wrapping_sub(get(1)),
                Op::Mul => a.wrapping_mul(get(1)),
                Op::And => a & get(1),
                Op::Or => a | get(1),
                Op::Xor => a ^ get(1),
                Op::Shl | Op::Shr => {
                    let shift =
                        u32::try_from(get(1)).map_err(|_| Error::Contract("HCQ shift overflow"))?;
                    if shift >= u32::from(node.bits) {
                        return Err(Error::Contract("HCQ shift outside scalar width"));
                    }
                    match node.op {
                        Op::Shl => a << shift,
                        Op::Shr if node.signed => ((a as i64) >> shift) as u64,
                        Op::Shr => a >> shift,
                        _ => unreachable!(),
                    }
                }
                Op::Less => u64::from(if self.program.nodes[node.src[0]].signed {
                    (a as i64) < (get(1) as i64)
                } else {
                    a < get(1)
                }),
                Op::NotEqual => u64::from(a != get(1)),
                Op::Equal => u64::from(a == get(1)),
                Op::Where => {
                    if a != 0 {
                        get(1)
                    } else {
                        get(2)
                    }
                }
                Op::Function { function } => {
                    if a != function.token() {
                        return Err(Error::Contract("HCQ external function binding mismatch"));
                    }
                    a
                }
                Op::Call => {
                    let Op::Function { function } = self.program.nodes[node.src[0]].op else {
                        return Err(Error::Contract("HCQ call target is not a function"));
                    };
                    let mut args = [0; 8];
                    for (out, &source) in args.iter_mut().zip(&node.src[1..]) {
                        *out = self.values[source];
                    }
                    host.call(function, &args[..node.src.len() - 1], memory)?
                }
                Op::Range => {
                    if !matches!(self.program.nodes[node.src[0]].op, Op::Noop) && a == 0 {
                        pc = self.program.ends[pc] + 1;
                        continue;
                    }
                    0
                }
                Op::End => {
                    let begin = node.src[1];
                    let range = &self.program.nodes[begin];
                    let again = if matches!(self.program.nodes[range.src[0]].op, Op::Noop) {
                        node.src.get(2).is_none_or(|&s| self.values[s] != 0)
                    } else {
                        self.values[begin] = self.values[begin]
                            .checked_add(1)
                            .ok_or(Error::Contract("HCQ loop counter overflow"))?;
                        self.values[begin] < self.values[range.src[0]]
                    };
                    if again {
                        pc = begin + 1;
                        continue;
                    }
                    0
                }
                Op::Noop => 0,
            };
            self.values[pc] = node.normalize(value);
            pc += 1;
        }
        Ok(())
    }
}
