//! Explicit configured LLVM row sum-of-squares, software sqrt and zero clamp.
//! Admission and graph fusion are separate. The reduction follows the retained
//! generic UNROLL<=32 / UNROLL4-or-scalar policy for contiguous rows.
mod math;
use std::fmt::Write;

pub struct RowNorm {
    elements: usize,
}
pub struct Artifact {
    pub source: String,
    pub bytes: Vec<usize>,
    pub scalars: usize,
    pub workers: usize,
}
#[derive(Default)]
struct Ir {
    text: String,
    next: usize,
}
impl Ir {
    fn op(&mut self, expression: impl AsRef<str>) -> String {
        let name = format!("%r{}", self.next);
        self.next += 1;
        writeln!(self.text, "  {name} = {}", expression.as_ref()).unwrap();
        name
    }
    fn reduce(&mut self, base: &str, count: usize, initial: Option<String>) -> String {
        let mut groups = Vec::new();
        let mut offset = 0;
        while offset < count {
            let lanes = if count - offset >= 4 {
                4
            } else if count - offset >= 2 {
                2
            } else {
                1
            };
            groups.push((offset, lanes));
            offset += lanes;
        }
        // Source devectorization emits scalar tail loads before STACK vector
        // loads while retaining ascending logical order in the reduce DAG.
        groups.sort_by_key(|(_, lanes)| usize::from(*lanes != 1));
        let mut loaded = Vec::new();
        for (offset, lanes) in groups {
            let index = if offset == 0 {
                base.into()
            } else {
                self.op(format!("add nsw i32 {base}, {offset}"))
            };
            let pointer = self.op(format!(
                "getelementptr inbounds float, float* %input, i32 {index}"
            ));
            let value = if lanes == 1 {
                self.op(format!("load float, float* {pointer}"))
            } else {
                self.op(format!(
                    "load <{lanes} x float>, <{lanes} x float>* {pointer}"
                ))
            };
            loaded.push((offset, lanes, value));
        }
        let mut values = Vec::new();
        for (offset, lanes, value) in loaded {
            for lane in 0..lanes {
                let scalar = if lanes == 1 {
                    value.clone()
                } else {
                    self.op(format!(
                        "extractelement <{lanes} x float> {value}, i32 {lane}"
                    ))
                };
                values.push((offset + lane, scalar));
            }
        }
        values.sort_by_key(|(offset, _)| *offset);
        let mut accumulator = initial;
        for (_, value) in values {
            let product = self.op(format!("fmul nsz arcp contract afn float {value}, {value}"));
            accumulator = Some(match accumulator {
                None => product,
                Some(previous) => self.op(format!(
                    "fadd nsz arcp contract afn float {previous}, {product}"
                )),
            });
        }
        accumulator.unwrap()
    }
}
impl RowNorm {
    pub fn new(elements: usize) -> Result<Self, String> {
        if elements < 2 || elements > i32::MAX as usize / 4 {
            return Err("row norm requires bounded contiguous non-scalar f32 storage".into());
        }
        Ok(Self { elements })
    }
    pub fn artifact(&self, function: &str, entry: &str) -> Result<Artifact, String> {
        let identifier = |name: &str| {
            !name.is_empty()
                && name.bytes().enumerate().all(|(i, b)| {
                    b.is_ascii_alphabetic() || b == b'_' || i > 0 && b.is_ascii_digit()
                })
        };
        if !identifier(function) || !identifier(entry) || function == entry {
            return Err("invalid row normalization LLVM identifiers".into());
        }
        let mut ir = Ir::default();
        writeln!(ir.text, "define void @{function}(float* noalias align 32 %out, float* noalias align 32 %input) #0 {{").unwrap();
        let sum = if self.elements <= 32 {
            ir.reduce("0", self.elements, None)
        } else {
            let lanes = if self.elements.is_multiple_of(4) {
                4
            } else {
                1
            };
            ir.text.push_str("  %acc = alloca float\n  store float 0.0, float* %acc\n  br label %loop\nloop:\n  %index = phi i32 [0, %0], [%next, %loop]\n  %initial = load float, float* %acc\n");
            let accumulated = ir.reduce("%index", lanes, Some("%initial".into()));
            writeln!(ir.text,"  store float {accumulated}, float* %acc\n  %next = add nsw i32 %index, {lanes}\n  %more = icmp ult i32 %next, {}\n  br i1 %more, label %loop, label %done\ndone:",self.elements).unwrap();
            ir.op("load float, float* %acc")
        };
        ir.text.push_str(
            &math::SQRT_ZERO_CLAMP
                .replace("%sum", &sum)
                .replace("%destination", "%out"),
        );
        ir.text.push_str("  ret void\n}\nattributes #0 = { noinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }\n");
        writeln!(ir.text,"define void @{entry}(i8** %buffers, i32* %scalars) {{\n  %o = load i8*, i8** %buffers\n  %out = bitcast i8* %o to float*\n  %s = getelementptr inbounds i8*, i8** %buffers, i32 1\n  %i = load i8*, i8** %s\n  %input = bitcast i8* %i to float*\n  call void @{function}(float* %out, float* %input)\n  ret void\n}}").unwrap();
        Ok(Artifact {
            source: ir.text,
            bytes: vec![4, self.elements * 4],
            scalars: 0,
            workers: 1,
        })
    }
}

