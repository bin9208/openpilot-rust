//! Fused SiLU arithmetic from retained elementwise.sigmoid and xexp2.
//! Tinygrad MIT source provenance is recorded with the independent controls.
//! Use the matrix semantic DAG so LLVM sees the original shared reduction,
//! polynomial, signed casts and grouped outputs without a storage barrier.
use crate::compiler_backend_cpu::image::Ir;

fn literal(value: f32) -> String {
    format!("0x{:016X}", f64::from(value).to_bits())
}
impl Ir {
    fn arithmetic(&mut self, op: &str, a: &str, b: &str) -> String {
        self.op(format!("{op} nsz arcp contract afn float {a}, {b}"))
    }
    fn compare(&mut self, predicate: &str, a: &str, b: &str) -> String {
        self.op(format!(
            "fcmp nsz arcp contract afn {predicate} float {a}, {b}"
        ))
    }
    fn select(&mut self, condition: &str, yes: &str, no: &str) -> String {
        self.op(format!("select i1 {condition}, float {yes}, float {no}"))
    }
    fn matrix_exp2(&mut self, d: &str) -> String {
        // Source _lazy_map_numbers, rintk and ldexp2k, including negative odd q.
        let nan = self.compare("une", d, d);
        let not_negative_infinity = self.compare("une", d, "0xFFF0000000000000");
        let not_positive_infinity = self.compare("une", d, "0x7FF0000000000000");
        let finite = self.select(&not_negative_infinity, d, "0.0");
        let finite = self.select(&nan, "0.0", &finite);
        let finite = self.select(&not_positive_infinity, &finite, "0.0");
        let negative = self.compare("olt", &finite, "0.0");
        let rounding = self.select(&negative, "-0.5", "0.5");
        let rounded = self.arithmetic("fadd", &finite, &rounding);
        let q = self.op(format!("fptosi float {rounded} to i32"));
        let odd = self.op(format!("srem i32 {q}, 2"));
        let negative_q = self.op(format!("icmp slt i32 {q}, 0"));
        let odd = self.op(format!("icmp ne i32 {odd}, 0"));
        let negative_odd = self.op(format!("and i1 {odd}, {negative_q}"));
        let negative_odd = self.signed_op(format!("zext i1 {negative_odd} to i32"));
        let q_float = self.op(format!("sitofp i32 {q} to float"));
        let adjustment = self.signed_op(format!("select i1 {negative_q}, i32 1, i32 0"));
        let adjusted = self.op(format!("add nsw i32 {q}, {adjustment}"));
        let adjustment = self.op(format!("mul i32 {negative_odd}, -1"));
        let lo = self.op(format!("ashr i32 {adjusted}, 1"));
        let lo = self.op(format!("add nsw i32 {lo}, {adjustment}"));
        let negative_lo = self.op(format!("mul i32 {lo}, -1"));
        let hi = self.op(format!("add nsw i32 {q}, {negative_lo}"));
        let hi = self.op(format!("add nsw i32 {hi}, 127"));
        let hi = self.op(format!("shl i32 {hi}, 23"));
        let hi = self.op(format!("bitcast i32 {hi} to float"));
        let lo = self.op(format!("add nsw i32 {lo}, 127"));
        let lo = self.op(format!("shl i32 {lo}, 23"));
        let lo = self.op(format!("bitcast i32 {lo} to float"));
        let minus_q = self.arithmetic("fmul", &q_float, "-1.0");
        let s = self.arithmetic("fadd", &finite, &minus_q);
        let coefficients = [
            f32::from_bits(0x39210d90),
            f32::from_bits(0x3aaf8a33),
            f32::from_bits(0x3c1d966e),
            f32::from_bits(0x3d63579c),
            f32::from_bits(0x3e75fdec),
            f32::from_bits(0x3f317218),
            f32::from_bits(0x3f800000),
        ];
        let mut polynomial = literal(coefficients[0]);
        for coefficient in &coefficients[1..] {
            let product = self.arithmetic("fmul", &polynomial, &s);
            polynomial = self.arithmetic("fadd", &product, &literal(*coefficient));
        }
        let scaled = self.arithmetic("fmul", &polynomial, &lo);
        let scaled = self.arithmetic("fmul", &scaled, &hi);
        let below = self.compare("olt", d, "-150.0");
        let below_upper = self.compare("olt", d, "128.0");
        let scaled = self.select(&below_upper, &scaled, "0x7FF0000000000000");
        let scaled = self.select(&below, "0.0", &scaled);
        self.select(&nan, "0x7FF8000000000000", &scaled)
    }
    pub(super) fn matrix_silu(&mut self, value: &str) -> String {
        let exponent = self.arithmetic("fmul", value, &literal(f32::from_bits(0xbfb8aa3b)));
        let exp = self.matrix_exp2(&exponent);
        let denominator = self.arithmetic("fadd", "1.0", &exp);
        self.arithmetic("fdiv", value, &denominator)
    }
}
