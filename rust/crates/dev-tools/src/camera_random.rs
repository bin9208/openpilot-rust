//! MT19937 and the integer-seed/random-bit protocol used by CPython Random.
//! Independent implementation of the published MT19937 recurrence and CPython's
//! documented random() 53-bit construction, checked against original captures.
pub struct Random {
    words: [u32; 624],
    index: usize,
}
impl Random {
    pub fn new(seed: u32) -> Self {
        let mut result = Self {
            words: [0; 624],
            index: 624,
        };
        result.words[0] = 19650218;
        for i in 1..624 {
            result.words[i] = 1812433253u32
                .wrapping_mul(result.words[i - 1] ^ (result.words[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        let mut i = 1;
        for _ in 0..624 {
            result.words[i] = (result.words[i]
                ^ (result.words[i - 1] ^ (result.words[i - 1] >> 30)).wrapping_mul(1664525))
            .wrapping_add(seed);
            i += 1;
            if i == 624 {
                result.words[0] = result.words[623];
                i = 1;
            }
        }
        for _ in 0..623 {
            result.words[i] = (result.words[i]
                ^ (result.words[i - 1] ^ (result.words[i - 1] >> 30)).wrapping_mul(1566083941))
            .wrapping_sub(i as u32);
            i += 1;
            if i == 624 {
                result.words[0] = result.words[623];
                i = 1;
            }
        }
        result.words[0] = 0x80000000;
        result
    }
    fn word(&mut self) -> u32 {
        if self.index == 624 {
            for i in 0..624 {
                let y = (self.words[i] & 0x80000000) | (self.words[(i + 1) % 624] & 0x7fffffff);
                self.words[i] = self.words[(i + 397) % 624]
                    ^ (y >> 1)
                    ^ if y & 1 == 0 { 0 } else { 0x9908b0df };
            }
            self.index = 0;
        }
        let mut y = self.words[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c5680;
        y ^= (y << 15) & 0xefc60000;
        y ^ (y >> 18)
    }
    pub fn random(&mut self) -> f64 {
        let a = self.word() >> 5;
        let b = self.word() >> 6;
        (f64::from(a) * 67108864.0 + f64::from(b)) / 9007199254740992.0
    }
    /// This checker needs at most 64 random bits; word order is little endian.
    pub fn getrandbits(&mut self, count: u32) -> u64 {
        assert!(count <= 64);
        match count {
            0 => 0,
            1..=32 => u64::from(self.word() >> (32 - count)),
            _ => u64::from(self.word()) | (u64::from(self.word() >> (64 - count)) << 32),
        }
    }
    /// Callers validate a positive range, preserving rejection bit consumption.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0);
        let bits = 64 - n.leading_zeros();
        loop {
            let value = self.getrandbits(bits);
            if value < n {
                return value;
            }
        }
    }
}