/// Source final f32 Div boundary, with one invariant norm scalar. Graph callers
/// must separately prove the original storage and complete reader ownership.
pub(super) fn division_artifact(
    elements: usize,
    function: &str,
    entry: &str,
) -> Result<Artifact, String> {
    RowNorm::new(elements)?;
    let mut ir = Ir::default();
    writeln!(ir.text,"define void @{function}(float* noalias align 32 %out, float* noalias align 32 %input, float* noalias align 32 %norm) #0 {{").unwrap();
    let scalar = ir.op("load float, float* %norm");
    let lanes = if elements.is_multiple_of(4) { 4 } else { 1 };
    let groups = elements / lanes;
    let base = if groups > 1 {
        ir.text
            .push_str("  br label %loop\nloop:\n  %group = phi i32 [0, %0], [%next, %loop]\n");
        ir.op(format!("mul i32 %group, {lanes}"))
    } else {
        "0".into()
    };
    let ptr = ir.op(format!(
        "getelementptr inbounds float, float* %input, i32 {base}"
    ));
    let vector = if lanes == 4 {
        ir.op(format!("load <4 x float>, <4 x float>* {ptr}"))
    } else {
        ir.op(format!("load float, float* {ptr}"))
    };
    let mut values = Vec::new();
    for lane in 0..lanes {
        let value = if lanes == 4 {
            ir.op(format!("extractelement <4 x float> {vector}, i32 {lane}"))
        } else {
            vector.clone()
        };
        values.push(ir.op(format!(
            "fdiv nsz arcp contract afn float {value}, {scalar}"
        )));
    }
    let pointer = ir.op(format!(
        "getelementptr inbounds float, float* %out, i32 {base}"
    ));
    if lanes == 4 {
        let mut vector = "poison".to_owned();
        for (lane, value) in values.iter().enumerate() {
            vector = ir.op(format!(
                "insertelement <4 x float> {vector}, float {value}, i32 {lane}"
            ));
        }
        writeln!(
            ir.text,
            "  store <4 x float> {vector}, <4 x float>* {pointer}"
        )
        .unwrap();
    } else {
        writeln!(ir.text, "  store float {}, float* {pointer}", values[0]).unwrap();
    }
    if groups > 1 {
        writeln!(ir.text,"  %next = add i32 %group, 1\n  %more = icmp ult i32 %next, {groups}\n  br i1 %more, label %loop, label %done\ndone:").unwrap();
    }
    ir.text.push_str("  ret void\n}\nattributes #0 = { noinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }\n");
    writeln!(ir.text,"define void @{entry}(i8** %buffers, i32* %scalars) {{\n  %s0 = getelementptr inbounds i8*, i8** %buffers, i32 0\n  %r0 = load i8*, i8** %s0\n  %p0 = bitcast i8* %r0 to float*\n  %s1 = getelementptr inbounds i8*, i8** %buffers, i32 1\n  %r1 = load i8*, i8** %s1\n  %p1 = bitcast i8* %r1 to float*\n  %s2 = getelementptr inbounds i8*, i8** %buffers, i32 2\n  %r2 = load i8*, i8** %s2\n  %p2 = bitcast i8* %r2 to float*\n  call void @{function}(float* %p0,float* %p1,float* %p2)\n  ret void\n}}").unwrap();
    Ok(Artifact {
        source: ir.text,
        bytes: vec![elements * 4, elements * 4, 4],
        scalars: 0,
        workers: 1,
    })
}

