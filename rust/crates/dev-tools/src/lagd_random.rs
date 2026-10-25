//! NumPy 2.5.3 SeedSequence/PCG64 and the binary64 distributions used by lagd QA.
//! Translation of NumPy dd88c0c19b54ad9ed3533224221285bf0873249a; upstream
//! copyright and licenses are retained in lagd_random/LICENSE-*. No runtime
//! candidate or original-output fixture participates in generation.
mod tables;

const MULTIPLIER: u128 = ((2_549_297_995_355_413_924u128) << 64) | 4_865_540_595_714_422_341;

fn hashmix(mut value: u32, constant: &mut u32) -> u32 {
    value ^= *constant;
    *constant = constant.wrapping_mul(0x931e_8875);
    value = value.wrapping_mul(*constant);
    value ^ (value >> 16)
}
fn mix(x: u32, y: u32) -> u32 {
    let value = 0xca01_f9ddu32
        .wrapping_mul(x)
        .wrapping_sub(0x4973_f715u32.wrapping_mul(y));
    value ^ (value >> 16)
}
/// Default four-word SeedSequence pool and generate_state(4, uint64).
/// Scalar seeds are restricted by this Rust API to u64; both checker seeds fit.
fn seed_words(seed: u64) -> [u64; 4] {
    let entropy = [seed as u32, (seed >> 32) as u32];
    let entropy_len = if entropy[1] == 0 { 1 } else { 2 };
    let mut constant = 0x43b0_d7e5;
    let mut pool = [0; 4];
    for (i, value) in pool.iter_mut().enumerate() {
        *value = hashmix(if i < entropy_len { entropy[i] } else { 0 }, &mut constant);
    }
    for source in 0..4 {
        for destination in 0..4 {
            if source != destination {
                pool[destination] = mix(pool[destination], hashmix(pool[source], &mut constant));
            }
        }
    }
    // u64 entropy has at most two words, so the upstream remaining-entropy loop
    // (starting at pool_size=4) is empty. Spawn keys are not part of this API.
    let mut output = [0u32; 8];
    constant = 0x8b51_f9dd;
    for (i, value) in output.iter_mut().enumerate() {
        let mut word = pool[i % 4] ^ constant;
        constant = constant.wrapping_mul(0x58f3_8ded);
        word = word.wrapping_mul(constant);
        *value = word ^ (word >> 16);
    }
    std::array::from_fn(|i| u64::from(output[2 * i]) | (u64::from(output[2 * i + 1]) << 32))
}

pub struct Generator {
    state: u128,
    increment: u128,
}
impl Generator {
    pub fn new(seed: u64) -> Self {
        let words = seed_words(seed);
        let state = (u128::from(words[0]) << 64) | u128::from(words[1]);
        let sequence = (u128::from(words[2]) << 64) | u128::from(words[3]);
        let mut generator = Self {
            state: 0,
            increment: sequence.wrapping_shl(1) | 1,
        };
        generator.step();
        generator.state = generator.state.wrapping_add(state);
        generator.step();
        generator
    }
    pub fn state(&self) -> (u128, u128) {
        (self.state, self.increment)
    }
    fn step(&mut self) {
        self.state = self
            .state
            .wrapping_mul(MULTIPLIER)
            .wrapping_add(self.increment);
    }
    pub fn raw(&mut self) -> u64 {
        self.step();
        let high = (self.state >> 64) as u64;
        (high ^ self.state as u64).rotate_right((high >> 58) as u32)
    }
    pub fn random(&mut self) -> f64 {
        (self.raw() >> 11) as f64 * (1. / 9_007_199_254_740_992.)
    }
    pub fn uniform(&mut self, lower: f64, upper: f64) -> f64 {
        lower + (upper - lower) * self.random()
    }
    pub fn normal(&mut self) -> f64 {
        loop {
            let raw = self.raw();
            let index = (raw & 255) as usize;
            let rest = raw >> 8;
            let magnitude = (rest >> 1) & 0x000f_ffff_ffff_ffff;
            let mut x = magnitude as f64 * f64::from_bits(tables::WI[index]);
            if rest & 1 != 0 {
                x = -x;
            }
            if magnitude < tables::KI[index] {
                return x;
            }
            if index == 0 {
                loop {
                    // NumPy uses log1p(-U) so U=0 never calls log(0).
                    let xx = -tables::INV_R * (-self.random()).ln_1p();
                    let yy = -(-self.random()).ln_1p();
                    if yy + yy > xx * xx {
                        let tail = tables::R + xx;
                        return if (magnitude >> 8) & 1 != 0 {
                            -tail
                        } else {
                            tail
                        };
                    }
                }
            } else {
                let current = f64::from_bits(tables::FI[index]);
                let previous = f64::from_bits(tables::FI[index - 1]);
                if (previous - current) * self.random() + current < (-0.5 * x * x).exp() {
                    return x;
                }
            }
        }
    }
}