/// Original packed-half Div output for a following closed paired-reader chain.
/// The loop and conversion retain the source DAG; padding is never read or
/// compared as defined tensor data.
pub(super) fn packed_division_artifact(
    elements: usize,
    function: &str,
    entry: &str,
) -> Result<Artifact, String> {
    RowNorm::new(elements)?;
    if !elements.is_multiple_of(4) {
        return Err("packed normalized matrix edge requires aligned channels".into());
    }
    let layout = super::matrix::MatrixLayout::new(&[1, elements], &[elements, 4])?;
    let mut ir = super::image::Ir::default();
    ir.parameters(&[("%out", 6, 0), ("%input", 6, 1), ("%norm", 6, 2)]);
    writeln!(ir.text,"define void @{function}(i16* noalias align 32 %out, float* noalias align 32 %input, float* noalias align 32 %norm) #0 {{").unwrap();
    let norm = ir.op("load float, float* %norm");
    let groups = elements / 4;
    let group = if groups > 1 {
        ir.begin_indexed_loop("divide", groups, vec![0, 0])
    } else {
        "0".into()
    };
    let base = ir.op(format!("mul i32 {group}, 4"));
    let pointer = ir.op(format!(
        "getelementptr inbounds float, float* %input, i32 {base}"
    ));
    let vector = ir.op(format!("load <4 x float>, <4 x float>* {pointer}"));
    for lane in 0..4 {
        let value = ir.op(format!("extractelement <4 x float> {vector}, i32 {lane}"));
        let divided = ir.op(format!("fdiv nsz arcp contract afn float {value}, {norm}"));
        let half = ir.plain_half(&divided);
        let index = ir.op(format!("add nsw i32 {base}, {lane}"));
        let pointer = ir.op(format!(
            "getelementptr inbounds i16, i16* %out, i32 {index}"
        ));
        ir.store_i16(&pointer, &half);
    }
    if groups > 1 {
        ir.end_loop("divide");
    }
    ir.line("ret void");
    ir.text.push_str(
        "}\nattributes #0 = { noinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }\n",
    );
    writeln!(ir.text,"define void @{entry}(i8** %buffers, i32* %scalars) {{\n  %s0 = getelementptr inbounds i8*, i8** %buffers, i32 0\n  %r0 = load i8*, i8** %s0\n  %p0 = bitcast i8* %r0 to i16*\n  %s1 = getelementptr inbounds i8*, i8** %buffers, i32 1\n  %r1 = load i8*, i8** %s1\n  %p1 = bitcast i8* %r1 to float*\n  %s2 = getelementptr inbounds i8*, i8** %buffers, i32 2\n  %r2 = load i8*, i8** %s2\n  %p2 = bitcast i8* %r2 to float*\n  call void @{function}(i16* %p0,float* %p1,float* %p2)\n  ret void\n}}").unwrap();
    Ok(Artifact {
        source: ir.text,
        bytes: vec![layout.input_elements * 2, elements * 4, 4],
        scalars: 0,
        workers: 1,
    })
}
